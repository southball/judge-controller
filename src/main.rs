mod api;
mod cli;
mod controller;
mod logger;
mod net;
mod precheck;
mod session;
mod util;

use clap::derive::Clap;
use cli::Opts;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties};

#[tokio::main]
async fn main() -> () {
    let opts: Opts = Opts::parse();

    logger::init_logger(&opts);
    cli::debug_opts(&opts);
    precheck::create_folders(&opts);

    loop {
        let conn = Connection::connect(&opts.amqp_url, ConnectionProperties::default())
            .await
            .expect("Connection error.");

        log::info!("Connected to AMQP server.");

        let channel = conn
            .create_channel()
            .await
            .expect("Failed to create channel.");
        channel
            .basic_qos(1, BasicQosOptions::default())
            .await
            .expect("Failed to set prefetch count.");
        let _queue = channel.queue_declare(
            "JUDGE_QUEUE",
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        );

        log::info!("Starting consumer...");
        let consumer = channel
            .basic_consume(
                "JUDGE_QUEUE",
                "judge-controller",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Creating consumer failed.");

        for delivery in consumer {
            if let Ok(delivery) = delivery {
                let submission_id = String::from_utf8_lossy(&delivery.data)
                    .parse::<i32>()
                    .expect("Failed to parse submission ID.");
                log::info!("Accepted request to process submission {}.", submission_id);

                // Acknowledge the delivery to prevent it the task from being
                // processed by multiple controller processes.
                channel
                    .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                    .await
                    .expect("Basic ACK failed.");

                controller::process_submission(&opts, submission_id)
                    .await
                    .unwrap();

                log::info!(
                    "Finished processing submission {}. Acknowledging.",
                    submission_id
                );
            }
        }
    }
}
