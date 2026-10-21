use serde_json::json;
use xhs_cli::{custom_hash, merge_edit};
#[test]
fn edit_preserves_private_visibility_media_and_unknown_fields() {
    let before = json!({"common":{"note_id":"abc","title":"old","desc":"body","privacy_info":{"type":1,"op_type":1},"ats":[{"id":"mention"}],"hash_tag":[],"extra":"keep"},"image_info":{"images":[{"file_id":"one","width":800}]},"video_info":null});
    let after = merge_edit(&before, Some("new"), None, None).unwrap();
    let mut expected = before;
    expected["common"]["title"] = json!("new");
    assert_eq!(after, expected);
}
#[test]
fn edit_can_clear_text_and_explicitly_change_visibility() {
    let before = json!({"common":{"note_id":"abc","title":"t","desc":"d","privacy_info":{"type":1}},"image_info":{"images":[{"file_id":"i"}]}});
    let after = merge_edit(&before, None, Some(""), Some(0)).unwrap();
    assert_eq!(after["common"]["desc"], "");
    assert_eq!(after["common"]["privacy_info"]["type"], 0);
}
#[test]
fn edit_rejects_incomplete_original_instead_of_guessing() {
    assert!(merge_edit(&json!({"common":{"note_id":"id"}}), Some("a"), None, None).is_err());
}
#[test]
fn native_hash_matches_upstream_vector() {
    assert_eq!(
        hex::encode(custom_hash(&[0; 24])),
        "9fad92669cd96e419344146e92a93fed"
    );
}
#[test]
fn creator_signature_matches_upstream_vector() {
    assert_eq!(
        xhs_cli::creator_sign(
            "/test",
            Some("{\"title\":\"你好\"}"),
            "test-a1",
            1700000000000
        ),
        "XYW_eyJzaWduU3ZuIjoiNTYiLCJzaWduVHlwZSI6IngyIiwiYXBwSWQiOiJ1Z2MiLCJzaWduVmVyc2lvbiI6IjEiLCJwYXlsb2FkIjoiMTY1MTY5Y2UwOWM5YmFkMjI2OGFhMjY4MjM5MGQzYzI5MjVkYzY2Y2FlMDU0Mzg1YzEwMzA2OGQwZGJlYmNiZDcxYTJmZDIwODM0MTRmMjgxNzJlZjRiNWQ4MTQ2NWI2YjhkZDkyZTRhYTRiMWFjYjE2ODU5ZjFmNzcyNjI0ZDVmZTlhNzM1MjkzNjc4MGUxMGViNDQyNDA1ODdjOGM3YTE0YWYwNzE4NmFhOTE3MTQ3NmZiNmM3MTQzNjBkMGQzNWU3MTg5NDIxNjA1NmQzMmMxODQ1MWVlYmVkZWUxOTczOGNhYWQzODQwNWIyZDZkMTdlYTMxNjkzNjllMjI4YmFkYTgxMDgzZjk0NTYyZDQzZTFmZjI3YzZkMzNmZGZlIn0="
    );
}
#[test]
fn creator_detail_is_mapped_without_losing_private_visibility_or_images() {
    let raw = json!({"id":"abc","title":"old","desc":"body","type":"normal","privacy":{"type":1},"images_list":[{"fileid":"img1","metadata":{"source":-1},"width":100}],"ats":[{"id":"mention"}],"hash_tag":[{"id":"topic"}],"post_loc":{"id":"loc"},"business_binds":"{\"version\":1}"});
    let after = merge_edit(&raw, Some("new"), None, None).unwrap();
    assert_eq!(after["common"]["note_id"], "abc");
    assert_eq!(after["common"]["privacy_info"]["type"], 1);
    assert_eq!(after["common"]["desc"], "body");
    assert_eq!(after["image_info"]["images"][0]["file_id"], "img1");
    assert_eq!(after["image_info"]["images"][0]["width"], 100);
    assert_eq!(after["common"]["business_binds"], "{\"version\":1}");
}
#[test]
fn image_edit_replaces_images_without_changing_note_or_privacy() {
    let mut v = json!({"common":{"note_id":"abc","privacy_info":{"type":1},"desc":"d"},"image_info":{"images":[{"file_id":"old"}],"extra":"keep"}});
    xhs_cli::replace_images(&mut v, &[json!({"file_id":"new"})]).unwrap();
    assert_eq!(v["common"]["note_id"], "abc");
    assert_eq!(v["common"]["privacy_info"]["type"], 1);
    assert_eq!(v["image_info"]["images"][0]["file_id"], "new");
    assert_eq!(v["image_info"]["extra"], "keep");
}
#[test]
fn topic_resolution_uses_current_creator_shape_and_exact_match() {
    let response = json!({"result":{"success":true},"topic_info_dtos":[{"id":"a","name":"Rust","type":"official","link":"https://example.test"},{"id":"b","name":"Rusty"}]});
    let topic = xhs_cli::select_topic(&response, "Rust").unwrap();
    assert_eq!(topic["id"], "a");
    assert_eq!(topic["type"], "topic");
    assert!(xhs_cli::select_topic(&response, "Ru").is_err());
}
