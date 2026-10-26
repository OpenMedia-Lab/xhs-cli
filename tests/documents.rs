use serde_json::{Value, json};
use xhs_cli::{bind_document, document_info};
#[test]
fn document_validation_rejects_installers_mismatched_files_and_oversize() {
    assert!(document_info("momo.exe", b"MZfake").is_err());
    assert!(document_info("momo.pdf", b"MZfake").is_err());
    assert!(document_info("momo.pdf", b"").is_err());
    assert!(document_info("momo.pdf", &vec![0; 100 * 1024 * 1024 + 1]).is_err());
    let d = document_info("中文说明.pdf", b"%PDF-1.7\nfake fixture").unwrap();
    assert_eq!(d["docType"], "PDF");
    assert_eq!(d["docName"], "中文说明.pdf");
}
#[test]
fn document_binding_preserves_other_components_and_replaces_only_attachment() {
    let mut b = json!({"version":1,"noteId":42,"optionRelationList":[{"type":"VOTE","relationList":[{"bizId":"keep"}]},{"type":"ATTACHMENT","relationList":[]}],"other":{"preserve":true}});
    let d = json!({"docId":"doc-1","fileId":"sns/test.pdf","docName":"说明.pdf","docShowName":"说明.pdf","docType":"PDF","docSize":17});
    bind_document(&mut b, &d).unwrap();
    assert_eq!(b["noteId"], 42);
    assert_eq!(b["other"]["preserve"], true);
    assert_eq!(b["optionRelationList"][0]["type"], "VOTE");
    let list = b["optionRelationList"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    let extra: Value =
        serde_json::from_str(list[1]["relationList"][0]["extraInfo"].as_str().unwrap()).unwrap();
    assert_eq!(extra[0], d);
    let old = b.clone();
    assert!(bind_document(&mut b, &json!({"docId":"missing-fields"})).is_err());
    assert_eq!(b, old);
}
