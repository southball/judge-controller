use crate::cli::Opts;
use std::{fs, path::Path};

pub fn create_folders(opts: &Opts) {
    fs::create_dir_all(Path::new(&opts.folder)).unwrap();
    fs::create_dir_all(Path::new(&opts.temp)).unwrap();

    log::info!("Created folders.");
}
