use crate::api::*;
use crate::cli::Opts;
use crate::net::*;
use crate::session::*;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread;

pub async fn process_submission(
    opts: &Opts,
    submission_id: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut session = Session::new(&opts.server);
    session.init(&opts.username, &opts.password).await;

    let client = reqwest::Client::new();
    let submission_id_str = submission_id.to_string();

    log::info!("Getting submission...");
    let submission: PartialSubmission = client
        .get(session.resolve(vec!["submission/", &submission_id_str]))
        .bearer_auth(session.get_access_token().await)
        .send()
        .await
        .unwrap()
        .json::<ApiSuccess<PartialSubmission>>()
        .await
        .unwrap()
        .data;
    log::debug!("Submission: {:?}", submission);

    log::info!("Getting problem...");
    let problem: ProblemMetadata = client
        .get(session.resolve(vec!["problem/", &submission.problem_slug]))
        .bearer_auth(session.get_access_token().await)
        .send()
        .await
        .unwrap()
        .json::<ApiSuccess<ProblemMetadata>>()
        .await
        .unwrap()
        .data;
    log::debug!("Problem: {:?}", problem);

    let is_problem_interactive = &problem.problem_type == "interactive";

    let problem_base_url =
        session.resolve(vec!["problem/", &format!("{}/", &submission.problem_slug)]);
    let metadata_url = problem_base_url.join("metadata").unwrap();
    let testcases_url = problem_base_url.join("testcases").unwrap();
    let checker_url = problem_base_url.join("checker").unwrap();
    let interactor_url = problem_base_url.join("interactor").unwrap();
    let testlib_url = session.resolve_single("admin/testlib");

    let resource_folder = std::path::PathBuf::from(&opts.folder).join(&submission.problem_slug);
    let temp_folder = std::path::PathBuf::from(&opts.temp);

    let testcases_path = resource_folder.join("testcases");
    let checker_path = resource_folder.join("checker.cpp");
    let interactor_path = resource_folder.join("interactor.cpp");
    let metadata_path = resource_folder.join("metadata.yml");

    let testcases_zip_path = temp_folder.join("testcases.zip");
    let testlib_path = temp_folder.join("testlib.h");
    let source_path = temp_folder.join("source");
    let verdict_path = temp_folder.join("verdict.json");

    std::fs::write(&source_path, &submission.source_code)?;

    if verdict_path.exists() {
        std::fs::remove_file(&verdict_path)?;
    }

    let should_download = {
        if !resource_folder.exists() {
            true
        } else {
            let last_download_str =
                std::fs::read_to_string(resource_folder.join("last-update-time.txt"))?;
            let last_download: DateTime<Utc> = DateTime::from_str(&last_download_str)?;
            let last_update: DateTime<Utc> = DateTime::from_str(&problem.last_update)?;

            log::info!("Last download: {}", last_download);
            log::info!("Last update: {}", last_update);
            last_download < last_update
        }
    };

    if should_download {
        // Delete and recreate folder if exists
        if resource_folder.exists() {
            std::fs::remove_dir_all(&resource_folder)?;
        }
        std::fs::create_dir_all(&resource_folder)?;

        // Download testcases
        std::fs::write(
            resource_folder.join("last-update-time.txt"),
            Utc::now().to_rfc3339(),
        )?;

        // Download metadata, checker and testlib.h
        download_to_file(
            &client,
            metadata_url,
            &metadata_path,
            session.get_access_token().await,
        )
        .await?;
        download_to_file(
            &client,
            checker_url,
            &checker_path,
            session.get_access_token().await,
        )
        .await?;
        download_to_file(
            &client,
            testlib_url,
            &testlib_path,
            session.get_access_token().await,
        )
        .await?;
        if is_problem_interactive {
            download_to_file(
                &client,
                interactor_url,
                &interactor_path,
                session.get_access_token().await,
            )
            .await?;
        }

        // Download testcases
        download_to_file(
            &client,
            testcases_url,
            &testcases_zip_path,
            session.get_access_token().await,
        )
        .await?;
        log::info!(
            "Compressed testcases for problem {} downloaded. Extracting...",
            &submission.problem_slug
        );

        crate::util::unzip(&testcases_zip_path, &testcases_path).await?;
        std::fs::remove_file(&testcases_zip_path)?;
        log::info!("Extracted testcases.");
    }

    // Start the judging process.
    let sandboxes_count_str = opts.sandboxes.to_string();
    let mut args: Vec<&str> = vec![
        "--metadata",
        metadata_path.to_str().unwrap(),
        "--language",
        &submission.language,
        "--source",
        source_path.to_str().unwrap(),
        "--checker",
        checker_path.to_str().unwrap(),
        "--testcases",
        testcases_path.to_str().unwrap(),
        "--testlib",
        testlib_path.to_str().unwrap(),
        "--sandboxes",
        &sandboxes_count_str,
        "--checker-language",
        &opts.checker_language,
        "--languages-definition",
        &opts.language_definition,
        "--verdict",
        verdict_path.to_str().unwrap(),
        "--verdict-format",
        "json",
        "-vv",
    ];

    if let Some(socket) = &opts.socket {
        args.push("--socket");
        args.push(socket);
    }

    if is_problem_interactive {
        args.push("--interactor");
        args.push(interactor_path.to_str().unwrap());
    }

    // Launch TCP listening server
    let socket = opts.socket.clone();
    let tcp_listener_thread = if let Some(socket) = socket {
        let socket = socket.clone();
        let session = session.clone();

        log::debug!("Spawning TCP listener thread...");

        Some(thread::spawn(move || {
            let context = zmq::Context::new();
            let requester = context.socket(zmq::SUB).unwrap();

            requester
                .connect(&socket)
                .expect("Failed to connect to socket.");
            requester
                .set_subscribe(b"")
                .expect("Failed to set subscription.");

            let mut judged_testcases: i32 = 0;
            let mut prev_request_instant = std::time::Instant::now();
            // TODO use correct total_testcases
            let total_testcases: i32 = problem.testcases.len() as i32;

            let mut msg = zmq::Message::new();
            loop {
                requester.recv(&mut msg, 0).unwrap();
                println!("Received message: {}", msg.as_str().unwrap());

                let value: serde_json::Value = serde_json::from_str(msg.as_str().unwrap()).unwrap();
                let event_type = value["event_type"].as_str().unwrap();

                if event_type == "testcase" {
                    // One submission received.
                    judged_testcases += 1;

                    // TODO set cooldown (e.g. 1s) for status update
                    let mut session = session.clone();
                    let client = reqwest::Client::new();

                    if judged_testcases < total_testcases
                        && prev_request_instant.elapsed() > std::time::Duration::from_secs(1)
                    {
                        prev_request_instant = std::time::Instant::now();

                        let mut rt = tokio::runtime::Runtime::new().unwrap();
                        let local = tokio::task::LocalSet::new();
                        local.block_on(&mut rt, async move {
                            let _response = client
                                .put(session.resolve(vec![
                                    "submission/",
                                    &format!("{}/", submission_id),
                                    "judge/progress",
                                ]))
                                .bearer_auth(session.get_access_token().await)
                                .json(&json!({
                                    "progress": judged_testcases,
                                    "total": total_testcases,
                                }))
                                .send()
                                .await;
                        });
                    }
                }

                if event_type == "submission" {
                    // The judging is completed and the thread should terminate.
                    break;
                }
            }

            ()
        }))
    } else {
        None
    };

    log::info!("Start judging with arguments: {}", args.join(" "));
    let mut child = Command::new(&opts.judge)
        .args(&args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    child.wait()?;
    if let Some(thread) = tcp_listener_thread {
        thread.join().unwrap();
    }

    let verdict: judge_definitions::JudgeOutput;
    if verdict_path.exists() {
        verdict = serde_json::from_str(&std::fs::read_to_string(verdict_path).unwrap()).unwrap();
    } else {
        verdict = judge_definitions::JudgeOutput {
            verdict: judge_definitions::verdicts::VERDICT_SE.into(),
            compile_message: "".to_string(),
            time: 0.,
            memory: 0,
            testcases: vec![],
        };
    }

    let response = client
        .put(session.resolve(vec!["submission/", &format!("{}/", submission_id), "judge"]))
        .bearer_auth(session.get_access_token().await)
        .json(&verdict)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    log::info!("Verdict: {}", verdict.verdict);
    log::info!(
        "Push to {}, response: {}",
        session.resolve(vec!["submission/", &format!("{}/", submission_id), "judge"]),
        response
    );
    log::info!("Judging finished.");

    Ok(())
}
