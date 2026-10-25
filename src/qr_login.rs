//! Native HTTP QR flow. Never replace the old session with a guest or wrong account.
use super::{Client, query, save_session};
use anyhow::{Context, Result, bail, ensure};
use qrcode::{
    QrCode,
    render::{svg, unicode},
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn user_id(v: &Value) -> Option<&str> {
    v.get("user_id")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/login_info/user_id").and_then(Value::as_str))
        .or_else(|| v.pointer("/basic_info/user_id").and_then(Value::as_str))
}
fn apply(c: &mut Client, v: &Value) {
    for (source, target) in [
        ("session", "web_session"),
        ("secure_session", "web_session_sec"),
    ] {
        if let Some(value) = v
            .get(source)
            .or_else(|| v.get("login_info").and_then(|x| x.get(source)))
            .and_then(Value::as_str)
            .filter(|x| !x.is_empty())
        {
            c.cookies[target] = json!(value);
        }
    }
}
fn render(url: &str, path: Option<&Path>) -> Result<()> {
    let parsed = url::Url::parse(url).context("Invalid login QR URL")?;
    ensure!(
        matches!(
            parsed.scheme(),
            "http" | "https" | "xhsdiscover" | "xhslink"
        ),
        "Unexpected login QR URL scheme"
    );
    if matches!(parsed.scheme(), "http" | "https") {
        ensure!(
            parsed.host_str().is_some_and(|h| h == "xiaohongshu.com"
                || h.ends_with(".xiaohongshu.com")
                || h == "xhslink.com"
                || h.ends_with(".xhslink.com")),
            "Unexpected QR origin"
        );
    }
    let qr = QrCode::new(url.as_bytes())?;
    eprintln!(
        "Scan with the Xiaohongshu app and confirm:\n{}",
        qr.render::<unicode::Dense1x2>().build()
    );
    if let Some(path) = path {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(path)
            .context("QR file must be a new writable SVG file")?;
        f.write_all(
            qr.render::<svg::Color>()
                .min_dimensions(512, 512)
                .build()
                .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn login(path: Option<PathBuf>, qr_file: Option<&Path>, timeout: u64) -> Result<Value> {
    ensure!(
        (1..=600).contains(&timeout),
        "Timeout must be 1..600 seconds"
    );
    let a1 = format!(
        "{:024x}{}{:015x}",
        rand::random::<u128>() & ((1 << 96) - 1),
        super::now(),
        rand::random::<u64>() & ((1 << 60) - 1)
    );
    let mut c = Client {
        http: reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        cookies: json!({"a1":a1,"webId":format!("{:032x}",rand::random::<u128>())}),
    };
    let activated = c.request(
        "POST",
        "/api/sns/web/v1/login/activate",
        Some(json!({})),
        false,
    )?;
    apply(&mut c, &activated);
    let generated = c.request(
        "POST",
        "/api/sns/web/v1/login/qrcode/create",
        Some(json!({"qr_type":1})),
        false,
    )?;
    let id = generated["qr_id"].as_str().context("QR ID missing")?;
    let code = generated["code"].as_str().context("QR code missing")?;
    render(
        generated["url"].as_str().context("QR URL missing")?,
        qr_file,
    )?;
    let until = Instant::now() + Duration::from_secs(timeout);
    let mut scanned = false;
    while Instant::now() < until {
        std::thread::sleep(Duration::from_secs(2));
        let info = c.request(
            "POST",
            "/api/qrcode/userinfo",
            Some(json!({"qrId":id,"code":code})),
            false,
        )?;
        match info["codeStatus"].as_i64() {
            Some(0) => {}
            Some(1) => {
                if !scanned {
                    eprintln!("Scanned; waiting for phone confirmation");
                    scanned = true;
                }
            }
            Some(2) => {
                let confirmed = info["userId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .context("Confirmed QR has no account ID")?;
                let complete = c.request(
                    "GET",
                    &query(
                        "/api/sns/web/v1/login/qrcode/status",
                        &[("qr_id", id.into()), ("code", code.into())],
                    ),
                    None,
                    false,
                )?;
                apply(&mut c, &complete);
                ensure!(
                    c.cookies["web_session"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty()),
                    "QR confirmed but no login session returned; existing session kept"
                );
                let me = c.request("GET", "/api/sns/web/v2/user/me", None, false)?;
                ensure!(
                    user_id(&me) == Some(confirmed) && me["guest"] != true,
                    "QR confirmation did not produce the confirmed account; existing session kept"
                );
                save_session(path, c.cookies)?;
                return Ok(json!({"is_login":true,"user_id":confirmed}));
            }
            _ => bail!("QR expired or rejected; existing session kept"),
        }
    }
    bail!("QR login timed out; existing session kept")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applies_nested_session_and_resolves_account() {
        let mut c = Client {
            http: reqwest::blocking::Client::new(),
            cookies: json!({"a1":"fixture"}),
        };
        let v = json!({"login_info":{"session":"fixture-session","user_id":"user1"}});
        apply(&mut c, &v);
        assert_eq!(c.cookies["web_session"], "fixture-session");
        assert_eq!(user_id(&v), Some("user1"));
        assert_eq!(user_id(&json!({"guest":true})), None);
    }
}
