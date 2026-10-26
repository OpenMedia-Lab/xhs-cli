//! Creator video contracts checked against the official web publisher on 2026-10-26.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

/// Reserve once, then atomically update. An interrupted update cannot destroy
/// the last synced payload needed to reconcile an uncertain publication.
pub struct Receipt {
    path: std::path::PathBuf,
}
impl Receipt {
    pub fn create(path: &std::path::Path, value: &Value) -> Result<Self> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).with_context(|| {
            format!(
                "receipt already exists or cannot be created: {}; inspect it before retrying",
                path.display()
            )
        })?;
        file.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new("."));
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(Self {
            path: path.to_owned(),
        })
    }
    pub fn update(&self, value: &Value) -> Result<()> {
        use std::io::Write;
        replace_receipt(&self.path, |file| {
            file.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
            Ok(())
        })
    }
}
fn replace_receipt(
    path: &std::path::Path,
    write: impl FnOnce(&mut std::fs::File) -> Result<()>,
) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let name = path
        .file_name()
        .context("receipt path must name a file")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{:032x}.tmp", rand::random::<u128>()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
            .context("atomically replace receipt; previous receipt retained if rename fails")?;
        #[cfg(unix)]
        {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn upload_success(status: u16) -> Result<()> {
    ensure!(
        (200..300).contains(&status),
        "material upload did not complete (HTTP {status}); redirects are not followed"
    );
    Ok(())
}

pub fn processing_response(mut data: Value, video_id: &str) -> Result<Value> {
    ensure!(data.is_object(), "processing response must be an object");
    data["video_id"] = json!(video_id);
    Ok(data)
}

/// Candidates only: the caller must inspect the actual note before reconciling.
pub fn publication_candidates(list: &Value, title: &str) -> Result<Vec<Value>> {
    Ok(list["notes"]
        .as_array()
        .context("creator list missing notes")?
        .iter()
        .filter_map(|v| {
            let actual = v["display_title"].as_str().or_else(|| v["title"].as_str());
            (actual == Some(title)).then(|| json!({"id":v["id"],"title":actual,"type":v["type"]}))
        })
        .collect())
}

/// A conservative native-cover surface; the browser renders its main cover to
/// JPEG. The 32 MiB bound is local, not a claim about the server's main-cover cap.
pub fn video_cover_info(bytes: &[u8]) -> Result<Value> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 32 * 1024 * 1024,
        "native video cover must be nonempty and at most 32 MiB"
    );
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        bail!("native video cover must be JPEG or PNG");
    };
    let size = imagesize::blob_size(bytes).context("read video cover dimensions")?;
    ensure!(
        size.width > 0 && size.height > 0,
        "invalid video cover dimensions"
    );
    Ok(json!({"width":size.width,"height":size.height,"mime":mime}))
}

fn validate_spectrum_file_id(file_id: &str) -> Result<()> {
    ensure!(
        file_id.starts_with("spectrum/")
            && file_id.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))),
        "invalid spectrum file ID"
    );
    Ok(())
}

/// The official uploader lets x-ros-* response headers override permit metadata.
/// Retain both names locally; aliases never go into the note API payload.
pub fn upload_file_identity(permit_id: &str, canonical: Option<&str>) -> Result<Value> {
    validate_spectrum_file_id(permit_id)?;
    let canonical = canonical.unwrap_or(permit_id);
    validate_spectrum_file_id(canonical)?;
    let mut aliases = vec![permit_id];
    if canonical != permit_id {
        aliases.push(canonical);
    }
    Ok(json!({"file_id":canonical,"upload_file_id":permit_id,"file_id_aliases":aliases}))
}

/// Fingerprint the actual locally approved cover, without storing its bytes.
pub fn cover_fingerprint(bytes: &[u8]) -> Result<Value> {
    let mut info = video_cover_info(bytes)?;
    info["md5"] = json!(format!("{:x}", md5::compute(bytes)));
    info["sha256"] = json!(format!("{:x}", Sha256::digest(bytes)));
    info["bytes"] = json!(bytes.len());
    Ok(info)
}

