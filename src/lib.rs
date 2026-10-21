//! Native request signing. See NOTICE for upstream provenance.
use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::Rng;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as u64
}
pub fn merge_edit(
    original: &Value,
    title: Option<&str>,
    desc: Option<&str>,
    privacy: Option<u8>,
) -> Result<Value> {
    let normalized;
    let original = if original["common"].is_object() {
        original
    } else {
        ensure!(
            original["id"].is_string() && original["privacy"]["type"].is_number(),
            "incomplete creator detail"
        );
        let mut common = serde_json::Map::new();
        for key in [
            "type",
            "title",
            "desc",
            "ats",
            "hash_tag",
            "post_loc",
            "post_locs",
            "metadata",
            "source",
            "business_binds",
            "capa_version",
        ] {
            if let Some(v) = original.get(key) {
                common.insert(key.into(), v.clone());
            }
        }
        common.insert("note_id".into(), original["id"].clone());
        common.insert("privacy_info".into(), original["privacy"].clone());
        let images = original["images_list"].as_array().map(|list| {
            list.iter()
                .map(|i| {
                    let mut image = i.clone();
                    if let Some(id) = image.as_object_mut().and_then(|m| m.remove("fileid")) {
                        image["file_id"] = id;
                    }
                    image
                })
                .collect::<Vec<_>>()
        });
        normalized = json!({"common":common,"image_info":images.map(|images|json!({"images":images})),"video_info":original.get("video_info").cloned().unwrap_or(Value::Null)});
        &normalized
    };
    ensure!(
        original["common"]["note_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "missing original note ID"
    );
    ensure!(
        original["common"]["privacy_info"]["type"]
            .as_u64()
            .is_some(),
        "missing original privacy; refusing to guess visibility"
    );
    ensure!(
        original["common"]["title"].is_string() && original["common"]["desc"].is_string(),
        "missing original text fields"
    );
    ensure!(
        original["image_info"]["images"].is_array() || original["video_info"].is_object(),
        "missing original media"
    );
    let mut value = original.clone();
    if let Some(t) = title {
        value["common"]["title"] = json!(t)
    }
    if let Some(d) = desc {
        value["common"]["desc"] = json!(d)
    }
    if let Some(p) = privacy {
        ensure!(p <= 1, "visibility must be public or private");
        value["common"]["privacy_info"]["type"] = json!(p);
        value["common"]["privacy_info"]["op_type"] = json!(1)
    }
    Ok(value)
}
pub fn custom_hash(bytes: &[u8]) -> Vec<u8> {
    let len = bytes.len() as u32;
    let [mut a, mut b, mut c, mut d] = [
        1831565813u32 ^ len,
        461845907 ^ (len << 8),
        2246822507 ^ (len << 16),
        3266489909 ^ (len << 24),
    ];
    for chunk in bytes.as_chunks::<8>().0 {
        let x = u32::from_le_bytes(chunk[..4].try_into().unwrap());
        let y = u32::from_le_bytes(chunk[4..].try_into().unwrap());
        a = (a.wrapping_add(x) ^ c).rotate_left(7);
        b = (x ^ b).wrapping_add(d).rotate_left(11);
        c = (c.wrapping_add(y) ^ a).rotate_left(13);
        d = (d ^ y).wrapping_add(b).rotate_left(17);
    }
    let x = a ^ len;
    let y = b ^ x;
    let z = c.wrapping_add(y);
    let w = d ^ z;
    a = x.rotate_left(9).wrapping_add(z.rotate_left(17));
    b = y.rotate_left(13) ^ w.rotate_left(19);
    c = z.rotate_left(17).wrapping_add(a);
    d = w.rotate_left(19) ^ b;
    [a, b, c, d].iter().flat_map(|v| v.to_le_bytes()).collect()
}
pub fn creator_sign(uri: &str, body: Option<&str>, a1: &str, time: u64) -> String {
    let digest = format!(
        "{:x}",
        md5::compute(format!("url={uri}{}", body.unwrap_or("")))
    );
    let plain = format!("x1={digest};x2=0|0|0|1|0|0|1|0|0|0|1|0|0|0|0|1|0|0|0;x3={a1};x4={time};");
    let plain = STANDARD.encode(plain);
    let enc =
        cbc::Encryptor::<aes::Aes128>::new(b"7cc4adla5ay0701v".into(), b"4uzjr7mbsibcaldp".into())
            .encrypt_padded_vec_mut::<Pkcs7>(plain.as_bytes());
    format!("XYW_{}",STANDARD.encode(json!({"signSvn":"56","signType":"x2","appId":"ugc","signVersion":"1","payload":hex::encode(enc)}).to_string()))
}
const ALPH: &str = "ZmserbBoHQtNP+wOcza/LpngG8yJq42KWYj0DSfdikx3VT16IlUAFM97hECvuRX5";
const X3: &str = "MfgqrsbcyzPQRStuvC7mn501HIJBo2DEFTKdeNOwxWXYZap89+/A4UVLhijkl63G";
fn b64(bytes: impl AsRef<[u8]>, alphabet: &str) -> String {
    let std = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    STANDARD
        .encode(bytes)
        .bytes()
        .map(|b| {
            std.iter()
                .position(|v| *v == b)
                .map(|i| alphabet.as_bytes()[i] as char)
                .unwrap_or(b as char)
        })
        .collect()
}
fn rc4(data: &[u8]) -> Vec<u8> {
    let key = b"xhswebmplfbt";
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j)
    }
    let (mut i, mut j) = (0, 0);
    data.iter()
        .map(|v| {
            i = (i + 1) % 256;
            j = (j + s[i] as usize) % 256;
            s.swap(i, j);
            v ^ s[(s[i] as usize + s[j] as usize) % 256]
        })
        .collect()
}
fn b1(time: u64) -> String {
    let fp = json!({"x33":"0","x34":"0","x35":"0","x36":"10","x37":"0|0|0|0|0|0|0|0|0|1|0|0|0|0|0|0|0|0|1|0|0|0|0|0","x38":"0|0|1|0|1|0|0|0|0|0|1|0|1|0|1|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0|0","x39":0,"x42":"3.4.4","x43":"d41d8cd98f00b204e9800998ecf8427e","x44":time.to_string(),"x45":"__SEC_CAV__1-1-1-1-1|__SEC_WSA__|","x46":"false","x48":"","x49":"{list:[],type:}","x50":"","x51":"","x52":"","x82":"_0x17a2|_0x1954"});
    // Upstream encodes RC4 bytes as Latin-1 characters, then percent-encodes UTF-8.
    let encrypted = rc4(fp.to_string().as_bytes());
    let latin: String = encrypted.iter().map(|b| char::from(*b)).collect();
    b64(latin.as_bytes(), ALPH)
}
pub fn main_sign(content: &str, a1: &str, time: u64) -> (String, String) {
    let seed = rand::thread_rng().r#gen::<u32>();
    let byte = seed as u8;
    let mut p = vec![121, 104, 96, 41];
    p.extend(seed.to_le_bytes());
    let ts = time.to_le_bytes();
    p.extend(ts);
    p.extend(time.saturating_sub(25000).to_le_bytes());
    p.extend(16u32.to_le_bytes());
    p.extend(1450u32.to_le_bytes());
    p.extend((content.len() as u32).to_le_bytes());
    p.extend(md5::compute(content).0[..8].iter().map(|v| v ^ byte));
    let mut a = a1.as_bytes().to_vec();
    a.resize(52, 0);
    a.truncate(52);
    p.push(52);
    p.extend(a);
    p.push(10);
    p.extend(b"xhs-pc-web");
    let table = [
        115, 248, 83, 102, 103, 201, 181, 131, 99, 94, 4, 68, 250, 132, 21,
    ];
    let checks = [0, 1, 18, 1, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0];
    p.push(1);
    p.push(byte ^ table[0]);
    for i in 1..15 {
        p.push(table[i] ^ checks[i]);
    }
    let path = content
        .split('?')
        .next()
        .unwrap_or(content)
        .split('{')
        .next()
        .unwrap_or(content);
    let mut hash_input = ts.to_vec();
    hash_input.extend(md5::compute(path).0);
    p.extend([2, 97, 51, 16]);
    p.extend(custom_hash(&hash_input).iter().map(|v| v ^ byte));
    let key=hex::decode("71a302257793271ddd273bcee3e4b98d9d7935e1da33f5765e2ea8afb6dc77a51a499d23b67c20660025860cbf13d4540d92497f58686c574e508f46e1956344f39139bf4faf22a3eef120b79258145b2feb5193b6478669961298e79bedca646e1a693a926154a5a7a1bd1cf0dedb742f917a747a1e388b234f2277516db7116035439730fa61e9822a0eca7bff72d8").unwrap();
    for (i, v) in p.iter_mut().enumerate() {
        if i < key.len() {
            *v ^= key[i]
        }
    }
    let xs=format!("XYS_{}",b64(json!({"x0":"4.2.6","x1":"xhs-pc-web","x2":"macOS","x3":format!("mns0301_{}",b64(&p[..144],X3)),"x4":""}).to_string(),ALPH));
    let fp = b1(time);
    let crc = (crc32fast::hash(fp.as_bytes()) ^ 0xedb88320) as i32;
    let common=b64(json!({"s0":5,"s1":"","x0":"1","x1":"4.2.6","x2":"macOS","x3":"xhs-pc-web","x4":"4.86.0","x5":a1,"x6":"","x7":"","x8":fp,"x9":crc,"x10":0,"x11":"normal"}).to_string(),ALPH);
    (xs, common)
}
pub fn replace_images(payload: &mut Value, images: &[Value]) -> Result<()> {
    ensure!(!images.is_empty(), "image replacement cannot be empty");
    ensure!(
        payload["image_info"].is_object(),
        "cannot replace images in a video note"
    );
    payload["image_info"]["images"] = json!(images);
    Ok(())
}
pub fn select_topic(response: &Value, name: &str) -> Result<Value> {
    let list = response
        .as_array()
        .or_else(|| response.get("topic_info_dtos").and_then(Value::as_array))
        .or_else(|| response.get("topics").and_then(Value::as_array))
        .or_else(|| response.get("items").and_then(Value::as_array))
        .ok_or_else(|| anyhow::anyhow!("topic response missing list"))?;
    list.iter()
        .find(|v| v["name"] == name || v["id"] == name)
        .map(|v| json!({"id":v["id"],"name":v["name"],"type":"topic"}))
        .ok_or_else(|| anyhow::anyhow!("topic not found with exact name or ID"))
}
