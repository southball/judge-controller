use futures_util::StreamExt;
use std::io::Write;
use url::Url;

/// Using the reqwest client `client` provided, download file from `url` to `path` using the passed
/// `access_token`.
pub async fn download_to_file<'a>(
    client: &reqwest::Client,
    url: Url,
    path: &'a std::path::Path,
    access_token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .unwrap()
        .bytes_stream();

    let mut file = std::fs::File::create(path).unwrap();
    while let Some(Ok(item)) = stream.next().await {
        file.write(&item)?;
    }
    file.flush()?;

    Ok(())
}