pub fn original_cover_url(input: &str) -> Result<url::Url> {
    let url = url::Url::parse(input).context("invalid original cover URL")?;
    ensure!(url.scheme() == "https", "original cover requires HTTPS");
    ensure!(
        url.host_str()
            .is_some_and(|host| host == "xhscdn.com" || host.ends_with(".xhscdn.com")),
        "original cover must use the Xiaohongshu CDN"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.fragment().is_none(),
        "unexpected original cover URL credentials, port or fragment"
    );
    Ok(url)
}

/// HTTP 206 and redirects are not complete original files. Bound reads even
/// when the CDN omits Content-Length, then require the advertised byte count.
pub fn read_original_cover_response(
    status: u16,
    content_length: Option<u64>,
    input: impl Read,
) -> Result<Vec<u8>> {
    const LIMIT: u64 = 32 * 1024 * 1024;
    ensure!(
        status == 200,
        "original cover requires complete HTTP 200; got {status}; redirects are not followed"
    );
    ensure!(
        content_length.is_none_or(|n| n > 0 && n <= LIMIT),
        "original cover length is empty or exceeds 32 MiB"
    );
    let mut bytes = Vec::new();
    input
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .context("read complete original cover")?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= LIMIT,
        "original cover is empty or exceeds 32 MiB"
    );
    ensure!(
        content_length.is_none_or(|n| n == bytes.len() as u64),
        "original cover response is truncated or has a different byte count"
    );
    Ok(bytes)
}

/// Separate unauthenticated client: never send API cookies, signatures or auth.
pub fn fetch_original_cover(input: &str) -> Result<Vec<u8>> {
    let url = original_cover_url(input)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()?;
    let response = client
        .get(url)
        .send()
        .context("fetch original cover without authentication")?;
    read_original_cover_response(
        response.status().as_u16(),
        response.content_length(),
        response,
    )
}

