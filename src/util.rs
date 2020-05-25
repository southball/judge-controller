use futures_util::stream::{Stream, StreamExt};
use std::io::Write;

pub async fn write_stream_to_file<'a, T>(
    stream: &mut T,
    path: &'a std::path::Path,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Stream<Item = reqwest::Result<bytes::Bytes>> + std::marker::Unpin,
{
    let mut file = std::fs::File::create(path).unwrap();
    while let Some(Ok(item)) = stream.next().await {
        file.write(&item)?;
    }
    file.flush()?;
    Ok(())
}

/// Unzip the zip file at zip_path to folder at folder_path.
pub async fn unzip<'a>(
    zip_path: &'a std::path::Path,
    folder_path: &'a std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
        "Extracting {} to {}...",
        zip_path.to_str().unwrap(),
        folder_path.to_str().unwrap()
    );

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
                if !p.exists() {
                    std::fs::create_dir_all(&p)?;
                }
            }
            let mut sink = std::fs::File::create(&target).unwrap();
            std::io::copy(&mut file, &mut sink)?;
        }
    }

    Ok(())
}
