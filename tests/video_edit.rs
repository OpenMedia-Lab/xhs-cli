use serde_json::{Value, json};

#[test]
fn edit_exposes_reusable_video_receipts_and_non_mutating_preview() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["edit", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for flag in ["--video-info", "--desc-file", "--dry-run", "--receipt"] {
        assert!(help.contains(flag), "missing {flag}");
    }
}

#[test]
fn edit_requires_a_new_journal_before_authentication_or_network() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            "missing-session.json",
            "edit",
            "oldnote",
            "--video-info",
            "missing-material.json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("edit requires --receipt"));
}

fn material() -> Value {
    json!({"upload_complete":true,"file_id":"spectrum/new-video","video_id":"new-video-id","width":1080,"height":1920,"duration":193.834,"md5":"0123456789abcdef0123456789abcdef","cover":{"file_id":"spectrum/new-cover","width":1080,"height":1920,"bytes":128386,"md5":"11111111111111111111111111111111","sha256":"1111111111111111111111111111111111111111111111111111111111111111"}})
}

fn original() -> Value {
    json!({"common":{"note_id":"oldnote","title":"图解 MoE","desc":"旧正文\n#AI替我说[话题]#","type":"video","privacy_info":{"type":1,"user_ids":[],"future_permission":"keep"},"hash_tag":[{"id":"topic","name":"AI替我说","type":"topic"}],"business_binds":"{\"userDeclarationBind\":{\"origin\":2},\"optionRelationList\":[{\"type\":\"ACTIVITY_COMPONENT\",\"relationList\":[{\"bizId\":\"45151\"}]}],\"unknownBinding\":{\"value\":3}}","future_common":{"keep":true}},"video_info":{"file_id":"spectrum/old-video","width":1080,"height":1920,"duration":193,"media_v2":{"video_id":"old-video-id","video":{"md5":"old"}},"url":"https://cdn.test/old.mp4","chapters":[{"text":"旧章节","time":1000}],"future_video":{"keep":true},"cover":{"file_id":"spectrum/old-cover","width":1080,"height":1920,"frame":{"ts":1,"is_upload":false,"is_user_select":true},"future_cover":"keep"}},"image_info":null,"future_top":{"keep":true}})
}

#[test]
fn replacement_preserves_original_note_bindings_privacy_and_extensions() {
    let before = original();
    let p = xhs_cli::publish::edit_payload(
        &before,
        "oldnote",
        None,
        Some("修正正文\n\n#AI替我说[话题]#"),
        None,
        Some(&material()),
    )
    .unwrap();
    assert_eq!(p["common"]["note_id"], "oldnote");
    assert_eq!(p["common"]["desc"], "修正正文\n\n#AI替我说[话题]#");
    for key in [
        "title",
        "privacy_info",
        "hash_tag",
        "business_binds",
        "future_common",
    ] {
        let mut expected = before["common"][key].clone();
        if key == "privacy_info" {
            expected["op_type"] = json!(1);
        }
        assert_eq!(p["common"][key], expected, "{key}");
    }
    assert_eq!(p["future_top"], before["future_top"]);
    assert_eq!(
        p["video_info"]["future_video"],
        before["video_info"]["future_video"]
    );
    assert_eq!(p["video_info"]["file_id"], "spectrum/new-video");
    assert_eq!(p["video_info"]["cover"]["file_id"], "spectrum/new-cover");
    assert_eq!(p["video_info"]["replaced_video"], true);
    assert_eq!(p["video_info"]["segments"]["items"][0]["duration"], 193.834);
    assert!(p["video_info"].get("media_v2").is_none());
    assert!(p["video_info"].get("url").is_none());
    assert!(p.get("material").is_none());
}