pub fn verify_readback_with_material(
    payload: &Value,
    detail: &Value,
    material: Option<&Value>,
    mut fetch_cover: impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    let submitted_video = payload["video_info"]["file_id"]
        .as_str()
        .context("payload video file ID missing")?;
    let canonical_video = detail["video_info"]["file_id"]
        .as_str()
        .context("readback video file ID missing")?;
    let submitted_cover = payload["video_info"]["cover"]["file_id"]
        .as_str()
        .context("payload cover file ID missing")?;
    let canonical_cover = detail["cover"]
        .get("file_id")
        .or_else(|| detail["cover"].get("fileid"))
        .and_then(Value::as_str)
        .context("readback cover file ID missing")?;
    let video_changed = submitted_video != canonical_video;
    let cover_changed = submitted_cover != canonical_cover;
    let mut identity = json!({
        "video":{"method":"file_id_exact","submitted_file_id":submitted_video,"canonical_file_id":canonical_video},
        "cover":{"method":"file_id_exact","submitted_file_id":submitted_cover,"canonical_file_id":canonical_cover}
    });

    // Only this temporary verification copy may use server-canonical IDs.
    // It is never submitted or written back over the publication journal.
    let mut verification_payload = payload.clone();
    verification_payload["video_info"]["file_id"] = json!(canonical_video);
    verification_payload["video_info"]["cover"]["file_id"] = json!(canonical_cover);
    let mut report = verify_readback(&verification_payload, detail)?;
    if video_changed || cover_changed {
        let material =
            material.context("changed media IDs require the original material receipt")?;
        ensure!(
            material["upload_complete"] == true,
            "material receipt is incomplete"
        );
        ensure!(
            material["file_id"] == submitted_video,
            "payload video alias differs from the saved material receipt"
        );
        ensure!(
            material["cover"]["file_id"] == submitted_cover,
            "payload cover alias differs from the saved material receipt"
        );
        validate_spectrum_file_id(canonical_video)?;
        validate_spectrum_file_id(canonical_cover)?;
        if video_changed {
            let source = &detail["video_info"]["media_v2"]["video"];
            let expected_md5 = digest_field(material, "md5", 32)?;
            let actual_md5 = digest_field(source, "md5", 32)?;
            ensure!(
                expected_md5.eq_ignore_ascii_case(actual_md5),
                "original video MD5 differs from material receipt"
            );
            let expected_video_id = material["video_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("material video ID missing")?;
            ensure!(
                detail["video_info"]["media_v2"]["video_id"] == expected_video_id,
                "original video ID differs from material receipt"
            );
            let (width, height) = positive_dimensions(material)?;
            ensure!(
                positive_dimensions(source)? == (width, height)
                    && positive_dimensions(&detail["video_info"])? == (width, height),
                "original video dimensions differ from material receipt"
            );
            ensure!(
                payload["video_info"]["format_width"] == width
                    && payload["video_info"]["format_height"] == height,
                "payload video dimensions differ from material receipt"
            );
            let submitted_duration = positive_duration(material)?;
            let actual_duration = positive_duration(source)?;
            ensure!(
                (actual_duration - submitted_duration).abs() <= 1.0,
                "original video duration differs from material receipt by more than one second"
            );
            identity["video"] = json!({"method":"original_video_md5_video_id_dimensions_duration","submitted_file_id":submitted_video,"canonical_file_id":canonical_video,"md5":actual_md5,"video_id":expected_video_id,"width":width,"height":height,"submitted_duration_seconds":submitted_duration,"platform_original_duration_seconds":actual_duration});
        }
        if cover_changed {
            let cover = &material["cover"];
            let (width, height) = positive_dimensions(cover)?;
            ensure!(
                positive_dimensions(&detail["cover"])? == (width, height),
                "readback cover dimensions differ from material receipt"
            );
            ensure!(
                payload["video_info"]["cover"]["width"] == width
                    && payload["video_info"]["cover"]["height"] == height,
                "payload cover dimensions differ from material receipt"
            );
            let expected_sha256 = digest_field(cover, "sha256", 64)?;
            let expected_md5 = digest_field(cover, "md5", 32)?;
            let expected_bytes = cover["bytes"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 32 * 1024 * 1024)
                .context("material cover byte count missing or outside bounds")?;
            let url = original_cover_url(
                detail["cover"]["original"]
                    .as_str()
                    .context("readback original cover URL missing")?,
            )?;
            let bytes = fetch_cover(url.as_str())?;
            let fingerprint = cover_fingerprint(&bytes)?;
            ensure!(
                fingerprint["bytes"] == expected_bytes,
                "original cover byte count differs from material receipt"
            );
            ensure!(
                digest_field(&fingerprint, "sha256", 64)?.eq_ignore_ascii_case(expected_sha256),
                "original cover SHA-256 differs from material receipt"
            );
            ensure!(
                digest_field(&fingerprint, "md5", 32)?.eq_ignore_ascii_case(expected_md5),
                "original cover MD5 differs from material receipt"
            );
            ensure!(
                positive_dimensions(&fingerprint)? == (width, height),
                "original cover file dimensions differ from material receipt"
            );
            identity["cover"] = json!({"method":"original_cover_sha256_bytes_md5_dimensions","submitted_file_id":submitted_cover,"canonical_file_id":canonical_cover,"source":"creator_detail.cover.original","host":url.host_str(),"sha256":fingerprint["sha256"],"md5":fingerprint["md5"],"bytes":expected_bytes,"width":width,"height":height});
        }
    }
    report["media_identity"] = identity;
    report["attestation"] = json!({"original_payload_preserved":true,"canonical_ids_resolved_from_material_evidence":video_changed || cover_changed});
    Ok(report)
}

fn digest_field<'a>(value: &'a Value, field: &str, length: usize) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|text| text.len() == length && text.bytes().all(|c| c.is_ascii_hexdigit()))
        .with_context(|| format!("missing or invalid original {field} fingerprint"))
}

fn positive_dimensions(value: &Value) -> Result<(u64, u64)> {
    let width = value["width"]
        .as_u64()
        .filter(|n| *n > 0)
        .context("positive media width missing")?;
    let height = value["height"]
        .as_u64()
        .filter(|n| *n > 0)
        .context("positive media height missing")?;
    Ok((width, height))
}

fn positive_duration(value: &Value) -> Result<f64> {
    value["duration"]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .context("positive original video duration missing")
}

