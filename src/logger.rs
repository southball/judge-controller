use crate::cli::{self, Opts};

pub fn init_logger(opts: &Opts) {
    // Derive log level from CLI options and construct logger.
    let log_level = cli::calc_log_level(opts.verbosity, opts.quiet);

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{:5}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Error)
        .level_for("judge_controller", log_level)
        .chain(std::io::stdout())
        // .chain(fern::log_file("output.log")?)
        .apply()
        .unwrap();

    log::info!("Initialized logger from options.");
}
