use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
use xhs_cli::{creator_sign, main_sign, merge_edit, now, replace_images};
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
#[derive(Subcommand)]
enum Cmd {
    #[command(alias = "status")]
    Me,
    Login {
        #[arg(long, default_value = "-")]
        cookie: String,
    },
    Topics {
        keyword: String,
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
    #[command(alias = "post")]
    Publish {
        #[arg(long)]
        title: String,
        #[arg(long, visible_alias = "body")]
        desc: String,
        #[arg(long,visible_alias="images",required=true,num_args=1..)]
        image: Vec<PathBuf>,
        #[arg(long, conflicts_with = "private")]
        visibility: Option<Visibility>,
        #[arg(long)]
        private: bool,
        #[arg(long)]
        topic: Vec<String>,
    },
    Edit {
        note: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, visible_alias = "body")]
        desc: Option<String>,
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
                if path.starts_with("/api/galaxy/") {
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
        let text = response.text()?;
        let data: Value = serde_json::from_str(&text)
            .with_context(|| format!("non-JSON response (HTTP {status})"))?;
        ensure!(status.is_success(), "HTTP {status}: {data}");
        ensure!(
            data["success"] == true || data["code"] == 0 || data["result"] == 0,
            "platform error: {data}"
        );
        Ok(data.get("data").cloned().unwrap_or(data))
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
    fn upload(&self, path: &PathBuf) -> Result<String> {
        let bytes = fs::read(path).with_context(|| format!("read image {}", path.display()))?;
        ensure!(!bytes.is_empty(), "empty image");
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
        self.http
            .put(format!("https://ros-upload.xiaohongshu.com/{id}"))
            .header("X-Cos-Security-Token", token)
            .header("Content-Type", mime)
            .body(bytes)
            .send()?
            .error_for_status()?;
        Ok(id.to_owned())
    }
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
        visibility,
        image,
        ..
    } = &cli.command
    {
        ensure!(
            title.is_some() || desc.is_some() || visibility.is_some() || !image.is_empty(),
            "edit requires --title, --desc or --visibility"
        )
    }
    if let Cmd::Login { cookie } = &cli.command {
        return import_session(cli.cookies, cookie);
    }
    let c = Client::new(cli.cookies)?;
    match cli.command{
 Cmd::Login{..}=>unreachable!(),
 Cmd::Topics{keyword}=>c.request("POST","/web_api/sns/v1/search/topic",Some(json!({"keyword":keyword,"suggest_topic_request":{"title":"","desc":""},"page":{"page_size":20,"page":1}})),true),
 Cmd::Me=>c.request("GET","/api/sns/web/v2/user/me",None,false),
 Cmd::Search{keyword,page,sort}=>c.request("POST","/api/sns/web/v1/search/notes",Some(json!({"keyword":keyword,"page":page,"page_size":20,"search_id":now().to_string(),"sort":sort,"note_type":0,"ext_flags":[],"filters":[{"tags":["general"],"type":"sort_type"},{"tags":["不限"],"type":"filter_note_type"},{"tags":["不限"],"type":"filter_note_time"},{"tags":["不限"],"type":"filter_note_range"},{"tags":["不限"],"type":"filter_pos_distance"}],"geo":"","image_formats":["jpg","webp","avif"]})),false),
 Cmd::Read{note,token}=>{let(id,t)=note_ref(&note)?;c.request("POST","/api/sns/web/v1/feed",Some(json!({"source_note_id":id,"image_formats":["jpg","webp","avif"],"extra":{"need_body_topic":"1"},"xsec_source":"pc_feed","xsec_token":token.unwrap_or(t)})),false)},
 Cmd::User{id}=>c.request("GET",&query("/api/sns/web/v1/user/otherinfo",&[("target_user_id",id)]),None,false),
 Cmd::UserNotes{id,cursor}=>c.request("GET",&query("/api/sns/web/v1/user_posted",&[("num","30".into()),("cursor",cursor),("user_id",id),("image_scenes","FD_WM_WEBP".into())]),None,false),
 Cmd::MyNotes{page,tab}=>c.request("GET",&format!("/api/galaxy/v2/creator/note/user/posted?tab={tab}&page={page}"),None,true),
 Cmd::Detail{note}=>c.detail(&note_ref(&note)?.0),
 Cmd::Publish{title,desc,image,visibility,private,topic}=>{
 for p in &image{ensure!(p.is_file(),"image does not exist: {}",p.display())}
 let topics=resolve_topics(&c,&topic)?;
 let images=image.iter().map(|p|c.upload(p).map(|id|json!({"file_id":id,"metadata":{"source":-1}}))).collect::<Result<Vec<_>>>()?;
 c.request("POST","/web_api/sns/v2/note",Some(json!({"common":{"type":"normal","title":title,"note_id":"","desc":desc,"source":"{\"type\":\"web\",\"ids\":\"\",\"extraInfo\":\"{\\\"subType\\\":\\\"official\\\"}\"}","business_binds":json!({"version":1,"noteId":0,"noteOrderBind":{},"notePostTiming":{"postTime":null},"noteCollectionBind":{"id":""}}).to_string(),"ats":[],"hash_tag":topics,"post_loc":{},"privacy_info":{"op_type":1,"type":if private{1}else{visibility.unwrap_or(Visibility::Public).code()}}},"image_info":{"images":images},"video_info":null})),false)
 },
 Cmd::Edit{note,title,desc,visibility,image}=>{let id=note_ref(&note)?.0;let mut original=c.detail(&id)?;let bindings=c.request("GET",&query("/web_api/sns/capa/postgw/note/business_bind",&[("note_id",id.clone())]),None,true)?;original["business_binds"]=json!(bindings.to_string());let mut payload=merge_edit(&original,title.as_deref(),desc.as_deref(),visibility.map(Visibility::code))?;if !image.is_empty(){let images=image.iter().map(|p|c.upload(p).map(|id|json!({"file_id":id,"metadata":{"source":-1}}))).collect::<Result<Vec<_>>>()?;replace_images(&mut payload,&images)?;}ensure!(payload["common"]["note_id"]==id,"detail returned a different note ID");c.request("PUT","/web_api/sns/capa/postgw/note/update",Some(payload),true)},
 Cmd::Delete{note}=>c.request("POST","/web_api/sns/capa/postgw/note/delete",Some(json!({"note_id":note_ref(&note)?.0})),true),
 }
}
fn import_session(path: Option<PathBuf>, input: &str) -> Result<Value> {
    use std::io::{Read, Write};
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
