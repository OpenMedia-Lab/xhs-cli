use std::path::PathBuf;
use xhs_cli::prepare_images;

fn fixture(name: &str, valid: bool) -> PathBuf {
    let p = std::env::temp_dir().join(format!("xhs-images-{}-{name}", std::process::id()));
    // PNG signature and IHDR with a 1620 × 2160 canvas.
    let mut bytes = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82];
    bytes.extend_from_slice(&1620u32.to_be_bytes());
    bytes.extend_from_slice(&2160u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    std::fs::write(&p, if valid { &bytes[..] } else { b"invalid" }).unwrap();
    p
}
#[test]
fn uploaded_payload_preserves_dimensions() {
    let p = fixture("valid.png", true);
    let images = prepare_images(std::slice::from_ref(&p), |_, bytes| {
        assert!(!bytes.is_empty());
        Ok("uploaded-id".to_string())
    })
    .unwrap();
    assert_eq!(
        images[0],
        serde_json::json!({"file_id":"uploaded-id","width":1620,"height":2160,"metadata":{"source":-1}})
    );
    std::fs::remove_file(p).unwrap();
}
#[test]
fn validates_entire_batch_before_any_upload() {
    let good = fixture("batch.png", true);
    let bad = fixture("bad.png", false);
    let mut uploads = 0;
    let result = prepare_images(&[good.clone(), bad.clone()], |_, _| {
        uploads += 1;
        Ok("id".to_string())
    });
    assert!(result.is_err());
    assert_eq!(uploads, 0);
    assert!(format!("{:#}", result.unwrap_err()).contains("bad.png"));
    std::fs::remove_file(good).unwrap();
    std::fs::remove_file(bad).unwrap();
}

#[test]
fn missing_empty_and_zero_dimension_images_never_upload() {
    let empty = fixture("empty.png", false);
    std::fs::write(&empty, []).unwrap();
    let zero = fixture("zero.png", true);
    let mut bytes = std::fs::read(&zero).unwrap();
    bytes[16..20].copy_from_slice(&0u32.to_be_bytes());
    std::fs::write(&zero, bytes).unwrap();
    let missing = std::env::temp_dir().join(format!("xhs-missing-{}.png", std::process::id()));
    for path in [&empty, &zero, &missing] {
        assert!(
            prepare_images(std::slice::from_ref(path), |_, _| {
                panic!("invalid image must not upload")
            })
            .is_err()
        );
    }
    std::fs::remove_file(empty).unwrap();
    std::fs::remove_file(zero).unwrap();
}

#[test]
fn publish_reports_invalid_image_as_json_before_network() {
    let bad = fixture("cli-bad.png", false);
    let cookies =
        std::env::temp_dir().join(format!("xhs-image-session-{}.json", std::process::id()));
    std::fs::write(&cookies, r#"{"a1":"test-only"}"#).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["--json", "--cookies"])
        .arg(&cookies)
        .args(["publish", "--title", "test", "--desc", "test", "--image"])
        .arg(&bad)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("read image dimensions")
    );
    std::fs::remove_file(bad).unwrap();
    std::fs::remove_file(cookies).unwrap();
}