#[test]
fn body_only_video_edit_preserves_media_and_does_not_claim_replacement() {
    let before = original();
    let p = xhs_cli::publish::edit_payload(&before, "oldnote", None, Some("正文修正"), None, None)
        .unwrap();
    assert_eq!(p["video_info"]["file_id"], "spectrum/old-video");
    assert_eq!(p["video_info"]["cover"]["file_id"], "spectrum/old-cover");
    assert_eq!(p["video_info"]["replaced_video"], false);
    assert_eq!(
        p["video_info"]["chapters"],
        before["video_info"]["chapters"]
    );
    assert_eq!(
        p["video_info"]["cover"]["frame"],
        before["video_info"]["cover"]["frame"]
    );
    assert_eq!(p["video_info"]["cover"]["future_cover"], "keep");
    let mut expected_privacy = before["common"]["privacy_info"].clone();
    expected_privacy["op_type"] = json!(1);
    assert_eq!(p["common"]["privacy_info"], expected_privacy);
}

#[test]
fn raw_creator_video_maps_top_level_cover_and_retains_business_bindings() {
    let before = json!({"id":"oldnote","type":"video","title":"图解 MoE","desc":"旧正文","privacy":{"type":0},"hash_tag":[],"business_binds":"{\"userDeclarationBind\":{\"origin\":2}}","biz_relations":[{"biz_type":"FUTURE_COMPONENT","biz_id":"existing"}],"video_info":{"file_id":"spectrum/old-video","width":1080,"height":1920,"duration":193,"media_v2":{"video_id":"old-video-id"}},"cover":{"fileid":"spectrum/old-cover","width":1080,"height":1920,"original":"https://sns-na-i1.xhscdn.com/old"}});
    let p =
        xhs_cli::publish::edit_payload(&before, "oldnote", None, Some("修正"), None, None).unwrap();
    assert_eq!(p["common"]["note_id"], "oldnote");
    assert_eq!(p["common"]["business_binds"], before["business_binds"]);
    assert_eq!(p["common"]["biz_relations"], before["biz_relations"]);
    assert_eq!(p["video_info"]["cover"]["file_id"], "spectrum/old-cover");
    assert_eq!(p["video_info"]["replaced_video"], false);
    assert_eq!(p["common"]["privacy_info"]["op_type"], 1);
}

#[test]
fn legacy_text_edit_does_not_require_a_new_receipt() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            "missing-legacy-edit-session.json",
            "edit",
            "oldnote",
            "--title",
            "修正标题",
        ])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("read session"), "{error}");
    assert!(!error.contains("requires --receipt"), "{error}");
}

