//! Public video discovery and cookie-free, non-overwriting media downloads.
use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use url::Url;

fn clean_url(input: &str) -> Result<Url> {
    let url = Url::parse(input).context("invalid URL")?;
    ensure!(url.scheme() == "https", "only HTTPS URLs are supported");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL must not contain credentials"
    );
    ensure!(url.port().is_none_or(|p| p == 443), "unexpected URL port");
    Ok(url)
}

fn note_host(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("xiaohongshu.com" | "www.xiaohongshu.com" | "m.xiaohongshu.com")
    )
}

fn share_host(url: &Url) -> bool {
    matches!(url.host_str(), Some("xhslink.cn" | "xhslink.com"))
}

/// Parse a full note URL without fetching its page.
pub fn note_url(input: &str) -> Result<(String, String)> {
    let url = clean_url(input)?;
    ensure!(note_host(&url), "expected Xiaohongshu note URL");
    let segments: Vec<_> = url.path_segments().context("missing note path")?.collect();
    let id = match segments.as_slice() {
        ["explore", id] | ["discovery", "item", id] => *id,
        _ => bail!("expected /explore/ID or /discovery/item/ID note URL"),
    };
    ensure!(
        !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid note ID"
    );
    let token = url
        .query_pairs()
        .find(|(key, _)| key == "xsec_token")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    Ok((id.to_owned(), token))
}

fn public_client() -> Result<Client> {
    // Deliberately separate from the signed API client. No cookies or authentication headers.
    Ok(Client::builder()
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// Resolve an ID, full note URL, or official share link. Share requests never receive session cookies.
pub fn resolve_note(input: &str) -> Result<(String, String)> {
    if !input.contains("://") {
        ensure!(
            !input.is_empty() && input.bytes().all(|b| b.is_ascii_alphanumeric()),
            "invalid note ID"
        );
        return Ok((input.to_owned(), String::new()));
    }
    let client = public_client()?;
    resolve_note_with(input, |url| {
        let response = client
            .get(url.clone())
            .header("user-agent", "Mozilla/5.0")
            .send()
            .context("resolve public share link")?;
        ensure!(
            response.status().is_redirection(),
            "share link did not redirect (HTTP {})",
            response.status()
        );
        Ok(response
            .headers()
            .get(reqwest::header::LOCATION)
            .context("share redirect missing Location")?
            .to_str()?
            .to_owned())
    })
}

/// Follow official share redirects through a caller-supplied, unauthenticated transport.
pub fn resolve_note_with(
    input: &str,
    mut redirect: impl FnMut(&Url) -> Result<String>,
) -> Result<(String, String)> {
    let mut url = clean_url(input)?;
    for _ in 0..5 {
        if note_host(&url) {
            return note_url(url.as_str());
        }
        ensure!(
            share_host(&url),
            "refusing redirect outside official Xiaohongshu share domains"
        );
        let location = redirect(&url)?;
        url = clean_url(url.join(&location)?.as_str())?;
    }
    bail!("too many share redirects")
}

/// Accept only public Xiaohongshu CDN resources; normalize legacy HTTP URLs to HTTPS.
pub fn media_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input).context("invalid media URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "unsupported media URL scheme"
    );
    ensure!(url.port().is_none(), "unexpected media URL port");
    url.set_scheme("https")
        .map_err(|_| anyhow::anyhow!("invalid media URL scheme"))?;
    let url = clean_url(url.as_str())?;
    ensure!(
        url.host_str()
            .is_some_and(|h| h == "xhscdn.com" || h.ends_with(".xhscdn.com")),
        "refusing media outside Xiaohongshu CDN"
    );
    Ok(url)
}

