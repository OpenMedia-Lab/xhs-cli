use std::process::Command;
#[test]
fn cli_exposes_real_operations() {
    let output = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .arg("--help")
        .output()
        .unwrap();
    let s = String::from_utf8(output.stdout).unwrap();
    for command in [
        "search",
        "read",
        "media",
        "user",
        "user-notes",
        "me",
        "my-notes",
        "bindings",
        "upload-document",
        "publish",
        "edit",
        "delete",
    ] {
        assert!(s.contains(command), "missing {command}");
    }
}

#[test]
fn media_requires_explicit_output_for_download_and_accepts_xsec_token() {
    let output = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["media", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--output"));
    assert!(help.contains("--token"));
}
#[test]
fn publish_requires_media() {
    let output = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["publish", "--title", "test", "--desc", "text"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
#[test]
fn edit_rejects_empty_patch() {
    let output = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["edit", "abc"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
#[test]
fn compatibility_surface_is_available() {
    for command in ["post", "status", "user-posts", "login"] {
        let output = Command::new(env!("CARGO_BIN_EXE_xhs"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing {command}");
    }
    let o = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["edit", "abc", "--help"])
        .output()
        .unwrap();
    let s = String::from_utf8(o.stdout).unwrap();
    assert!(s.contains("--image"));
    assert!(s.contains("--body"));
}
#[test]
fn login_imports_from_stdin_with_private_permissions() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("xhs-test-cookies-{}.json", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            path.to_str().unwrap(),
            "login",
            "--cookie",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"a1=test-session; web_session=test-auth")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["a1"], "test-session");
    assert!(!String::from_utf8(out.stdout).unwrap().contains("test-auth"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn user_posts_alias_requires_target_user_id() {
    let o = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["user-posts", "--help"])
        .output()
        .unwrap();
    let s = String::from_utf8(o.stdout).unwrap();
    assert!(
        s.contains("<ID>"),
        "user-posts must list the supplied user's notes"
    );
}
#[test]
fn json_runtime_errors_are_machine_readable() {
    let o = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["--json", "edit", "abc"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    let value: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
    assert!(value["error"].as_str().unwrap().contains("edit requires"));
}
#[cfg(unix)]
#[test]
fn failed_atomic_import_preserves_existing_session() {
    use std::{io::Write, os::unix::fs::PermissionsExt};
    let dir = std::env::temp_dir().join(format!("xhs-import-preserve-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cookies.json");
    std::fs::write(&path, b"{\"a1\":\"original\"}").unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            path.to_str().unwrap(),
            "login",
            "--cookie",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"a1=replacement; web_session=new")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let content = std::fs::read(&path).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        !out.status.success(),
        "import must require a same-directory temporary file"
    );
    assert_eq!(content, b"{\"a1\":\"original\"}");
}

#[test]
fn attachment_surface_is_explicit_and_available_for_publish_and_edit() {
    for command in ["publish", "edit"] {
        let out = Command::new(env!("CARGO_BIN_EXE_xhs"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(
            String::from_utf8(out.stdout)
                .unwrap()
                .contains("--attachment")
        );
    }
    let out = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["upload-document", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--json",
            "--cookies",
            "/not-present-xhs-test",
            "edit",
            "abc",
            "--attachment",
            "doc.json",
        ])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        !err.contains("edit requires"),
        "attachment-only edits must pass the patch gate"
    );
}
