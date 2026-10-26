use serde_json::{Value, json};
use xhs_cli::publish::{api_response, inspect_mp4, verify_readback, video_payload};

#[test]
fn video_upload_selects_a_supported_ros_channel_and_never_sends_tokens_to_other_hosts() {
    use xhs_cli::publish::video_upload_target;
    assert_eq!(
        video_upload_target("http://ros-upload.xiaohongshu.com", "spectrum/key")
            .unwrap()
            .as_str(),
        "https://ros-upload.xiaohongshu.com/spectrum/key"
    );
    assert!(video_upload_target("ros-upload.xhscdn.com", "spectrum/key").is_ok());
    for host in [
        "https://evil.test",
        "https://ros-upload.xiaohongshu.com.evil.test",
        "https://secret@ros-upload.xiaohongshu.com",
        "https://ros-upload.xiaohongshu.com?token=x",
    ] {
        assert!(video_upload_target(host, "spectrum/key").is_err());
    }
    assert!(video_upload_target("ros-upload.xiaohongshu.com", "../outside").is_err());
}

#[test]
fn official_activity_is_bound_separately_from_its_topic_entities() {
    let a = json!({"activity_id":45151,"activity_name":"AI替我说","activity_status":1,"start_time":10,"end_time":30,"topic_infos":[{"id":"exact-topic","name":"AI替我说"}]});
    let mut binds = json!({"version":1,"optionRelationList":[{"type":"ATTACHMENT"}]});
    let topics = xhs_cli::publish::bind_activity(&mut binds, &a, 20).unwrap();
    assert_eq!(topics[0]["id"], "exact-topic");
    assert_eq!(binds["optionRelationList"][0]["type"], "ATTACHMENT");
    let relation = &binds["optionRelationList"][1]["relationList"][0];
    assert_eq!(relation["bizType"], "ACTIVITY_COMPONENT");
    assert_eq!(relation["bizId"], "45151");
    let extra: Value = serde_json::from_str(relation["extraInfo"].as_str().unwrap()).unwrap();
    assert_eq!(extra["start_time"], 10);
    assert!(xhs_cli::publish::bind_activity(&mut binds, &a, 40).is_err());
}

fn media() -> Value {
    json!({"file_id":"spectrum/test-video","video_id":"12345","width":32,"height":48,
        "duration":0.4,"bytes":2345,"md5":"0123456789abcdef0123456789abcdef",
        "cover":{"file_id":"spectrum/test-cover","width":32,"height":48},"upload_complete":true})
}

#[test]
fn native_mp4_inspection_reads_actual_tracks_and_rejects_truncation() {
    let bytes = include_bytes!("fixtures/tiny-h264-aac.mp4");
    let v = inspect_mp4(
        &mut std::io::Cursor::new(bytes.as_slice()),
        bytes.len() as u64,
    )
    .unwrap();
    assert_eq!(v["width"], 32);
    assert_eq!(v["height"], 48);
    assert_eq!(v["has_audio"], true);
    assert!(v["duration"].as_f64().unwrap() > 0.3);
    assert!(inspect_mp4(&mut std::io::Cursor::new(&bytes[..32]), 32).is_err());
    assert!(inspect_mp4(&mut std::io::Cursor::new(b"not an mp4"), 10).is_err());
}

#[test]
fn video_payload_preserves_real_newlines_visibility_and_topic_entities() {
    let topics = vec![json!({"id":"topic-exact","name":"用AI讲知识","type":"topic"})];
    let p = video_payload(
        "知识",
        "第一行\n第二行\n\n#用AI讲知识",
        1,
        &topics,
        &media(),
        &json!({"version":1}),
    )
    .unwrap();
    assert_eq!(p["common"]["type"], "video");
    assert_eq!(p["common"]["desc"], "第一行\n第二行\n\n#用AI讲知识");
    assert_eq!(p["common"]["privacy_info"]["type"], 1);
    assert_eq!(p["common"]["hash_tag"], json!(topics));
    assert!(p["image_info"].is_null());
    assert_eq!(p["video_info"]["file_id"], "spectrum/test-video");
    assert_eq!(p["video_info"]["format_width"], 32);
    assert_eq!(p["video_info"]["cover"]["frame"]["is_upload"], true);
    let serialized = p.to_string();
    let decoded: Value = serde_json::from_str(&serialized).unwrap();
    assert!(decoded["common"]["desc"].as_str().unwrap().contains('\n'));
}

