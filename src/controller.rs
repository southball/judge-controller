use std::borrow::Borrow;
use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use reqwest::Response;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use super::cli::Opts;
use std::process::{Command, Stdio};

#[derive(Debug, Serialize, Deserialize)]
struct JWTTokenPair {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiSuccess<T> {
    success: bool,
    data: T,
}

struct Session {
    base_url: Url,
    expiry: DateTime<Utc>,
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JWTClaims {
    exp: i64,
}

impl Session {
    fn new(base_url: &str) -> Session {
        Session {
            base_url: Url::parse(base_url).unwrap(),
            expiry: Utc.timestamp(0, 0),
            access_token: String::new(),
            refresh_token: String::new(),
        }
    }

    fn resolve(&self, url_fragment: Vec<&str>) -> Url {
        url_fragment.iter().fold(
            self.base_url.clone(),
            |url, fragment| url.join(fragment).unwrap())
    }

    fn resolve_single(&self, url_fragment: &str) -> Url {
        self.resolve(vec![url_fragment])
    }

    async fn init(&mut self, username: &str, password: &str) {
        let client = reqwest::Client::new();
        let body = {
            let mut map = HashMap::new();
            map.insert("username", username);
            map.insert("password", password);
            map
        };
        let response = client
            .post(self.resolve_single("auth/login"))
            .json(&body)
            .send()
            .await.unwrap()
            .json::<ApiSuccess<JWTTokenPair>>()
            .await.unwrap();

        self.access_token = String::from(response.data.access_token);
        self.refresh_token = String::from(response.data.refresh_token);

        self.recalc_expiry();
    }

    /// Recompute expiry time from `self.access_token`.
    fn recalc_expiry(&mut self) {
        let token_message = jsonwebtoken::dangerous_unsafe_decode::<JWTClaims>(&self.access_token).unwrap();
        self.expiry = Utc.timestamp(token_message.claims.exp, 0);
    }

    async fn refresh(&mut self) {
        let client = reqwest::Client::new();
        let body = {
            let mut map = HashMap::new();
            map.insert("refresh_token", &self.refresh_token);
            map
        };
        let response = client
            .post(self.resolve_single("auth/refresh"))
            .json(&body)
            .send()
            .await.unwrap()
            .json::<ApiSuccess<JWTTokenPair>>()
            .await.unwrap();

        self.access_token = String::from(response.data.access_token);
        self.recalc_expiry();
    }

    async fn get_access_token(&mut self) -> &str {
        if Utc::now() + Duration::minutes(5) > self.expiry {
            self.refresh().await;
        }

        &self.access_token
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PartialSubmission {
    id: i32,
    problem_slug: String,
    language: String,
    source_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProblemMetadata {
    #[serde(rename = "type")]
    problem_type: String,
    last_update: String,
}

async fn write_stream_to_file<'a, T>(stream: &mut T, path: &'a std::path::Path) -> Result<(), Box<dyn std::error::Error>>
    where T: Stream<Item=reqwest::Result<bytes::Bytes>> + std::marker::Unpin
{
    let mut file = std::fs::File::create(path).unwrap();
    while let Some(Ok(item)) = stream.next().await {
        file.write(&item)?;
    }
    file.flush()?;
    Ok(())
}

async fn unzip<'a>(zip_path: &'a std::path::Path, folder_path: &'a std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Extracting {} to {}...", zip_path.to_str().unwrap(), folder_path.to_str().unwrap());

    let zip_file = std::fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(zip_file).unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let filename = file.sanitized_name();
        let target = folder_path.join(&filename);

        if filename.ends_with("/") {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                if !p.exists() { std::fs::create_dir_all(&p)?; }
            }
            let mut sink = std::fs::File::create(&target).unwrap();
            std::io::copy(&mut file, &mut sink)?;
        }
    }

    Ok(())
}

/// Using the reqwest client `client` provided, download file from `url` to `path` using the passed
/// `access_token`.
async fn download_to_file<'a>(
    client: &reqwest::Client, url: Url, path: &'a std::path::Path, access_token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client.get(url)
        .bearer_auth(access_token)
        .send()
        .await.unwrap()
        .bytes_stream();