/// Select a complete MP4 including sound, preferring H.264/EF4 over other codecs.
pub fn select_video(feed: &Value, id: &str) -> Result<Value> {
    let feed = feed.get("data").unwrap_or(feed);
    let item = feed["items"]
        .as_array()
        .context("feed missing items")?
        .iter()
        .find(|item| item["id"] == id || item["note_card"]["note_id"] == id)
        .context("feed did not return requested note")?;
    let note = &item["note_card"];
    ensure!(note["type"] == "video", "note is not a video");
    let video = &note["video"];
    let parsed;
    let media = if video["media"]["stream"].is_object() {
        &video["media"]
    } else {
        parsed = serde_json::from_str::<Value>(
            video["media_v2"]
                .as_str()
                .context("video missing media streams")?,
        )?;
        &parsed
    };
    let streams = media["stream"]
        .as_object()
        .context("video missing streams")?;
    let mut candidates: Vec<&Value> = streams
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter(|s| {
            s["format"]
                .as_str()
                .is_some_and(|f| f.eq_ignore_ascii_case("mp4"))
                && s["audio_channels"].as_u64().unwrap_or(0) > 0
                && s["master_url"]
                    .as_str()
                    .is_some_and(|u| media_url(u).is_ok())
        })
        .collect();
    candidates.sort_by_key(|s| {
        let codec = s["video_codec"].as_str().unwrap_or("").to_ascii_lowercase();
        (
            matches!(codec.as_str(), "ef4" | "h264" | "avc" | "avc1"),
            s["width"]
                .as_u64()
                .unwrap_or(0)
                .saturating_mul(s["height"].as_u64().unwrap_or(0)),
            s["avg_bitrate"].as_u64().unwrap_or(0),
        )
    });
    let stream = candidates.last().context(
        "no complete MP4 with audio found; fragmented or silent streams require separate muxing",
    )?;
    let url = media_url(stream["master_url"].as_str().unwrap())?;
    let backups: Vec<_> = stream["backup_urls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|s| media_url(s).ok())
        .map(|u| u.to_string())
        .collect();
    Ok(
        json!({"note_id":id,"title":note["title"],"url":url.as_str(),"backup_urls":backups,"format":stream["format"],"codec":stream["video_codec"],"audio_codec":stream["audio_codec"],"width":stream["width"],"height":stream["height"],"duration_ms":stream["duration"],"size":stream["size"],"chapters":video["consumer"]["chapters"]}),
    )
}

fn fetch_media(client: &Client, url: &Url) -> Result<Response> {
    let mut url = url.clone();
    for _ in 0..5 {
        let response = client
            .get(url.clone())
            .header("user-agent", "Mozilla/5.0")
            .header("referer", "https://www.xiaohongshu.com/")
            .send()
            .context("download video")?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("media redirect missing Location")?
                .to_str()?;
            url = media_url(url.join(location)?.as_str())?;
        } else {
            return response
                .error_for_status()
                .context("video CDN returned error");
        }
    }
    bail!("too many media redirects")
}

/// Download to a new file only. Errors never publish a partial output.
pub fn download_video(url: &str, output: &Path) -> Result<u64> {
    ensure!(
        !output.exists(),
        "output already exists: {}",
        output.display()
    );
    let response = fetch_media(&public_client()?, &media_url(url)?)?;
    let length = response.content_length();
    write_video(response, length, output)
}

/// Stream and validate an MP4 before atomically exposing its final path.
pub fn write_video(
    mut input: impl Read,
    expected_length: Option<u64>,
    output: &Path,
) -> Result<u64> {
    ensure!(
        !output.exists(),
        "output already exists: {}",
        output.display()
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .context("output must name a file")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{:032x}.part", rand::random::<u128>()));
    let result = (|| -> Result<u64> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .context("create temporary video file")?;
        let mut header = [0u8; 12];
        input
            .read_exact(&mut header)
            .context("video response is empty or truncated")?;
        ensure!(&header[4..8] == b"ftyp", "response is not an MP4 video");
        file.write_all(&header)?;
        let size =
            12 + std::io::copy(&mut input, &mut file).context("video download interrupted")?;
        ensure!(
            expected_length.is_none_or(|n| n == size),
            "video download size mismatch"
        );
        file.sync_all()?;
        drop(file);
        // hard_link is atomic and fails if the destination appeared during the download.
        fs::hard_link(&temporary, output)
            .context("publish video without overwriting existing file")?;
        Ok(size)
    })();
    let _ = fs::remove_file(&temporary);
    result
}
