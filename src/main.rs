#![feature(async_closure)]
use std::{fs, path::Path};

use clap::derive::Clap;
use futures_executor::LocalPool;
use lapin::{
    Connection, ConnectionProperties, options::*, types::FieldTable,
};
use simplelog::{CombinedLogger, Config, TerminalMode, TermLogger};
use tokio::prelude::*;

use cli::*;

mod cli;
mod controller;

#[tokio::main]
async fn main() -> () {
    let opts: Opts = Opts::parse();

    // Derive log level from CLI options and construct logger.
    let log_level = cli::calc_log_level(opts.verbosity, opts.quiet);
    CombinedLogger::init(
        vec![
            TermLogger::new(log_level, Config::default(), TerminalMode::Mixed).unwrap()
        ]
    ).unwrap();

    debug_opts(&opts);

    log::debug!("Preparing controller process...");

    log::debug!("Creating folder.");
    fs::create_dir_all(Path::new(&opts.folder)).unwrap();
    fs::create_dir_all(Path::new(&opts.temp)).unwrap();

    log::debug!("Starting controller process...");
    // let mut executor = LocalPool::new();
    // executor.run_until(async {
    loop {
        let conn = Connection::connect(&opts.amqp_url, ConnectionProperties::default())
            .await
            .expect("Connection error.");

        log::info!("Connected to AMQP server.");

        let channel = conn.create_channel().await.expect("Failed to create channel.");
        channel.basic_qos(1, BasicQosOptions::default()).await.expect("Failed to set prefetch count.");
        let queue = channel.queue_declare("JUDGE_QUEUE", QueueDeclareOptions {
            durable: true,
            ..QueueDeclareOptions::default()
        }, FieldTable::default());

        log::info!("Starting consumer...");
        let consumer = channel.basic_consume("JUDGE_QUEUE", "judge-controller", BasicConsumeOptions::default(), FieldTable::default())
            .await
            .expect("Creating consumer failed.");

        for delivery in consumer {
            if let Ok(delivery) = delivery {
                let submission_id = String::from_utf8_lossy(&delivery.data).parse::<i32>()
                    .expect("Failed to parse submission ID.");
                log::info!("Accepted request to process submission {}.", submission_id);

                controller::process_submission(&opts,submission_id).await.unwrap();

                log::info!("Finished processing submission {}. Acknowledging.", submission_id);
                channel.basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                    .await
                    .expect("Basic ACK failed.");
            }
        }
    }
    // })
}