/// Both ROS channels are advertised by the current official creator uploader.
pub fn video_upload_target(address: &str, file_id: &str) -> Result<url::Url> {
    ensure!(
        file_id.starts_with("spectrum/")
            && file_id.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))),
        "invalid video upload file ID"
    );
    let address = if address.starts_with("http://") || address.starts_with("https://") {
        address.to_owned()
    } else {
        format!("https://{address}")
    };
    let mut url = url::Url::parse(&address)?;
    let host = url.host_str().context("upload address missing host")?;
    ensure!(
        host.starts_with("ros-upload")
            && (host.ends_with(".xiaohongshu.com") || host.ends_with(".xhscdn.com")),
        "unsupported video upload host: {host}"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.port().is_none(),
        "unexpected upload URL credentials/query/port"
    );
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "unsupported upload scheme"
    );
    url.set_scheme("https")
        .map_err(|_| anyhow::anyhow!("invalid upload scheme"))?;
    url.set_path(&format!("/{file_id}"));
    Ok(url)
}

/// Convert the current official campaign record, preserving unrelated business binds.
/// Campaign linkage does not establish moderation or reward eligibility.
pub fn bind_activity(binds: &mut Value, activity: &Value, time_ms: u64) -> Result<Vec<Value>> {
    ensure!(binds.is_object(), "business binds must be an object");
    let id = activity["activity_id"]
        .as_u64()
        .context("activity missing numeric activity_id")?
        .to_string();
    let name = activity["activity_name"]
        .as_str()
        .context("activity missing name")?;
    let start = activity["start_time"]
        .as_u64()
        .context("activity missing start_time")?;
    let end = activity["end_time"]
        .as_u64()
        .context("activity missing end_time")?;
    ensure!(
        activity["activity_status"] == 1 && time_ms >= start && time_ms <= end,
        "activity is not currently active"
    );
    let topics = activity["topic_infos"]
        .as_array()
        .context("activity missing topic entities")?
        .iter()
        .map(|v| {
            ensure!(
                v["id"].is_string() && v["name"].is_string(),
                "activity has incomplete topic entity"
            );
            Ok(json!({"id":v["id"],"name":v["name"],"type":"topic"}))
        })
        .collect::<Result<Vec<_>>>()?;
    if binds.get("optionRelationList").is_none() {
        binds["optionRelationList"] = json!([]);
    }
    let relations = binds["optionRelationList"]
        .as_array_mut()
        .context("invalid business relation list")?;
    relations.retain(|v| v["type"] != "ACTIVITY_COMPONENT");
    relations.push(json!({"type":"ACTIVITY_COMPONENT","relationList":[{"bizType":"ACTIVITY_COMPONENT","bizId":id,"extraInfo":json!({"id":id,"name":name,"start_time":start,"end_time":end}).to_string()}]}));
    Ok(topics)
}

pub fn api_response(status: u16, text: &str) -> Result<Value> {
    ensure!(
        !matches!(status, 461 | 471),
        "platform requires verification (HTTP {status})"
    );
    let data: Value = serde_json::from_str(text)
        .with_context(|| format!("non-JSON response (HTTP {status}); outcome may be unknown"))?;
    let code = data
        .get("code")
        .or_else(|| data.get("result").filter(|v| v.is_number()));
    let success = data
        .get("success")
        .and_then(Value::as_bool)
        .or_else(|| data["result"]["success"].as_bool());
    let code_ok = code.is_none_or(|v| v.as_i64() == Some(0));
    let accepted = success != Some(false) && code_ok && (success == Some(true) || code.is_some());
    ensure!(
        (200..300).contains(&status) && accepted,
        "platform error HTTP {status} code={}: {}",
        code.unwrap_or(&Value::Null),
        data.get("msg")
            .or_else(|| data.get("message"))
            .unwrap_or(&Value::Null)
    );
    Ok(data.get("data").cloned().unwrap_or(data))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .context("truncated MP4 metadata")?
            .try_into()?,
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .context("truncated MP4 metadata")?
            .try_into()?,
    ))
}
fn boxes(bytes: &[u8]) -> Result<Vec<(&[u8], &[u8])>> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let length = u32_at(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8).context("truncated MP4 box")?;
        let (size, head) = match length {
            0 => (bytes.len() - at, 8),
            1 => (usize::try_from(u64_at(bytes, at + 8)?)?, 16),
            _ => (length, 8),
        };
        ensure!(
            size >= head && size <= bytes.len() - at,
            "invalid or truncated MP4 box"
        );
        result.push((kind, &bytes[at + head..at + size]));
        at += size;
    }
    Ok(result)
}
fn child<'a>(bytes: &'a [u8], name: &[u8]) -> Result<&'a [u8]> {
    boxes(bytes)?
        .into_iter()
        .find(|(kind, _)| *kind == name)
        .map(|(_, value)| value)
        .with_context(|| format!("MP4 missing {}", String::from_utf8_lossy(name)))
}