    let mut file = std::fs::File::create(path).unwrap();
    while let Some(Ok(item)) = stream.next().await {
        file.write(&item)?;
    }
    file.flush()?;

    Ok(())
}

pub async fn process_submission(opts: &Opts, submission_id: i32) -> Result<(), Box<dyn std::error::Error>> {
    let mut session = Session::new(&opts.server);
    session.init(&opts.username, &opts.password).await;

    let client = reqwest::Client::new();
    let submission_id_str = submission_id.to_string();

    log::info!("Getting submission...");
    let submission: PartialSubmission = client.get(session.resolve(vec!["submission/", &submission_id_str]))
        .bearer_auth(session.get_access_token().await)
        .send()
        .await.unwrap()
        .json::<ApiSuccess<PartialSubmission>>()
        .await.unwrap()
        .data;
    log::debug!("Submission: {:?}", submission);

    log::info!("Getting problem...");
    let problem: ProblemMetadata = client.get(session.resolve(vec!["problem/", &submission.problem_slug]))
        .bearer_auth(session.get_access_token().await)
        .send()
        .await.unwrap()
        .json::<ApiSuccess<ProblemMetadata>>()
        .await.unwrap()
        .data;
    log::debug!("Problem: {:?}", problem);

    let is_problem_interactive = &problem.problem_type == "interactive";

    let problem_base_url = session.resolve(vec!["problem/", &format!("{}/", &submission.problem_slug)]);
    let metadata_url = problem_base_url.join("metadata").unwrap();
    let testcases_url = problem_base_url.join("testcases").unwrap();
    let checker_url = problem_base_url.join("checker").unwrap();
    let interactor_url = problem_base_url.join("interactor").unwrap();
    let testlib_url = session.resolve_single("admin/testlib.h");

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

    std::fs::write(&source_path, &submission.source_code);

    let should_download = {
        if !resource_folder.exists() { true }
        else {
            let last_download_str = std::fs::read_to_string(resource_folder.join("last-update-time.txt"))?;
            let last_download: DateTime<Utc> = DateTime::from_str(&last_download_str)?;
            let last_update: DateTime<Utc> = DateTime::from_str(&problem.last_update)?;

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
        std::fs::write(resource_folder.join("last-update-time.txt"), Utc::now().to_rfc3339())?;

        // Download metadata, checker and testlib.h
        download_to_file(&client, metadata_url, &metadata_path, session.get_access_token().await).await?;
        download_to_file(&client, checker_url, &checker_path, session.get_access_token().await).await?;
        download_to_file(&client, testlib_url, &testlib_path, session.get_access_token().await).await?;
        if is_problem_interactive {
            download_to_file(&client, interactor_url, &interactor_path, session.get_access_token().await).await?;
        }

        // Download testcases
        download_to_file(&client, testcases_url, &testcases_zip_path, session.get_access_token().await).await?;
        log::info!("Compressed testcases for problem {} downloaded. Extracting...", &submission.problem_slug);

        unzip(&testcases_zip_path, &testcases_path).await?;
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
        "--languages-definition",
        &opts.language_definition,
        "--verdict",
        verdict_path.to_str().unwrap(),
        "--verdict-format",
        "json",
        "-vvv"
    ];

    if is_problem_interactive {
        args.push("--interactor");
        args.push(interactor_path.to_str().unwrap());
    }

    log::info!("Start judging with arguments: {}", args.join(" "));
    let mut child = Command::new(&opts.judge)
        .args(&args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    child.wait()?;

    let verdict: judge_definitions::JudgeOutput = serde_json::from_str(&std::fs::read_to_string(verdict_path).unwrap()).unwrap();
    let response = client
        .put(session.resolve(vec!["submission/", &format!("{}/", submission_id), "judge"]))
        .bearer_auth(session.get_access_token().await)
        .json(&verdict)
        .send()
        .await.unwrap()
        .text()
        .await.unwrap();

    log::info!("Verdict: {}", verdict.verdict);
    log::info!("Push to {}, response: {}", session.resolve(vec!["submission/", &format!("{}/", submission_id), "judge"]), response);
    log::info!("Judging finished.");

    Ok(())
}