use clap::Clap;
use simplelog::LevelFilter;

/// Judge-Controller
/// The controller between Judge-Server and MiniJudge-Rust
#[derive(Clap, Clone)]
#[clap(version = "0.0-alpha.1", author = "Southball")]
pub struct Opts {
    /// The URL to the judge server.
    #[clap(long = "server")]
    pub server: String,

    /// The user of account on judge server.
    #[clap(long = "username")]
    pub username: String,

    /// The password of account on judge server.
    #[clap(long = "password")]
    pub password: String,

    /// The URL to the AMQP server.
    #[clap(long = "amqp-url")]
    pub amqp_url: String,

    /// The folder to store downloaded files.
    #[clap(long = "folder")]
    pub folder: String,

    /// The folder to store temporary files.
    #[clap(long = "temp")]
    pub temp: String,

    /// The path to the minijudge-rust file.
    #[clap(long = "judge")]
    pub judge: String,

    /// The number of sandboxes to use.
    #[clap(long = "sandboxes")]
    pub sandboxes: i32,

    /// The checker language to be passed to the judge.
    #[clap(long = "checker-language")]
    pub checker_language: String,

    /// The file containing the language definitions.
    #[clap(long = "language-definition")]
    pub language_definition: String,

    /// The level of verbosity.
    #[clap(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbosity: i32,

    /// The socket to bind to for TCP connection.
    #[clap(long = "socket")]
    pub socket: Option<String>,

    /// Whether the log should be suppressed. This option overrides the verbose option.
    #[clap(short = "q", long = "quiet")]
    pub quiet: bool,
}

pub fn debug_opts(opts: &Opts) {
    log::debug!("Server: {}", &opts.server);
    log::debug!("Folder: {}", &opts.folder);
}

pub fn calc_log_level(verbosity: i32, quiet: bool) -> LevelFilter {
    if quiet {
        LevelFilter::Off
    } else {
        match verbosity {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        }
    }
}
