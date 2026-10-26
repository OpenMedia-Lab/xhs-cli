mod qr_login;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
use xhs_cli::{
    bind_document, creator_sign, document_info, main_sign, now, prepare_images, replace_images,
};
#[derive(Parser)]
#[command(version, about = "Native Rust Xiaohongshu client")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    cookies: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}
#[derive(Clone, Copy, ValueEnum)]
enum Visibility {
    Public,
    Private,
}
impl Visibility {
    fn code(self) -> u8 {
        match self {
            Self::Public => 0,
            Self::Private => 1,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum ContentDeclaration {
    AiSynthesis,
}
#[derive(Subcommand)]
enum Cmd {
    #[command(alias = "status")]
    Me,
    Login {
        #[arg(long, conflicts_with = "qr")]
        cookie: Option<String>,
        /// QR is the default when --cookie is omitted.
        #[arg(long)]
        qr: bool,
        /// Write the official login QR to an SVG file.
        #[arg(long, conflicts_with = "cookie")]
        qr_file: Option<PathBuf>,
        #[arg(long, default_value_t = 240, conflicts_with = "cookie")]
        timeout: u64,
    },
    Topics {
        keyword: String,
    },
    /// Read creator campaigns. Topic-only campaigns do not use an activity component.
    Activities {
        #[arg(long)]
        topic_only: bool,
    },
    Search {
        keyword: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value = "general")]
        sort: String,
    },
    Read {
        note: String,
        #[arg(long, visible_alias = "xsec-token")]
        token: Option<String>,
    },
    /// Inspect video resources, or download a complete MP4 with sound to a new file.
    Media {
        note: String,
        #[arg(long, visible_alias = "xsec-token")]
        token: Option<String>,
        /// Download to this file. Existing files are never overwritten.
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    User {
        id: String,
    },
    #[command(alias = "user-posts")]
    UserNotes {
        id: String,
        #[arg(long, default_value = "")]
        cursor: String,
    },
    MyNotes {
        #[arg(long, default_value_t = 0)]
        page: u32,
        #[arg(long, default_value_t = 0)]
        tab: u32,
    },
    Detail {
        note: String,
    },
    /// Read business bindings, including attached document metadata.
    Bindings {
        note: String,
    },
    /// Upload one supported document and return reusable attachment metadata.
    UploadDocument {
        file: PathBuf,
    },
    /// Upload reusable video/cover assets. This does not create a note or draft.
    UploadVideo {
        file: PathBuf,
        #[arg(long)]
        cover: PathBuf,
        /// New receipt file; retained even when a later upload step fails.
        #[arg(long)]
        receipt: PathBuf,
    },
    /// Query a previously uploaded video's processing status without publishing.
    VideoStatus {
        #[arg(long)]
        video_info: PathBuf,
    },
    /// Reconcile a publication or edit receipt with creator detail; never mutates notes.
    CheckPublish {
        #[arg(long)]
        receipt: PathBuf,
        /// Explicit candidate ID when an uncertain result is missing its ID.
        #[arg(long)]
        note: Option<String>,
    },
    #[command(alias = "post")]
    Publish {
        #[arg(long)]
        title: String,
        #[arg(
            long,
            visible_alias = "body",
            required_unless_present = "desc_file",
            conflicts_with = "desc_file"
        )]
        desc: Option<String>,
        /// UTF-8 body with actual newlines, read without escape substitution.
        #[arg(long)]
        desc_file: Option<PathBuf>,
        #[arg(long,visible_alias="images",required_unless_present_any=["video","video_info"],conflicts_with_all=["video","video_info"],num_args=1..)]
        image: Vec<PathBuf>,
        /// H.264 MP4. Use upload-video first to reuse a material receipt.
        #[arg(long, conflicts_with = "video_info", requires = "cover")]
        video: Option<PathBuf>,
        /// JSON receipt returned by upload-video.
        #[arg(long, conflicts_with = "cover")]
        video_info: Option<PathBuf>,
        #[arg(long, requires = "video")]
        cover: Option<PathBuf>,
        /// New journal file, required for video publication. Prevents blind retries.
        #[arg(long)]
        receipt: Option<PathBuf>,
        /// Resolve topics and print the payload using --video-info; create no note.
        #[arg(long, requires = "video_info", conflicts_with = "video")]
        dry_run: bool,
        #[arg(long, conflicts_with = "private")]
        visibility: Option<Visibility>,
        #[arg(long)]
        private: bool,
        #[arg(long)]
        topic: Vec<String>,
        /// Link an active official creator campaign by numeric ID.
        #[arg(long)]
        activity: Option<u64>,
        /// Native content-type declaration; does not add text or hashtags.
        #[arg(long)]
        declaration: Option<ContentDeclaration>,
        /// JSON metadata returned by upload-document.
        #[arg(long)]
        attachment: Option<PathBuf>,
    },
    Edit {
        note: String,
        /// Replace document using metadata returned by upload-document.
        #[arg(long)]
        attachment: Option<PathBuf>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, visible_alias = "body")]
        desc: Option<String>,
        /// UTF-8 replacement body, preserving actual newlines.
        #[arg(long, conflicts_with = "desc")]
        desc_file: Option<PathBuf>,
        /// Reusable uploaded video and explicit cover receipt; replaces this note's video.
        #[arg(long, conflicts_with = "image")]
        video_info: Option<PathBuf>,
        /// Preview the exact original-ID PUT payload using read-only requests.
        #[arg(long, conflicts_with = "image")]
        dry_run: bool,
        /// New durable single-PUT journal; required for non-preview video replacement.
        #[arg(long)]
        receipt: Option<PathBuf>,
        #[arg(long)]
        visibility: Option<Visibility>,
        #[arg(long,visible_alias="images",num_args=1..)]
        image: Vec<PathBuf>,
    },
    Delete {
        note: String,
    },
}
struct Client {
    http: reqwest::blocking::Client,
    cookies: Value,
}
const EDITH: &str = "https://edith.xiaohongshu.com";
const CREATOR: &str = "https://creator.xiaohongshu.com";
impl Client {
    fn new(path: Option<PathBuf>) -> Result<Self> {
        let p = path.unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".xiaohongshu-cli/cookies.json")
        });
        let cookies: Value = serde_json::from_str(
            &fs::read_to_string(&p).with_context(|| format!("read session {}", p.display()))?,
        )?;
        ensure!(cookies["a1"].is_string(), "session missing a1 cookie");
        Ok(Self {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(45))
                .retry(reqwest::retry::never())
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            cookies,
        })
    }
    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        creator: bool,
    ) -> Result<Value> {
        let serialized = body.as_ref().map(Value::to_string);
        let timestamp = now();
        let a1 = self.cookies["a1"].as_str().unwrap();
        let (host, sign, common) = if creator {
            (
                if path.starts_with("/api/galaxy/")
                    || path.starts_with("/api/media/v1/upload/creator/")
                {
                    CREATOR
                } else {
                    EDITH
                },
                creator_sign(path, serialized.as_deref(), a1, timestamp),
                None,
            )
        } else {
            let content = format!("{path}{}", serialized.as_deref().unwrap_or(""));
            let (s, c) = main_sign(&content, a1, timestamp);
            (EDITH, s, Some(c))
        };
        let cookie = self
            .cookies
            .as_object()
            .context("cookies must be object")?
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}={s}")))
            .collect::<Vec<_>>()
            .join("; ");
        let mut req=self.http.request(method.parse()?,format!("{host}{path}")).header("user-agent","Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36").header("cookie",cookie).header("origin",if creator||path.starts_with("/web_api"){CREATOR}else{"https://www.xiaohongshu.com"}).header("referer",if creator||path.starts_with("/web_api"){"https://creator.xiaohongshu.com/"}else{"https://www.xiaohongshu.com/"}).header("content-type","application/json;charset=UTF-8").header("accept","application/json, text/plain, */*").header("x-s",sign).header("x-t",timestamp.to_string());
        if path == "/api/qrcode/userinfo" {
            req = req.header("service-tag", "webcn");
        }
        if let Some(c) = common {
            req = req
                .header("x-s-common", c)
                .header("x-b3-traceid", format!("{:016x}", rand::random::<u64>()));
        }
        if let Some(s) = serialized {
            req = req.body(s)
        }
        let response = req
            .send()
            .context("request failed; mutation not automatically retried")?;
        let status = response.status();
        ensure!(
            status.as_u16() != 461 && status.as_u16() != 471,
            "platform requires verification (HTTP {status})"
        );
        xhs_cli::publish::api_response(status.as_u16(), &response.text()?)
    }
    fn detail(&self, id: &str) -> Result<Value> {
        self.request(
            "GET",
            &query(
                "/web_api/sns/capa/postgw/note/detail",
                &[
                    ("note_id", id.to_owned()),
                    ("source", "web".into()),
                    ("edit_mode", "1".into()),
                ],
            ),
            None,
            true,
        )
    }
    fn read(&self, note: &str, token: Option<String>) -> Result<(String, Value)> {
        let (id, embedded_token) = xhs_cli::media::resolve_note(note)?;
        let data = self.request("POST", "/api/sns/web/v1/feed", Some(json!({"source_note_id":id,"image_formats":["jpg","webp","avif"],"extra":{"need_body_topic":"1"},"xsec_source":"pc_feed","xsec_token":token.unwrap_or(embedded_token)})), false)?;
        Ok((id, data))
    }
    fn upload_document(&self, path: &std::path::Path) -> Result<Value> {
        let size = fs::metadata(path)?.len();
        ensure!(
            size > 0 && size <= 100 * 1024 * 1024,
            "document must be nonempty and at most 100 MiB"
        );
        let bytes = fs::read(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("document filename must be UTF-8")?;
        let mut document = document_info(name, &bytes)?;
        let format = document["docType"].as_str().unwrap().to_lowercase();
        let permit = self.request(
            "GET",
            &query(
                "/api/media/v1/upload/creator/permit",
                &[
                    ("biz_name", "sns".into()),
                    ("scene", "web_doc".into()),
                    ("file_count", "1".into()),
                    ("file_format", format),
                    ("version", "1".into()),
                    ("source", "web".into()),
                ],
            ),
            None,
            true,
        )?;
        let p = &permit["uploadTempPermits"][0];
        let id = p["fileIds"][0]
            .as_str()
            .context("document permit missing file ID")?;
        let token = p["token"]
            .as_str()
            .context("document permit missing token")?;
        let mime = match document["docType"].as_str().unwrap() {
            "PDF" => "application/pdf",
            "DOC" => "application/msword",
            "DOCX" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "PPT" => "application/vnd.ms-powerpoint",
            _ => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        };
        let response = self
            .http
            .put(format!("https://ros-upload.xiaohongshu.com/{id}"))
            .header("X-Cos-Security-Token", token)
            .header("Content-Type", mime)
            .body(bytes)
            .send()
            .context("document upload failed; not retried")?;
        xhs_cli::publish::upload_success(response.status().as_u16())?;
        let generated = self.request(
            "POST",
            "/api/galaxy/v2/creator/doc/gen_id",
            Some(json!({})),
            true,
        )?;
        document["docId"] = generated
            .get("id")
            .cloned()
            .context("missing generated document ID")?;
        document["fileId"] = json!(id);
        ensure!(
            document["docId"].as_str().is_some_and(|s| !s.is_empty()),
            "invalid generated document ID"
        );
        Ok(document)
    }
    fn upload(&self, path: &std::path::Path, bytes: &[u8]) -> Result<String> {
        let material = self.upload_image(path, bytes)?;
        Ok(material["file_id"].as_str().unwrap().to_owned())
    }
    fn upload_image(&self, path: &std::path::Path, bytes: &[u8]) -> Result<Value> {
        let permit=self.request("GET","/api/media/v1/upload/web/permit?biz_name=spectrum&scene=image&file_count=1&version=1&source=web",None,true)?;
        let p = &permit["uploadTempPermits"][0];
        let id = p["fileIds"][0]
            .as_str()
            .context("upload permit missing file ID")?;
        let token = p["token"].as_str().context("upload permit missing token")?;
        let mime = match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/jpeg",
        };
        let response = self
            .http
            .put(format!("https://ros-upload.xiaohongshu.com/{id}"))
            .header("X-Cos-Security-Token", token)
            .header("Content-Type", mime)
            .body(bytes.to_vec())
            .send()?;
        xhs_cli::publish::upload_success(response.status().as_u16())?;
        let canonical = response
            .headers()
            .get("x-ros-file-id")
            .map(|value| value.to_str())
            .transpose()
            .context("invalid canonical image file ID header")?;
        xhs_cli::publish::upload_file_identity(id, canonical)
    }
    fn video_status(&self, media: &Value) -> Result<Value> {
        let id = if let Some(id) = media["video_id"].as_str() {
            id.to_owned()
        } else {
            self.generated_video_id(
                media["file_id"]
                    .as_str()
                    .context("video receipt missing file_id")?,
            )?
        };
        let status = self.request(
            "GET",
            &query(
                "/web_api/sns/capa/postgw/query_transcode",
                &[
                    ("video_id", id.clone()),
                    ("need_transcode", "false".into()),
                    ("level", "14".into()),
                    ("resource_type", "0".into()),
                ],
            ),
            None,
            true,
        )?;
        xhs_cli::publish::processing_response(status, &id)
    }
    fn generated_video_id(&self, file_id: &str) -> Result<String> {
        let result = self.request(
            "GET",
            &query(
                "/web_api/sns/capa/postgw/videoid",
                &[("file_key", file_id.into()), ("biz_name", "217".into())],
            ),
            None,
            true,
        )?;
        let id = result
            .get("video_id")
            .or_else(|| result.get("videoId"))
            .and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .context("video ID generation returned no ID")?;
        ensure!(!id.is_empty() && id != "-1", "video ID generation failed");
        Ok(id)
    }
    fn upload_video(
        &self,
        path: &std::path::Path,
        cover: &std::path::Path,
        receipt: &std::path::Path,
    ) -> Result<Value> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file =
            fs::File::open(path).with_context(|| format!("read video {}", path.display()))?;
        let size = file.metadata()?.len();
        let mut media = xhs_cli::publish::inspect_mp4(&mut file, size)?;
        // Validate cover before requesting any credentials or uploading video bytes.
        ensure!(
            fs::metadata(cover)?.len() <= 32 * 1024 * 1024,
            "native video cover must be at most 32 MiB"
        );
        let mut cover_bytes = Vec::new();
        fs::File::open(cover)?
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut cover_bytes)?;
        let cover_info = xhs_cli::publish::cover_fingerprint(&cover_bytes)?;
        let mut hash = md5::Context::new();
        let mut buffer = [0; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.consume(&buffer[..n]);
        }
        file.seek(SeekFrom::Start(0))?;
        media["md5"] = json!(format!("{:x}", hash.compute()));
        media["upload_complete"] = json!(false);
        media["status"] = json!("requesting_permit");
        let mut journal = new_receipt(receipt, &media)?;
        let permit = self.request(
            "GET",
            &query(
                "/api/media/v1/upload/creator/permit",
                &[
                    ("biz_name", "spectrum".into()),
                    ("scene", "video".into()),
                    ("file_count", "1".into()),
                    ("version", "1".into()),
                    ("source", "web".into()),
                ],
            ),
            None,
            true,
        )?;
        if let Some(max) = permit["uploadLimitPolicy"]["maxSize"]
            .as_u64()
            .or_else(|| permit["upload_limit_policy"]["max_size"].as_u64())
        {
            ensure!(size <= max, "video exceeds the server upload size limit");
        }
        let permits = permit["uploadTempPermits"]
            .as_array()
            .context("video permit missing channel list")?;
        let (p, target) = permits
            .iter()
            .find_map(|p| {
                let id = p["fileIds"][0].as_str()?;
                let host = p["uploadAddr"].as_str()?;
                xhs_cli::publish::video_upload_target(host, id)
                    .ok()
                    .map(|url| (p, url))
            })
            .context("video permit has no supported ROS upload channel")?;
        let id = p["fileIds"][0]
            .as_str()
            .context("video permit missing file ID")?;
        let token = p["token"].as_str().context("video permit missing token")?;
        media["upload_host"] = json!(target.host_str());
        media["file_id"] = json!(id);
        media["status"] = json!("uploading_video");
        write_receipt(&mut journal, &media)?;
        let response = self
            .http
            .put(target)
            .header("X-Cos-Security-Token", token)
            .header("Content-Type", "video/mp4")
            .header("Content-Length", size)
            .timeout(Duration::from_secs(900))
            .body(file)
            .send()
            .context("video upload failed; use receipt to investigate before re-uploading")?;
        xhs_cli::publish::upload_success(response.status().as_u16())?;
        let header_id = response
            .headers()
            .get("x-ros-video-id")
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        let canonical = response
            .headers()
            .get("x-ros-file-id")
            .map(|value| value.to_str())
            .transpose()
            .context("invalid canonical video file ID header")?;
        for (key, value) in xhs_cli::publish::upload_file_identity(id, canonical)?
            .as_object()
            .unwrap()
        {
            media[key] = value.clone();
        }
        if let Some(id) = &header_id {
            media["video_id"] = json!(id);
        }
        media["status"] = json!("uploaded_video");
        write_receipt(&mut journal, &media)?;
        let video_id = if let Some(id) = header_id {
            id
        } else {
            self.generated_video_id(media["file_id"].as_str().unwrap())?
        };
        ensure!(
            !video_id.is_empty() && video_id != "-1",
            "video ID generation failed"
        );
        media["video_id"] = json!(video_id);
        media["status"] = json!("uploading_cover");
        write_receipt(&mut journal, &media)?;
        let cover_name = std::path::Path::new(if cover_info["mime"] == "image/png" {
            "cover.png"
        } else {
            "cover.jpg"
        });
        let mut cover_material = self.upload_image(cover_name, &cover_bytes)?;
        for (key, value) in cover_info.as_object().unwrap() {
            cover_material[key] = value.clone();
        }
        media["cover"] = cover_material;
        media["upload_complete"] = json!(true);
        media["status"] = json!("uploaded");
        write_receipt(&mut journal, &media)?;
        // A processing failure does not discard successfully uploaded material IDs.
        match self.video_status(&media) {
            Ok(status) => media["processing"] = status,
            Err(e) => media["processing_error"] = json!(e.to_string()),
        }
        write_receipt(&mut journal, &media)?;
        Ok(media)
    }
}
fn new_receipt(path: &std::path::Path, value: &Value) -> Result<xhs_cli::publish::Receipt> {
    xhs_cli::publish::Receipt::create(path, value)
}
fn write_receipt(file: &mut xhs_cli::publish::Receipt, value: &Value) -> Result<()> {
    file.update(value)
}
fn query(path: &str, pairs: &[(&str, String)]) -> String {
    let mut url = url::Url::parse(&format!("{EDITH}{path}")).unwrap();
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in pairs {
            q.append_pair(k, v);
        }
    }
    format!("{}?{}", url.path(), url.query().unwrap_or(""))
}
fn note_ref(input: &str) -> Result<(String, String)> {
    if input.starts_with("https://") {
        let u = url::Url::parse(input)?;
        ensure!(
            u.host_str()
                .is_some_and(|h| h == "xiaohongshu.com" || h.ends_with(".xiaohongshu.com")),
            "expected Xiaohongshu note URL"
        );
        let id = u
            .path_segments()
            .and_then(|mut s| s.next_back())
            .context("missing note ID")?
            .to_owned();
        let token = u
            .query_pairs()
            .find(|(k, _)| k == "xsec_token")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
        Ok((id, token))
    } else {
        ensure!(
            !input.is_empty() && input.bytes().all(|b| b.is_ascii_alphanumeric()),
            "invalid note ID"
        );
        Ok((input.to_owned(), String::new()))
    }
}
fn run(cli: Cli) -> Result<Value> {
    if let Cmd::Edit {
        title,
        desc,
        desc_file,
        video_info,
        dry_run,
        receipt,
        visibility,
        image,
        attachment,
        ..
    } = &cli.command
    {
        ensure!(
            title.is_some()
                || desc.is_some()
                || desc_file.is_some()
                || video_info.is_some()
                || visibility.is_some()
                || !image.is_empty()
                || attachment.is_some(),
            "edit requires --title, --desc, --desc-file, --video-info, --visibility, --image or --attachment"
        );
        ensure!(
            *dry_run || video_info.is_none() || receipt.is_some(),
            "video edit requires --receipt; use --dry-run to preview without updating"
        );
        if !dry_run && let Some(receipt) = receipt {
            ensure!(
                !receipt.exists(),
                "edit receipt already exists; use check-publish before retrying"
            );
        }
    }
    if let Cmd::Login {
        cookie,
        qr_file,
        timeout,
        ..
    } = &cli.command
    {
        return if let Some(cookie) = cookie {
            import_session(cli.cookies, cookie)
        } else {
            qr_login::login(cli.cookies, qr_file.as_deref(), *timeout)
        };
    }
    if let Cmd::Publish {
        video,
        video_info,
        receipt,
        dry_run,
        ..
    } = &cli.command
        && (video.is_some() || video_info.is_some())
    {
        ensure!(
            *dry_run || receipt.is_some(),
            "video publication requires --receipt; use --dry-run with --video-info to preview"
        );
        if !dry_run {
            ensure!(
                !receipt.as_ref().unwrap().exists(),
                "publication receipt already exists; use check-publish before retrying"
            );
        }
    }
    let c = Client::new(cli.cookies)?;
    match cli.command{
 Cmd::Login{..}=>unreachable!(),
 Cmd::Topics{keyword}=>c.request("POST","/web_api/sns/v1/search/topic",Some(json!({"keyword":keyword,"suggest_topic_request":{"title":"","desc":""},"page":{"page_size":20,"page":1}})),true),
 Cmd::Activities{topic_only}=>c.request("GET",&query("/api/galaxy/v2/creator/activity_center/list",&[("sort","1".into()),("type","1".into()),("source","3".into()),("topic_activity",if topic_only {"1"}else{"0"}.into())]),None,true),
 Cmd::Me=>c.request("GET","/api/sns/web/v2/user/me",None,false),
 Cmd::Search{keyword,page,sort}=>c.request("POST","/api/sns/web/v1/search/notes",Some(json!({"keyword":keyword,"page":page,"page_size":20,"search_id":now().to_string(),"sort":sort,"note_type":0,"ext_flags":[],"filters":[{"tags":["general"],"type":"sort_type"},{"tags":["不限"],"type":"filter_note_type"},{"tags":["不限"],"type":"filter_note_time"},{"tags":["不限"],"type":"filter_note_range"},{"tags":["不限"],"type":"filter_pos_distance"}],"geo":"","image_formats":["jpg","webp","avif"]})),false),
 Cmd::Read{note,token}=>c.read(&note,token).map(|(_,data)|data),
 Cmd::Media{note,token,output}=>{
     if let Some(path)=&output { ensure!(!path.exists(),"output already exists: {}",path.display()); }
     let (id,data)=c.read(&note,token)?;
     let mut media=xhs_cli::media::select_video(&data,&id)?;
     if let Some(path)=output {
         let bytes=xhs_cli::media::download_video(media["url"].as_str().unwrap(),&path)?;
         media["download"]=json!({"path":path,"bytes":bytes});
     }
     Ok(media)
 },
 Cmd::User{id}=>c.request("GET",&query("/api/sns/web/v1/user/otherinfo",&[("target_user_id",id)]),None,false),
 Cmd::UserNotes{id,cursor}=>c.request("GET",&query("/api/sns/web/v1/user_posted",&[("num","30".into()),("cursor",cursor),("user_id",id),("image_scenes","FD_WM_WEBP".into())]),None,false),
 Cmd::MyNotes{page,tab}=>c.request("GET",&format!("/api/galaxy/v2/creator/note/user/posted?tab={tab}&page={page}"),None,true),
 Cmd::Detail{note}=>c.detail(&note_ref(&note)?.0),
 Cmd::Bindings{note}=>c.request("GET",&query("/web_api/sns/capa/postgw/note/business_bind",&[("note_id",note_ref(&note)?.0)]),None,true),
 Cmd::UploadDocument{file}=>c.upload_document(&file),
 Cmd::UploadVideo{file,cover,receipt}=>c.upload_video(&file,&cover,&receipt),
 Cmd::VideoStatus{video_info}=>{let media:Value=serde_json::from_slice(&fs::read(video_info)?)?;c.video_status(&media)},
 Cmd::CheckPublish{receipt,note}=>{
     let journal:Value=serde_json::from_slice(&fs::read(&receipt)?)?;
     let explicit=note.map(|note|note_ref(&note).map(|(id,_)|id)).transpose()?;
     let id=xhs_cli::publish::reconciliation_note_id(&journal,explicit.as_deref())?;
     if let Some(id)=id {let id=note_ref(&id)?.0;let mut detail=c.detail(&id)?;ensure!(detail["id"]==id,"creator detail returned a different note ID");let binds=c.request("GET",&query("/web_api/sns/capa/postgw/note/business_bind",&[("note_id",id.clone())]),None,true)?;detail["business_binds"]=json!(binds.to_string());let mut report=xhs_cli::publish::verify_readback_with_material(&journal["payload"],&detail,journal.get("material"),xhs_cli::publish::fetch_original_cover)?;if journal["operation"]=="edit" {report["operation"]=json!("edit");report["original_note_id"]=json!(id);}Ok(report)} else {
         let list=c.request("GET","/api/galaxy/v2/creator/note/user/posted?tab=0&page=0",None,true)?;
         let title=&journal["payload"]["common"]["title"];
         let candidates=xhs_cli::publish::publication_candidates(&list,title.as_str().context("journal title missing")?)?;
         Ok(json!({"status":"unknown","automatic_retry":false,"candidates":candidates,"next":"inspect my-notes (all pages) and use check-publish --note ID; zero first-page matches does not prove failure"}))
     }
 },
 Cmd::Publish{title,desc,desc_file,image,video,video_info,cover,receipt,dry_run,visibility,private,topic,activity,declaration,attachment}=>{
 let desc=if let Some(path)=desc_file {fs::read_to_string(path).context("read UTF-8 description file")?}else{desc.unwrap_or_default()};
 let mut binds=json!({"version":1,"noteId":0,"noteOrderBind":{},"notePostTiming":{"postTime":null},"noteCollectionBind":{"id":""}});
 if let Some(path)=attachment {let document:Value=serde_json::from_slice(&fs::read(path)?)?;bind_document(&mut binds,&document)?;}
 if let Some(ContentDeclaration::AiSynthesis)=declaration {binds["userDeclarationBind"]=json!({"origin":2});}
 if video.is_some() || video_info.is_some() {
     ensure!(!desc.contains("\\n"),"body contains literal \\n; use --desc-file for actual newlines");
     ensure!(title.chars().count()<=20 && desc.chars().count()<=1000,"video title/body exceeds creator length limits (20/1000)");
 }
 let mut topics=resolve_topics(&c,&topic)?;
 if let Some(id)=activity {
     let list=c.request("GET",&query("/api/galaxy/v2/creator/activity_center/list",&[("sort","1".into()),("type","1".into()),("source","3".into()),("topic_activity","0".into())]),None,true)?;
     let record=list["activity_list"].as_array().context("creator activity response missing list")?.iter().find(|v|v["activity_id"].as_u64()==Some(id)).context("active official campaign ID not found")?;
     for topic in xhs_cli::publish::bind_activity(&mut binds,record,now())? {if !topics.iter().any(|v|v["id"]==topic["id"]) {topics.push(topic);}}
 }
 let privacy=if private{1}else{visibility.unwrap_or(Visibility::Public).code()};
 if video.is_some() || video_info.is_some() {
     let media=if let Some(path)=video_info {serde_json::from_slice(&fs::read(path)?)?}else{
         let sidecar=PathBuf::from(format!("{}.video.json",receipt.as_ref().unwrap().display()));
         c.upload_video(video.as_ref().unwrap(),cover.as_ref().unwrap(),&sidecar)?
     };
     let payload=xhs_cli::publish::video_payload(&title,&desc,privacy,&topics,&media,&binds)?;
     if dry_run {return Ok(json!({"dry_run":true,"creates_note":false,"payload":payload}));}
     let status=c.video_status(&media)?;
     ensure!(status["has_first_frame"]==true || status["hasFirstFrame"]==true,"video processing is not ready; query video-status before publishing");
     let mut journal=json!({"status":"submitting","automatic_retry":false,"created_at_ms":now(),"payload":payload,"material":media});
     let mut file=new_receipt(receipt.as_ref().unwrap(),&journal)?;
     match c.request("POST","/web_api/sns/v2/note",Some(payload.clone()),false) {
         Ok(result)=>{journal["status"]=json!("accepted_unverified");journal["result"]=result;write_receipt(&mut file,&journal)?;Ok(journal)},
         Err(e)=>{journal["status"]=json!("unknown");journal["error"]=json!(format!("{e:#}"));write_receipt(&mut file,&journal)?;Err(e.context("publication outcome may be unknown; do not resend, inspect --receipt with check-publish"))}
     }
 } else {
 let images=prepare_images(&image, |path, bytes| c.upload(path, bytes))?;
 c.request("POST","/web_api/sns/v2/note",Some(json!({"common":{"type":"normal","title":title,"note_id":"","desc":desc,"source":"{\"type\":\"web\",\"ids\":\"\",\"extraInfo\":\"{\\\"subType\\\":\\\"official\\\"}\"}","business_binds":binds.to_string(),"ats":[],"hash_tag":topics,"post_loc":{},"privacy_info":{"op_type":1,"type":privacy}},"image_info":{"images":images},"video_info":null})),false)
 }
 },
 Cmd::Edit{note,title,desc,desc_file,video_info,dry_run,receipt,visibility,image,attachment}=>{
     let id=note_ref(&note)?.0;
     let desc=if let Some(path)=desc_file {Some(fs::read_to_string(path).context("read UTF-8 description file")?)} else {desc};
     let material=video_info.map(|path|->Result<Value>{Ok(serde_json::from_slice(&fs::read(path)?)?)}).transpose()?;
     let mut original=c.detail(&id)?;
     ensure!(original["id"]==id,"creator detail returned a different note ID");
     let mut bindings=c.request("GET",&query("/web_api/sns/capa/postgw/note/business_bind",&[("note_id",id.clone())]),None,true)?;
     original["business_binds"]=json!(bindings.to_string());
     let before=original.clone();
     if let Some(path)=attachment {let document:Value=serde_json::from_slice(&fs::read(path)?)?;bind_document(&mut bindings,&document)?;original["business_binds"]=json!(bindings.to_string());}
     let mut payload=xhs_cli::publish::edit_payload(&original,&id,title.as_deref(),desc.as_deref(),visibility.map(Visibility::code),material.as_ref())?;
     if !image.is_empty(){let images=prepare_images(&image,|path,bytes|c.upload(path,bytes))?;replace_images(&mut payload,&images)?;}
     if dry_run {return Ok(json!({"dry_run":true,"creates_note":false,"updates_note":false,"operation":"edit","note_id":id,"method":"PUT","endpoint":xhs_cli::publish::UPDATE_NOTE_ENDPOINT,"payload":payload}));}
     if let Some(material)=&material {let status=c.video_status(material)?;ensure!(status["has_first_frame"]==true || status["hasFirstFrame"]==true,"replacement video processing is not ready; query video-status before editing");}
     if let Some(receipt)=receipt {
         let journal=xhs_cli::publish::edit_journal(&id,&before,&payload,material.as_ref(),now())?;
         xhs_cli::publish::submit_edit_once(&receipt,journal,|payload|c.request("PUT",xhs_cli::publish::UPDATE_NOTE_ENDPOINT,Some(payload.clone()),true))
     } else {
         c.request("PUT",xhs_cli::publish::UPDATE_NOTE_ENDPOINT,Some(payload),true)
     }
 },
 Cmd::Delete{note}=>c.request("POST","/web_api/sns/capa/postgw/note/delete",Some(json!({"note_id":note_ref(&note)?.0})),true),
 }
}
fn import_session(path: Option<PathBuf>, input: &str) -> Result<Value> {
    use std::io::Read;
    let raw = if input == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        fs::read_to_string(input).context("--cookie accepts '-' for stdin or a cookie file path")?
    };
    let cookies: Value = if raw.trim().starts_with('{') {
        serde_json::from_str(&raw)?
    } else {
        let mut m = serde_json::Map::new();
        for part in raw.trim().split(';') {
            if let Some((key, value)) = part.trim().split_once('=') {
                m.insert(key.to_owned(), json!(value));
            }
        }
        Value::Object(m)
    };
    save_session(path, cookies)
}
fn save_session(path: Option<PathBuf>, cookies: Value) -> Result<Value> {
    use std::io::Write;
    ensure!(
        cookies["a1"].as_str().is_some_and(|s| !s.is_empty()),
        "cookie import missing a1"
    );
    let path = path.unwrap_or_else(|| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join(".xiaohongshu-cli/cookies.json")
    });
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let filename = path
        .file_name()
        .context("cookie path must name a file")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{filename}.{:032x}.tmp", rand::random::<u128>()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options
            .open(&temporary)
            .context("create private temporary session file")?;
        file.write_all(cookies.to_string().as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &path).context("atomically replace session file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(json!({"imported":true,"path":path}))
}
fn resolve_topics(c: &Client, topics: &[String]) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    for name in topics {
        let response=c.request("POST","/web_api/sns/v1/search/topic",Some(json!({"keyword":name,"suggest_topic_request":{"title":"","desc":""},"page":{"page_size":20,"page":1}})),true)?;
        result.push(xhs_cli::select_topic(&response, name)?);
    }
    Ok(result)
}
fn main() {
    match run(Cli::parse()) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            eprintln!("{}", json!({"error":format!("{e:#}")}));
            std::process::exit(1)
        }
    }
}