/// Read only bounded metadata boxes; upload uses a streaming file body.
pub fn inspect_mp4(reader: &mut (impl Read + Seek), size: u64) -> Result<Value> {
    ensure!(
        size > 0 && size <= 1024 * 1024 * 1024,
        "video must be nonempty and at most 1 GiB"
    );
    let mut at = 0u64;
    let mut moov = None;
    let mut ftyp = false;
    let mut mdat = false;
    while at < size {
        ensure!(size - at >= 8, "truncated MP4 box header");
        reader.seek(SeekFrom::Start(at))?;
        let mut head = [0u8; 8];
        reader.read_exact(&mut head)?;
        let length = u32::from_be_bytes(head[..4].try_into()?);
        let mut header = 8;
        let length = match length {
            0 => size - at,
            1 => {
                let mut wide = [0; 8];
                reader.read_exact(&mut wide)?;
                header = 16;
                u64::from_be_bytes(wide)
            }
            n => n as u64,
        };
        ensure!(
            length >= header && length <= size - at,
            "invalid or truncated MP4 box"
        );
        match &head[4..] {
            b"ftyp" => {
                ensure!(length >= 16, "invalid MP4 file type");
                ftyp = true;
            }
            b"mdat" => {
                ensure!(length > header, "empty MP4 media data");
                mdat = true;
            }
            b"moov" => {
                ensure!(
                    length - header <= 32 * 1024 * 1024,
                    "MP4 metadata exceeds 32 MiB"
                );
                let mut b = vec![0; (length - header) as usize];
                reader.read_exact(&mut b)?;
                moov = Some(b);
            }
            _ => {}
        }
        at += length;
    }
    ensure!(ftyp && mdat, "expected a complete MP4 with ftyp and mdat");
    let moov = moov.context("MP4 missing moov metadata")?;
    let mvhd = child(&moov, b"mvhd")?;
    let (scale, duration) = if mvhd.first() == Some(&1) {
        (u32_at(mvhd, 20)?, u64_at(mvhd, 24)?)
    } else {
        (u32_at(mvhd, 12)?, u32_at(mvhd, 16)? as u64)
    };
    ensure!(scale > 0, "invalid MP4 time scale");
    let seconds = duration as f64 / scale as f64;
    ensure!(
        seconds.is_finite() && seconds > 0.0 && seconds <= 4.0 * 3600.0,
        "video duration must be positive and at most four hours"
    );
    let mut dimensions = None;
    let mut audio = false;
    for (_, track) in boxes(&moov)?
        .into_iter()
        .filter(|(kind, _)| *kind == b"trak")
    {
        let mdia = child(track, b"mdia")?;
        let hdlr = child(mdia, b"hdlr")?;
        let handler = hdlr.get(8..12).context("truncated MP4 handler")?;
        if handler == b"soun" {
            audio = true;
        }
        if handler != b"vide" {
            continue;
        }
        ensure!(
            dimensions.is_none(),
            "multiple video tracks are unsupported"
        );
        let tkhd = child(track, b"tkhd")?;
        ensure!(tkhd.len() >= 84, "truncated MP4 video track");
        let width = u32_at(tkhd, tkhd.len() - 8)? >> 16;
        let height = u32_at(tkhd, tkhd.len() - 4)? >> 16;
        let stsd = child(child(child(mdia, b"minf")?, b"stbl")?, b"stsd")?;
        let codec = stsd.get(12..16).context("missing MP4 video codec")?;
        ensure!(
            codec == b"avc1" || codec == b"avc3",
            "native upload currently requires H.264 MP4; convert HEVC/other codecs first"
        );
        // Rotated MOV/MP4 requires transcode/rotation handling not implemented here.
        let matrix = if tkhd[0] == 1 { 52 } else { 40 };
        ensure!(
            u32_at(tkhd, matrix)? == 65536 && u32_at(tkhd, matrix + 16)? == 65536,
            "rotated MP4 requires baking rotation into pixels before upload"
        );
        ensure!(width > 0 && height > 0, "invalid MP4 video dimensions");
        dimensions = Some((width, height));
    }
    let (width, height) = dimensions.context("MP4 has no video track")?;
    reader.seek(SeekFrom::Start(0))?;
    Ok(
        json!({"width":width,"height":height,"duration":seconds,"bytes":size,"has_audio":audio,"codec":"h264"}),
    )
}

