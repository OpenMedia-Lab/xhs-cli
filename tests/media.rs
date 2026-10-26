use serde_json::{Value, json};
use std::io::{self, Cursor, Read};
use xhs_cli::media::{media_url, note_url, resolve_note_with, select_video, write_video};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/video-feed.json")).unwrap()
}

// Catches selecting the default silent fmp4 instead of a playable video with sound.
#[test]
fn real_video_feed_selects_complete_mp4_and_retains_chapters() {
    let v = select_video(&fixture(), "6adb6000000000003301beee").unwrap();
    assert_eq!(v["width"], 720);
    assert_eq!(v["height"], 960);
    assert_eq!(v["duration_ms"], 380820);
    assert_eq!(v["format"], "mp4");
    assert_eq!(v["codec"], "EF4");
    assert_eq!(v["audio_codec"], "aac");
    assert_eq!(v["chapters"][1]["text"], "工具介绍");
    assert!(
        v["url"]
            .as_str()
            .unwrap()
            .starts_with("https://sns-video-v2.xhscdn.com/")
    );
}

// Catches failing to compare candidates or allowing a higher-resolution incompatible codec.
#[test]
fn h264_is_preferred_and_largest_complete_h264_variant_wins() {
    let mut feed = fixture();
    let mut bigger = feed["items"][0]["note_card"]["video"]["media"]["stream"]["EF4"][0].clone();
    bigger["width"] = json!(1080);
    bigger["height"] = json!(1440);
    bigger["master_url"] = json!("https://sns-video-v2.xhscdn.com/bigger.mp4");
    feed["items"][0]["note_card"]["video"]["media"]["stream"]["EF4"]
        .as_array_mut()
        .unwrap()
        .push(bigger.clone());
    bigger["video_codec"] = json!("h265");
    bigger["width"] = json!(2160);
    feed["items"][0]["note_card"]["video"]["media"]["stream"]["HEVC"] = json!([bigger]);
    let v = select_video(&feed, "6adb6000000000003301beee").unwrap();
    assert_eq!(v["url"], "https://sns-video-v2.xhscdn.com/bigger.mp4");
}

#[test]
fn nonvideo_missing_note_and_silent_fragments_fail_explicitly() {
    assert!(
        select_video(
            &json!({"items":[{"id":"a","note_card":{"type":"normal"}}]}),
            "a"
        )
        .is_err()
    );
    assert!(select_video(&fixture(), "different").is_err());
    let mut feed = fixture();
    feed["items"][0]["note_card"]["video"]["media"]["stream"]["EF4"] = json!([]);
    assert!(select_video(&feed, "6adb6000000000003301beee").is_err());
}

// Catches URL-host suffix confusion, credential-bearing redirects, and non-note landing pages.
#[test]
fn note_urls_preserve_tokens_and_reject_external_redirects() {
    let (id, token) = note_url(
        "https://www.xiaohongshu.com/discovery/item/6adb6000000000003301beee?xsec_token=hello%3D",
    )
    .unwrap();
    assert_eq!(id, "6adb6000000000003301beee");
    assert_eq!(token, "hello=");
    for url in [
        "https://xiaohongshu.com.evil.test/explore/id",
        "https://www.xiaohongshu.com@evil.test/explore/id",
        "https://user:secret@www.xiaohongshu.com/explore/id",
        "https://www.xiaohongshu.com:8443/explore/id",
        "https://www.xiaohongshu.com/user/profile/id",
        "http://www.xiaohongshu.com/explore/id",
    ] {
        assert!(note_url(url).is_err(), "accepted {url}");
    }
}

#[test]
fn share_redirects_resolve_official_notes_and_stop_before_visiting_external_domains() {
    let result = resolve_note_with("https://xhslink.cn/o/3NjfcrXedKu", |_| Ok("https://www.xiaohongshu.com/discovery/item/6adb6000000000003301beee?xsec_token=token%3D".into())).unwrap();
    assert_eq!(
        result,
        ("6adb6000000000003301beee".to_owned(), "token=".to_owned())
    );
    for target in [
        "https://evil.test/secret",
        "http://xhslink.cn/insecure",
        "https://user:pass@xhslink.com/a",
    ] {
        let mut calls = 0;
        let result = resolve_note_with("https://xhslink.com/a", |_| {
            calls += 1;
            Ok(target.to_owned())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "external domain must never be fetched");
    }
    let mut calls = 0;
    assert!(
        resolve_note_with("https://xhslink.com/a", |_| {
            calls += 1;
            Ok("/loop".to_owned())
        })
        .is_err()
    );
    assert_eq!(calls, 5);
}

#[test]
fn media_urls_upgrade_known_cdn_to_https_and_reject_other_hosts() {
    assert_eq!(
        media_url("http://sns-video-v2.xhscdn.com/video.mp4?a=1")
            .unwrap()
            .as_str(),
        "https://sns-video-v2.xhscdn.com/video.mp4?a=1"
    );
    for url in [
        "https://xhscdn.com.evil.test/a.mp4",
        "https://127.0.0.1/a.mp4",
        "file:///tmp/a",
        "https://a@xhscdn.com/a",
        "https://xhscdn.com:123/a",
    ] {
        assert!(media_url(url).is_err(), "accepted {url}");
    }
}

fn output(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("xhs-video-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("video.mp4")
}
const MP4: &[u8] = b"\0\0\0\x18ftypmp42\0\0\0\0mp42isom";

#[test]
fn complete_video_is_published_without_overwriting_existing_file() {
    let path = output("complete");
    let _ = std::fs::remove_file(&path);
    assert_eq!(write_video(Cursor::new(MP4), Some(24), &path).unwrap(), 24);
    assert_eq!(std::fs::read(&path).unwrap(), MP4);
    assert!(write_video(Cursor::new(MP4), Some(24), &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), MP4);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "interrupted",
        ))
    }
}

// Catches a failed or HTML download leaving a target or a temporary file behind.
#[test]
fn failed_truncated_or_nonvideo_downloads_leave_no_file() {
    let path = output("failure");
    for (bytes, length) in [
        (b"<html>verification</html>".as_slice(), None),
        (MP4, Some(100)),
    ] {
        assert!(write_video(Cursor::new(bytes), length, &path).is_err());
        assert!(!path.exists());
    }
    assert!(write_video(Broken, None, &path).is_err());
    assert!(!path.exists());
    assert!(write_video(Cursor::new(MP4).chain(Broken), None, &path).is_err());
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        0
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