#[test]
fn edit_rejects_ambiguous_body_or_image_video_replacements_before_authentication() {
    for flags in [
        vec!["--desc", "正文", "--desc-file", "body.txt", "--dry-run"],
        vec![
            "--image",
            "cover.png",
            "--video-info",
            "material.json",
            "--dry-run",
        ],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
            .args(["--cookies", "missing-session.json", "edit", "oldnote"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    }
}

#[test]
fn replacement_rejects_wrong_note_type_id_incomplete_cover_and_unfinished_material() {
    let before = original();
    assert!(
        xhs_cli::publish::edit_payload(&before, "othernote", None, None, None, Some(&material()))
            .is_err()
    );
    let mut normal = before.clone();
    normal["common"]["type"] = json!("normal");
    assert!(
        xhs_cli::publish::edit_payload(&normal, "oldnote", None, None, None, Some(&material()))
            .is_err()
    );
    for (key, value) in [
        ("cover", Value::Null),
        ("upload_complete", json!(false)),
        ("md5", Value::Null),
    ] {
        let mut invalid = material();
        invalid[key] = value;
        assert!(
            xhs_cli::publish::edit_payload(&before, "oldnote", None, None, None, Some(&invalid))
                .is_err(),
            "{key}"
        );
    }
    assert!(
        xhs_cli::publish::edit_payload(&before, "oldnote", None, Some("literal\\n"), None, None)
            .is_err()
    );
}

#[test]
fn an_existing_edit_journal_blocks_resending_before_authentication() {
    let path = std::env::temp_dir().join(format!("xhs-existing-edit-{}.json", std::process::id()));
    let before = "{\"operation\":\"edit\",\"status\":\"unknown\"}";
    std::fs::write(&path, before).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            "missing-session.json",
            "edit",
            "oldnote",
            "--desc",
            "修正正文",
            "--receipt",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("check-publish"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn uncertain_edit_saves_the_original_id_and_payload_before_one_put_and_never_retries() {
    let path = std::env::temp_dir().join(format!("xhs-edit-once-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let before = original();
    let payload = xhs_cli::publish::edit_payload(
        &before,
        "oldnote",
        None,
        Some("修正"),
        None,
        Some(&material()),
    )
    .unwrap();
    let journal =
        xhs_cli::publish::edit_journal("oldnote", &before, &payload, Some(&material()), 1234)
            .unwrap();
    let result = xhs_cli::publish::submit_edit_once(&path, journal.clone(), |sent| {
        let synced: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(synced["payload"], *sent);
        assert_eq!(synced["status"], "submitting");
        assert_eq!(synced["operation"], "edit");
        assert_eq!(synced["method"], "PUT");
        assert_eq!(synced["note_id"], "oldnote");
        assert_eq!(synced["material"], material());
        assert_eq!(synced["original"], before);
        anyhow::bail!("connection closed after write; response unknown")
    });
    assert!(result.is_err());
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["status"], "unknown");
    assert_eq!(saved["automatic_retry"], false);
    assert_eq!(saved["payload"], payload);
    assert!(
        xhs_cli::publish::submit_edit_once(&path, journal, |_| panic!(
            "existing journal must prevent second PUT"
        ))
        .is_err()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        saved
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn accepted_edit_remains_unverified_and_reconciliation_stays_on_the_original_note() {
    let path = std::env::temp_dir().join(format!("xhs-edit-accepted-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let before = original();
    let payload =
        xhs_cli::publish::edit_payload(&before, "oldnote", None, Some("修正"), None, None).unwrap();
    let journal = xhs_cli::publish::edit_journal("oldnote", &before, &payload, None, 1234).unwrap();
    let accepted =
        xhs_cli::publish::submit_edit_once(&path, journal, |_| Ok(json!({"note_id":"oldnote"})))
            .unwrap();
    assert_eq!(accepted["status"], "accepted_unverified");
    assert_eq!(
        xhs_cli::publish::reconciliation_note_id(&accepted, None)
            .unwrap()
            .as_deref(),
        Some("oldnote")
    );
    assert!(xhs_cli::publish::reconciliation_note_id(&accepted, Some("othernote")).is_err());
    let mut invalid = accepted.clone();
    invalid["payload"]["common"]["note_id"] = json!("othernote");
    assert!(xhs_cli::publish::reconciliation_note_id(&invalid, None).is_err());
    let mut invalid = accepted;
    invalid["note_id"] = json!("othernote");
    invalid["payload"]["common"]["note_id"] = json!("othernote");
    assert!(xhs_cli::publish::reconciliation_note_id(&invalid, None).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_different_response_note_id_stays_unknown_and_recovers_only_the_original_note() {
    for key in ["id", "note_id", "noteId"] {
        let path = std::env::temp_dir().join(format!(
            "xhs-edit-wrong-id-{}-{key}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let before = original();
        let payload =
            xhs_cli::publish::edit_payload(&before, "oldnote", None, Some("修正"), None, None)
                .unwrap();
        let journal =
            xhs_cli::publish::edit_journal("oldnote", &before, &payload, None, 1234).unwrap();
        let mut response = json!({});
        response[key] = json!("othernote");
        assert!(
            xhs_cli::publish::submit_edit_once(&path, journal, |_| Ok(response.clone())).is_err(),
            "{key}"
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["status"], "unknown");
        assert_eq!(saved["result"], response);
        assert_eq!(
            xhs_cli::publish::reconciliation_note_id(&saved, None)
                .unwrap()
                .as_deref(),
            Some("oldnote")
        );
        std::fs::remove_file(path).unwrap();
    }
}