#[test]
fn video_payload_refuses_unfinished_upload_invalid_visibility_and_topics() {
    let mut m = media();
    m["upload_complete"] = json!(false);
    assert!(video_payload("t", "d", 0, &[], &m, &json!({})).is_err());
    assert!(video_payload("t", "d", 2, &[], &media(), &json!({})).is_err());
    assert!(
        video_payload(
            "t",
            "d",
            0,
            &[json!({"name":"missing ID"})],
            &media(),
            &json!({})
        )
        .is_err()
    );
    assert!(video_payload("t", "literal\\nnewline", 0, &[], &media(), &json!({})).is_err());
}

#[test]
fn api_errors_keep_platform_codes_and_never_accept_explicit_failure() {
    for body in [
        json!({"success":false,"code":0,"msg":"failed"}),
        json!({"success":true,"code":-100,"msg":"expired"}),
        json!({"code":-9106,"msg":"deleted"}),
    ] {
        let err = api_response(200, &body.to_string())
            .unwrap_err()
            .to_string();
        assert!(err.contains("code="), "{err}");
    }
    assert!(
        api_response(471, "{}")
            .unwrap_err()
            .to_string()
            .contains("verification")
    );
    assert!(api_response(200, "<html>login</html>").is_err());
    assert!(api_response(200, "{}").is_err());
    assert_eq!(
        api_response(200, r#"{"success":true,"code":0,"data":{"id":"n"}}"#).unwrap()["id"],
        "n"
    );
}

#[test]
fn upload_redirects_and_errors_cannot_be_reported_as_completed() {
    for status in [301, 302, 307, 308, 400, 403, 500] {
        assert!(xhs_cli::publish::upload_success(status).is_err());
    }
    for status in [200, 201, 204] {
        assert!(xhs_cli::publish::upload_success(status).is_ok());
    }
}

#[test]
fn malformed_processing_data_returns_an_error_instead_of_panicking() {
    for value in [json!([]), json!("unexpected"), json!(7), Value::Null] {
        assert!(xhs_cli::publish::processing_response(value, "123").is_err());
    }
    assert_eq!(
        xhs_cli::publish::processing_response(json!({"has_first_frame":true}), "123").unwrap()["video_id"],
        "123"
    );
}

#[test]
fn video_cover_accepts_only_bounded_jpeg_or_png_material() {
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82];
    png.extend_from_slice(&32u32.to_be_bytes());
    png.extend_from_slice(&48u32.to_be_bytes());
    png.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        xhs_cli::publish::video_cover_info(&png).unwrap()["mime"],
        "image/png"
    );
    assert!(xhs_cli::publish::video_cover_info(b"GIF89a\x20\x00\x30\x00").is_err());
    assert!(xhs_cli::publish::video_cover_info(&vec![0; 32 * 1024 * 1024 + 1]).is_err());
}