pub fn video_payload(
    title: &str,
    desc: &str,
    privacy: u8,
    topics: &[Value],
    media: &Value,
    binds: &Value,
) -> Result<Value> {
    ensure!(privacy <= 1, "visibility must be public or private");
    ensure!(
        !desc.contains("\\n"),
        "body contains literal \\n; use --desc-file for actual newlines"
    );
    ensure!(
        title.chars().count() <= 20,
        "title must be at most 20 characters"
    );
    ensure!(
        desc.chars().count() <= 1000,
        "description must be at most 1000 characters"
    );
    ensure!(topics.len() <= 10, "at most 10 topics are supported");
    for topic in topics {
        ensure!(
            topic["id"].as_str().is_some_and(|s| !s.is_empty())
                && topic["name"].is_string()
                && topic["type"] == "topic",
            "topic entity requires exact ID, name and type=topic"
        );
    }
    ensure!(
        media["upload_complete"] == true,
        "video upload receipt is incomplete"
    );
    for key in ["file_id", "video_id"] {
        ensure!(
            media[key].as_str().is_some_and(|s| !s.is_empty()),
            "video receipt missing {key}"
        );
    }
    for (parent, key) in [
        (media, "width"),
        (media, "height"),
        (&media["cover"], "width"),
        (&media["cover"], "height"),
    ] {
        ensure!(
            parent[key].as_u64().is_some_and(|n| n > 0),
            "video or cover receipt missing {key}"
        );
    }
    let duration = media["duration"]
        .as_f64()
        .context("video receipt missing duration")?;
    ensure!(
        duration > 0.0 && duration <= 4.0 * 3600.0,
        "invalid video duration"
    );
    let file = media["file_id"].as_str().unwrap();
    let cover = &media["cover"];
    ensure!(
        cover["file_id"].as_str().is_some_and(|s| !s.is_empty()),
        "missing uploaded cover file ID"
    );
    Ok(
        json!({"common":{"type":"video","title":title,"note_id":"","desc":desc,"source":"{\"type\":\"web\",\"ids\":\"\",\"extraInfo\":\"{\\\"subType\\\":\\\"official\\\"}\"}","business_binds":binds.to_string(),"ats":[],"hash_tag":topics,"post_loc":{},"privacy_info":{"op_type":1,"type":privacy,"user_ids":[]}},"image_info":null,"video_info":{"fileid":file,"file_id":file,"format_width":media["width"],"format_height":media["height"],"video_preview_type":if media["width"].as_f64().unwrap()/media["height"].as_f64().unwrap()<=0.6 {"full_vertical_screen"}else{""},"composite_metadata":{"video":{},"audio":{}},"timelines":[],"cover":{"fileid":cover["file_id"],"file_id":cover["file_id"],"width":cover["width"],"height":cover["height"],"stickers":{"neptune":[],"version":2},"fonts":[],"cover_template_id":"","frame":{"ts":0,"is_upload":true,"is_user_select":false},"extra_info_json":"{\"cover_recommend\":{\"isUseRecommend\":false,\"isAiCover\":false}}"},"chapters":[],"summary":"","chapter_sync_text":false,"segments":{"count":1,"need_slice":false,"items":[{"mute":0,"speed":1,"start":0,"duration":duration,"transcoded":0,"media_source":1,"original_metadata":{"video":{},"audio":{}}}]},"entrance":"web"}}),
    )
}