#[test]
fn readback_checks_actual_text_topics_visibility_and_video_identity() {
    let p = video_payload(
        "标题",
        "一\n二",
        0,
        &[json!({"id":"topic","name":"话题","type":"topic"})],
        &media(),
        &json!({}),
    )
    .unwrap();
    let mut d = json!({"id":"note","type":"video","title":"标题","desc":"一\n二","privacy":{"type":0},"hash_tag":[{"id":"topic"}],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"}});
    let report = verify_readback(&p, &d).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["description_exact"], true);
    assert_eq!(report["normalization"], Value::Null);
    assert_eq!(report["format_warning"], Value::Null);
    assert_eq!(report["submitted_newline_count"], 1);
    assert_eq!(report["actual_newline_count"], 1);
    for (key, value) in [
        ("desc", json!("一\\n二")),
        ("privacy", json!({"type":1})),
        ("hash_tag", json!([])),
        ("video_info", json!({"file_id":"other"})),
        ("cover", json!({"fileid":"other-cover"})),
    ] {
        let old = d[key].clone();
        d[key] = value;
        assert!(verify_readback(&p, &d).is_err(), "{key}");
        d[key] = old;
    }
}

#[test]
fn readback_reports_the_observed_platform_collapse_of_consecutive_lf_only() {
    let submitted = "图解 LLM · 第 1 期\n\n段落一。\n\n段落二。\n\n我的博客原文：\nhttps://example.test/\n\n#AI替我说[话题]# #用AI讲知识[话题]#";
    let actual = "图解 LLM · 第 1 期\n段落一。\n段落二。\n我的博客原文：\nhttps://example.test/\n#AI替我说[话题]# #用AI讲知识[话题]#";
    let p = video_payload("标题", submitted, 0, &[], &media(), &json!({})).unwrap();
    let d = json!({"id":"note","type":"video","title":"标题","desc":actual,"privacy":{"type":0},"hash_tag":[],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"},"business_binds":"{}"});
    let report = verify_readback(&p, &d).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["description_exact"], false);
    assert_eq!(report["normalization"], "platform_collapsed_blank_lines");
    assert_eq!(
        report["format_warning"],
        "Platform collapsed consecutive LF blank lines; description formatting is not byte-for-byte identical."
    );
    assert_eq!(report["submitted_newline_count"], 9);
    assert_eq!(report["actual_newline_count"], 5);
    assert_eq!(report["desc"], actual);

    let p = video_payload("标题", "\n\n首\n\n\n尾\n\n", 0, &[], &media(), &json!({})).unwrap();
    let mut d = d;
    d["desc"] = json!("\n首\n尾\n");
    assert_eq!(verify_readback(&p, &d).unwrap()["description_exact"], false);
}

#[test]
fn blank_line_normalization_never_hides_text_whitespace_or_single_lf_changes() {
    let p = video_payload("标题", " 一。\n\n二。\n三。 ", 0, &[], &media(), &json!({})).unwrap();
    let mut d = json!({"id":"note","type":"video","title":"标题","privacy":{"type":0},"hash_tag":[],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"},"business_binds":"{}"});
    for actual in [
        " 一。\n二。\n三。 ",   // The sole permitted non-exact form.
        " 一。\n\n二。\n三。 ", // Exact form remains valid.
    ] {
        d["desc"] = json!(actual);
        assert!(verify_readback(&p, &d).is_ok(), "{actual:?}");
    }
    for actual in [
        " 一。二。\n三。 ",
        " 一。\n二。三。 ",
        " 一。\n\n\n二。\n三。 ",
        " 一。\r\n二。\r\n三。 ",
        " 一。\\n二。\\n三。 ",
        " 一。\n二。\n四。 ",
        " 一！\n二。\n三。 ",
        "一。\n二。\n三。 ",
        " 一。\n二。\n三。",
        " 一。\n二。 \n三。 ",
    ] {
        d["desc"] = json!(actual);
        assert!(verify_readback(&p, &d).is_err(), "{actual:?}");
    }
}

#[test]
fn normalized_description_keeps_all_other_readback_fields_strict() {
    let binds = json!({"userDeclarationBind":{"origin":2},"optionRelationList":[{"type":"ACTIVITY_COMPONENT","relationList":[{"bizType":"ACTIVITY_COMPONENT","bizId":"45151"}]}]});
    let p = video_payload(
        "标题",
        "一\n\n二",
        0,
        &[json!({"id":"topic","name":"话题","type":"topic"})],
        &media(),
        &binds,
    )
    .unwrap();
    let mut d = json!({"id":"note","type":"video","title":"标题","desc":"一\n二","privacy":{"type":0},"hash_tag":[{"id":"topic"}],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"},"business_binds":binds.to_string()});
    assert!(verify_readback(&p, &d).is_ok());
    for (key, value) in [
        ("title", json!("其他标题")),
        ("type", json!("normal")),
        ("privacy", json!({"type":1})),
        ("hash_tag", json!([])),
        ("video_info", json!({"file_id":"other"})),
        ("cover", json!({"fileid":"other-cover"})),
        (
            "business_binds",
            json!({"userDeclarationBind":{"origin":1},"optionRelationList":binds["optionRelationList"]}),
        ),
        (
            "business_binds",
            json!({"userDeclarationBind":{"origin":2},"optionRelationList":[]}),
        ),
    ] {
        let old = d[key].clone();
        d[key] = value;
        assert!(verify_readback(&p, &d).is_err(), "{key}");
        d[key] = old;
    }
}

#[test]
fn readback_requires_the_requested_activity_business_relation() {
    let binds = json!({"optionRelationList":[{"type":"ACTIVITY_COMPONENT","relationList":[{"bizType":"ACTIVITY_COMPONENT","bizId":"45151"}]}]});
    let p = video_payload("t", "d", 0, &[], &media(), &binds).unwrap();
    let mut d = json!({"id":"note","type":"video","title":"t","desc":"d","privacy":{"type":0},"hash_tag":[],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"},"business_binds":"{}"});
    assert!(verify_readback(&p, &d).is_err());
    d["business_binds"] = json!(binds.to_string());
    assert!(verify_readback(&p, &d).is_ok());
}

#[test]
fn cli_exposes_separate_upload_preview_and_readback_without_creating_notes() {
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    for command in [
        "upload-video",
        "video-status",
        "check-publish",
        "activities",
    ] {
        assert!(text.contains(command), "{command}");
    }
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args(["publish", "--help"])
        .output()
        .unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    for flag in ["--video-info", "--desc-file", "--receipt", "--dry-run"] {
        assert!(text.contains(flag), "{flag}");
    }
}

#[test]
fn video_publication_requires_receipt_before_upload_or_network() {
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "publish",
            "--title",
            "t",
            "--desc",
            "d",
            "--video",
            "missing.mp4",
            "--cover",
            "missing.jpg",
        ])
        .output()
        .unwrap();
    assert!(!p.status.success());
    assert!(String::from_utf8_lossy(&p.stderr).contains("receipt"));
}

#[test]
fn dry_run_reads_utf8_body_and_receipt_without_reaching_any_mutation_endpoint() {
    let dir = std::env::temp_dir().join(format!("xhs-video-dry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cookies.json"), r#"{"a1":"invalid-test-session"}"#).unwrap();
    std::fs::write(dir.join("material.json"), media().to_string()).unwrap();
    std::fs::write(dir.join("body.txt"), "一行\n二行\n\n三行").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .arg("--cookies")
        .arg(dir.join("cookies.json"))
        .args(["publish", "--title", "标题", "--desc-file"])
        .arg(dir.join("body.txt"))
        .arg("--video-info")
        .arg(dir.join("material.json"))
        .args(["--visibility", "private", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["creates_note"], false);
    assert_eq!(value["payload"]["common"]["desc"], "一行\n二行\n\n三行");
    assert_eq!(value["payload"]["common"]["privacy_info"]["type"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_existing_publication_journal_blocks_resending_before_auth_or_network() {
    let path = std::env::temp_dir().join(format!(
        "xhs-unknown-publication-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, r#"{"status":"unknown"}"#).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .args([
            "--cookies",
            "missing-session.json",
            "publish",
            "--title",
            "t",
            "--desc",
            "d",
            "--video-info",
            "missing-material.json",
            "--receipt",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("check-publish"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"status":"unknown"}"#
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn ai_declaration_is_a_native_binding_without_rewriting_body_or_topics() {
    let dir = std::env::temp_dir().join(format!("xhs-ai-declaration-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cookies.json"), r#"{"a1":"invalid-test-session"}"#).unwrap();
    std::fs::write(dir.join("material.json"), media().to_string()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xhs"))
        .arg("--cookies")
        .arg(dir.join("cookies.json"))
        .args(["publish", "--title", "标题", "--desc", "原文\n\n保留"])
        .arg("--video-info")
        .arg(dir.join("material.json"))
        .args(["--declaration", "ai-synthesis", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["creates_note"], false);
    let common = &value["payload"]["common"];
    assert_eq!(common["desc"], "原文\n\n保留");
    assert_eq!(common["hash_tag"], json!([]));
    let binds: Value = serde_json::from_str(common["business_binds"].as_str().unwrap()).unwrap();
    assert_eq!(binds["userDeclarationBind"], json!({"origin":2}));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn readback_rejects_a_missing_or_changed_requested_content_declaration() {
    let binds = json!({"userDeclarationBind":{"origin":2}});
    let p = video_payload("t", "d", 0, &[], &media(), &binds).unwrap();
    let mut d = json!({"id":"note","type":"video","title":"t","desc":"d","privacy":{"type":0},"hash_tag":[],"video_info":{"file_id":"spectrum/test-video"},"cover":{"fileid":"spectrum/test-cover"},"business_binds":"{}"});
    assert!(verify_readback(&p, &d).is_err());
    d["business_binds"] = json!({"userDeclarationBind":{"origin":1}});
    assert!(verify_readback(&p, &d).is_err());
    d["business_binds"] = json!("{\"userDeclarationBind\":{\"origin\":2}}");
    assert_eq!(
        verify_readback(&p, &d).unwrap()["content_declaration_origin"],
        2
    );
}

#[test]
fn uncertain_outcome_candidates_use_current_creator_display_titles() {
    let list = json!({"notes":[
        {"id":"matching-note","display_title":"图解 Transformer","type":"video"},
        {"id":"other-note","display_title":"图解 MoE","type":"video"}
    ],"page":{"has_more":false}});
    assert_eq!(
        xhs_cli::publish::publication_candidates(&list, "图解 Transformer").unwrap(),
        vec![json!({"id":"matching-note","title":"图解 Transformer","type":"video"})]
    );
    assert!(xhs_cli::publish::publication_candidates(&json!({"notes":null}), "t").is_err());
}

fn original_cover() -> Vec<u8> {
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82];
    png.extend_from_slice(&32u32.to_be_bytes());
    png.extend_from_slice(&48u32.to_be_bytes());
    png.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    png
}

fn aliased_media_case() -> (Value, Value, Value, Vec<u8>) {
    let cover = original_cover();
    let mut m = media();
    let fingerprint = xhs_cli::publish::cover_fingerprint(&cover).unwrap();
    for (key, value) in fingerprint.as_object().unwrap() {
        m["cover"][key] = value.clone();
    }
    let p = video_payload("标题", "一\n\n二", 0, &[], &m, &json!({})).unwrap();
    let d = json!({"id":"note","type":"video","title":"标题","desc":"一\n二","privacy":{"type":0},"hash_tag":[],"video_info":{"file_id":"spectrum/canonical-video","width":32,"height":48,"media_v2":{"video_id":"12345","video":{"width":32,"height":48,"duration":1,"md5":m["md5"]}}},"cover":{"fileid":"spectrum/canonical-cover","width":32,"height":48,"original":"https://sns-na-i1.xhscdn.com/spectrum/canonical-cover?sign=public-image-token"},"business_binds":"{}"});
    (p, d, m, cover)
}

#[test]
fn ros_canonical_file_header_preserves_the_permit_alias() {
    use xhs_cli::publish::upload_file_identity;
    assert_eq!(
        upload_file_identity("spectrum/permit-alias", Some("spectrum/canonical-id")).unwrap(),
        json!({"file_id":"spectrum/canonical-id","upload_file_id":"spectrum/permit-alias","file_id_aliases":["spectrum/permit-alias","spectrum/canonical-id"]})
    );
    assert_eq!(
        upload_file_identity("spectrum/permit-alias", None).unwrap()["file_id"],
        "spectrum/permit-alias"
    );
    for invalid in [
        "",
        "../../other",
        "https://evil.test/file",
        "spectrum/../other",
    ] {
        assert!(upload_file_identity("spectrum/permit-alias", Some(invalid)).is_err());
    }
}

#[test]
fn canonical_media_readback_requires_original_fingerprints_and_preserves_the_payload() {
    let (p, d, m, cover) = aliased_media_case();
    let original = p.clone();
    let mut fetches = 0;
    let report = xhs_cli::publish::verify_readback_with_material(&p, &d, Some(&m), |url| {
        fetches += 1;
        assert!(url.starts_with("https://sns-na-i1.xhscdn.com/"));
        Ok(cover.clone())
    })
    .unwrap();
    assert_eq!(fetches, 1);
    assert_eq!(p, original);
    assert_eq!(report["verified"], true);
    assert_eq!(report["description_exact"], false);
    assert_eq!(report["attestation"]["original_payload_preserved"], true);
    assert_eq!(
        report["media_identity"]["video"]["submitted_file_id"],
        "spectrum/test-video"
    );
    assert_eq!(
        report["media_identity"]["video"]["canonical_file_id"],
        "spectrum/canonical-video"
    );
    assert_eq!(report["media_identity"]["video"]["md5"], m["md5"]);
    assert_eq!(report["media_identity"]["video"]["video_id"], "12345");
    assert_eq!(
        report["media_identity"]["cover"]["sha256"],
        m["cover"]["sha256"]
    );
    assert_eq!(report["media_identity"]["cover"]["bytes"], cover.len());
}

#[test]
fn exact_media_ids_need_no_material_or_cover_download() {
    let (p, mut d, _, _) = aliased_media_case();
    d["video_info"]["file_id"] = p["video_info"]["file_id"].clone();
    d["cover"]["fileid"] = p["video_info"]["cover"]["file_id"].clone();
    let report = xhs_cli::publish::verify_readback_with_material(&p, &d, None, |_| {
        panic!("exact IDs must not download cover")
    })
    .unwrap();
    assert_eq!(report["media_identity"]["video"]["method"], "file_id_exact");
    assert_eq!(report["media_identity"]["cover"]["method"], "file_id_exact");
}

#[test]
fn canonical_video_rejects_missing_material_or_changed_original_evidence() {
    let (p, d, m, _) = aliased_media_case();
    assert!(
        xhs_cli::publish::verify_readback_with_material(&p, &d, None, |_| panic!(
            "missing material must fail before network"
        ))
        .is_err()
    );
    for (field, value) in [
        ("md5", json!("11111111111111111111111111111111")),
        ("md5", Value::Null),
        ("width", json!(33)),
        ("height", json!(49)),
        ("duration", json!(2)),
    ] {
        let mut changed = d.clone();
        changed["video_info"]["media_v2"]["video"][field] = value;
        assert!(
            xhs_cli::publish::verify_readback_with_material(&p, &changed, Some(&m), |_| panic!(
                "invalid video evidence must fail before network"
            ))
            .is_err(),
            "{field}"
        );
    }
    let mut changed = d.clone();
    changed["video_info"]["media_v2"]["video_id"] = json!("different-video");
    assert!(
        xhs_cli::publish::verify_readback_with_material(&p, &changed, Some(&m), |_| panic!(
            "wrong video ID"
        ))
        .is_err()
    );
    for (field, value) in [
        ("file_id", json!("spectrum/unrelated-payload")),
        ("md5", Value::Null),
        ("upload_complete", json!(false)),
        ("width", json!(33)),
    ] {
        let mut changed = m.clone();
        changed[field] = value;
        assert!(
            xhs_cli::publish::verify_readback_with_material(&p, &d, Some(&changed), |_| panic!(
                "invalid material must fail before network"
            ))
            .is_err(),
            "{field}"
        );
    }
}

#[test]
fn canonical_cover_rejects_changed_hash_bytes_dimensions_or_alias() {
    let (p, d, m, cover) = aliased_media_case();
    for (field, value) in [
        ("file_id", json!("spectrum/unrelated-cover")),
        ("md5", json!("11111111111111111111111111111111")),
        (
            "sha256",
            json!("1111111111111111111111111111111111111111111111111111111111111111"),
        ),
        ("sha256", Value::Null),
        ("bytes", json!(cover.len() + 1)),
        ("width", json!(33)),
        ("height", json!(49)),
    ] {
        let mut changed = m.clone();
        changed["cover"][field] = value;
        assert!(
            xhs_cli::publish::verify_readback_with_material(&p, &d, Some(&changed), |_| Ok(
                cover.clone()
            ))
            .is_err(),
            "{field}"
        );
    }
    let mut changed = d.clone();
    changed["cover"]["width"] = json!(33);
    assert!(
        xhs_cli::publish::verify_readback_with_material(&p, &changed, Some(&m), |_| Ok(
            cover.clone()
        ))
        .is_err()
    );
    let mut changed_bytes = cover;
    changed_bytes.push(0);
    assert!(
        xhs_cli::publish::verify_readback_with_material(&p, &d, Some(&m), |_| Ok(
            changed_bytes.clone()
        ))
        .is_err()
    );
}

#[test]
fn original_cover_transport_requires_https_cdn_and_complete_bounded_http_200() {
    use std::io::Read;
    use xhs_cli::publish::{original_cover_url, read_original_cover_response};
    assert!(original_cover_url("https://sns-na-i1.xhscdn.com/spectrum/file?sign=public").is_ok());
    for url in [
        "http://sns-na-i1.xhscdn.com/spectrum/file",
        "https://evil.test/spectrum/file",
        "https://xhscdn.com.evil.test/spectrum/file",
        "https://secret@sns-na-i1.xhscdn.com/spectrum/file",
        "https://sns-na-i1.xhscdn.com:8443/spectrum/file",
        "https://127.0.0.1/spectrum/file",
        "https://sns-na-i1.xhscdn.com/spectrum/file#fragment",
    ] {
        assert!(original_cover_url(url).is_err(), "{url}");
    }
    let bytes = original_cover();
    assert_eq!(
        read_original_cover_response(200, Some(bytes.len() as u64), bytes.as_slice()).unwrap(),
        bytes
    );
    for status in [206, 301, 302, 307, 308, 403, 404, 500] {
        assert!(
            read_original_cover_response(status, None, bytes.as_slice()).is_err(),
            "{status}"
        );
    }
    assert!(
        read_original_cover_response(200, Some(bytes.len() as u64 + 1), bytes.as_slice()).is_err()
    );
    assert!(
        read_original_cover_response(200, Some(32 * 1024 * 1024 + 1), std::io::empty()).is_err()
    );
    assert!(
        read_original_cover_response(200, None, std::io::repeat(0).take(32 * 1024 * 1024 + 1))
            .is_err()
    );
    let (p, mut d, m, _) = aliased_media_case();
    d["cover"]["original"] = json!("https://evil.test/file");
    assert!(
        xhs_cli::publish::verify_readback_with_material(&p, &d, Some(&m), |_| panic!(
            "untrusted URL must not reach transport"
        ))
        .is_err()
    );
}