/// A successful HTTP request alone does not establish the resulting visibility or content.
pub fn verify_readback(payload: &Value, detail: &Value) -> Result<Value> {
    let common = &payload["common"];
    for key in ["title", "type"] {
        ensure!(
            detail[key] == common[key],
            "readback {key} differs from submitted payload"
        );
    }
    let submitted_desc = common["desc"]
        .as_str()
        .context("payload description missing")?;
    let actual_desc = detail["desc"]
        .as_str()
        .context("readback description missing")?;
    let description_exact = actual_desc == submitted_desc;
    if !description_exact {
        // Observed creator behavior: runs of consecutive LF become one LF.
        // Preserve every other character, including spaces, CR and edge LF.
        let mut previous_lf = false;
        let collapsed: String = submitted_desc
            .chars()
            .filter(|&c| {
                let keep = c != '\n' || !previous_lf;
                previous_lf = c == '\n';
                keep
            })
            .collect();
        ensure!(
            actual_desc == collapsed,
            "readback desc differs from submitted payload beyond platform-collapsed blank lines"
        );
    }
    ensure!(
        detail["privacy"]["type"] == common["privacy_info"]["type"],
        "readback visibility differs"
    );
    ensure!(
        detail["video_info"]["file_id"] == payload["video_info"]["file_id"],
        "readback video file ID differs"
    );
    let cover = detail["cover"]
        .get("file_id")
        .or_else(|| detail["cover"].get("fileid"));
    ensure!(
        cover == Some(&payload["video_info"]["cover"]["file_id"]),
        "readback cover file ID differs"
    );
    let expected = common["hash_tag"]
        .as_array()
        .context("payload topics missing")?;
    let actual = detail["hash_tag"]
        .as_array()
        .context("readback topics missing")?;
    for topic in expected {
        ensure!(
            actual.iter().any(|v| v["id"] == topic["id"]),
            "readback topic ID missing: {}",
            topic["id"]
        );
    }
    if actual.len() != expected.len() {
        bail!("readback topic entity count differs");
    }
    let requested_binds: Value = serde_json::from_str(
        common["business_binds"]
            .as_str()
            .context("payload business bindings missing")?,
    )?;
    let got_binds = if let Some(text) = detail["business_binds"].as_str() {
        serde_json::from_str::<Value>(text)?
    } else {
        detail["business_binds"].clone()
    };
    if let Some(origin) = requested_binds["userDeclarationBind"]["origin"]
        .as_u64()
        .filter(|origin| *origin > 0)
    {
        ensure!(
            got_binds["userDeclarationBind"]["origin"] == origin,
            "readback content declaration differs or is missing"
        );
    }
    if let Some(activities) = requested_binds["optionRelationList"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v["type"] == "ACTIVITY_COMPONENT"))
    {
        let actual_activities = got_binds["optionRelationList"]
            .as_array()
            .and_then(|v| v.iter().find(|v| v["type"] == "ACTIVITY_COMPONENT"))
            .context("readback activity relation missing")?;
        let expected_id = &activities["relationList"][0]["bizId"];
        ensure!(
            actual_activities["relationList"]
                .as_array()
                .is_some_and(|v| v.iter().any(|r| r["bizId"] == *expected_id)),
            "readback activity ID differs"
        );
    }
    Ok(
        json!({"verified":true,"note_id":detail["id"],"title":detail["title"],"desc":actual_desc,"description_exact":description_exact,"normalization":if description_exact {Value::Null}else{json!("platform_collapsed_blank_lines")},"format_warning":if description_exact {Value::Null}else{json!("Platform collapsed consecutive LF blank lines; description formatting is not byte-for-byte identical.")},"submitted_newline_count":submitted_desc.bytes().filter(|&c|c==b'\n').count(),"actual_newline_count":actual_desc.bytes().filter(|&c|c==b'\n').count(),"topics":actual,"visibility":detail["privacy"]["type"],"video_file_id":detail["video_info"]["file_id"],"cover_file_id":cover,"content_declaration_origin":got_binds["userDeclarationBind"]["origin"]}),
    )
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    #[test]
    fn failed_update_preserves_the_last_synced_payload() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("xhs-receipt-{}", rand::random::<u64>()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("publication.json");
        let original = json!({"status":"submitting","payload":{"common":{"title":"recover me"}}});
        let receipt = Receipt::create(&path, &original).unwrap();
        let error = replace_receipt(&path, |file| {
            file.write_all(b"{broken")?;
            bail!("simulated disk write failure")
        });
        assert!(error.is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
            original
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        receipt
            .update(&json!({"status":"accepted_unverified","payload":original["payload"]}))
            .unwrap();
        assert!(Receipt::create(&path, &json!({})).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
