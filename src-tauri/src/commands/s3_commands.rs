use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key error");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn extract_host(raw_url: &str) -> Result<String, String> {
    let s = raw_url.trim();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .ok_or("URL 格式错误，需要 http:// 或 https://")?;
    Ok(rest.split('/').next().unwrap_or(rest).to_string())
}

struct S3Signer {
    access_key: String,
    secret_key: String,
    region: String,
}

impl S3Signer {
    fn sign(
        &self,
        method: &str,
        host: &str,
        canonical_uri: &str,
        canonical_query: &str,
        payload_hash: &str,
        content_type: Option<&str>,
    ) -> (String, String) {
        use std::collections::BTreeMap;

        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        let mut headers = BTreeMap::new();
        headers.insert("host".to_string(), host.to_string());
        headers.insert("x-amz-content-sha256".to_string(), payload_hash.to_string());
        headers.insert("x-amz-date".to_string(), amz_date.clone());
        if let Some(ct) = content_type {
            headers.insert("content-type".to_string(), ct.to_string());
        }

        let signed_headers: String = headers.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(";");
        let canonical_headers: String = headers
            .iter()
            .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
            .collect();

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method, canonical_uri, canonical_query, canonical_headers, signed_headers, payload_hash
        );

        let scope = format!("{}/{}/s3/aws4_request", date_stamp, self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            scope,
            sha256_hex(canonical_request.as_bytes())
        );

        let k_date = hmac_sha256(format!("AWS4{}", self.secret_key).as_bytes(), date_stamp.as_bytes());
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

        let credential = format!("{}/{}/{}/s3/aws4_request", self.access_key, date_stamp, self.region);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}, SignedHeaders={}, Signature={}",
            credential, signed_headers, signature
        );

        (authorization, amz_date)
    }
}

fn get_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(|v| v.as_str()).ok_or(format!("缺少 {}", key))
}

fn get_str_or<'a>(args: &'a Value, key: &str, default: &str) -> String {
    args.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string()
}

/// Resolve region: "auto" or empty → "us-east-1"
fn resolve_region(region: &str) -> String {
    let r = region.trim();
    if r.is_empty() || r == "auto" {
        "us-east-1".to_string()
    } else {
        r.to_string()
    }
}

/// Resolve endpoint + bucket with endpoint path priority.
/// Examples:
/// - endpoint=https://s3.hi168.com, bucket=vant => (https://s3.hi168.com, vant)
/// - endpoint=https://s3.hi168.com/vant, bucket=xxx => (https://s3.hi168.com, vant)
fn resolve_endpoint_and_bucket(endpoint: &str, bucket_input: &str) -> Result<(String, String), String> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    let bucket_input = bucket_input.trim();

    let (scheme, rest) = if let Some(rest) = endpoint.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("服务地址格式错误，需要 http:// 或 https://".to_string());
    };

    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or("").trim();
    if host.is_empty() {
        return Err("服务地址缺少 host".to_string());
    }

    let path_bucket = parts.find(|s| !s.trim().is_empty()).map(|s| s.trim().to_string());

    let final_bucket = if let Some(pb) = path_bucket {
        pb
    } else if !bucket_input.is_empty() {
        bucket_input.to_string()
    } else {
        return Err("缺少存储桶：请在 bucket 字段填写，或在 endpoint 后追加 /bucket".to_string());
    };

    let base_endpoint = format!("{}://{}", scheme, host);
    Ok((base_endpoint, final_bucket))
}

fn extract_xml_tag(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = content.find(&open)?;
    let val_start = start + open.len();
    let end = content[val_start..].find(&close)?;
    Some(content[val_start..val_start + end].to_string())
}

fn parse_list_objects(xml: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut pos = 0;
    while let Some(start) = xml[pos..].find("<Contents>") {
        let content_start = pos + start + "<Contents>".len();
        if let Some(end) = xml[content_start..].find("</Contents>") {
            let content = &xml[content_start..content_start + end];
            if let (Some(key), Some(lm)) = (extract_xml_tag(content, "Key"), extract_xml_tag(content, "LastModified")) {
                results.push((key, lm));
            }
            pos = content_start + end + "</Contents>".len();
        } else {
            break;
        }
    }
    results
}

const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[allow(non_snake_case)]
#[tauri::command]
pub async fn s3__test_connection(args: Value) -> Result<bool, String> {
    let endpoint = get_str(&args, "endpoint")?;
    let region = resolve_region(&get_str_or(&args, "region", "auto"));
    let access_key = get_str(&args, "accessKeyId")?;
    let secret_key = get_str(&args, "secretAccessKey")?;
    let bucket_input = get_str_or(&args, "bucket", "");

    let (base_endpoint, bucket) = resolve_endpoint_and_bucket(endpoint, &bucket_input)?;

    let query = "list-type=2&max-keys=1";
    let url = format!("{}/{}?{}", base_endpoint, bucket, query);
    let host = extract_host(&url)?;
    let canonical_uri = format!("/{}", bucket);

    let signer = S3Signer { access_key: access_key.to_string(), secret_key: secret_key.to_string(), region };
    let (auth, amz_date) = signer.sign("GET", &host, &canonical_uri, query, EMPTY_HASH, None);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", auth)
        .header("x-amz-content-sha256", EMPTY_HASH)
        .header("x-amz-date", amz_date)
        .send()
        .await
        .map_err(|e| format!("连接失败: {}", e))?;

    if resp.status().is_success() {
        Ok(true)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("连接失败: HTTP {} - {}", status, &body[..body.len().min(200)]))
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub async fn s3__backup(args: Value) -> Result<Value, String> {
    let endpoint = get_str(&args, "endpoint")?;
    let region = resolve_region(&get_str_or(&args, "region", "auto"));
    let access_key = get_str(&args, "accessKeyId")?;
    let secret_key = get_str(&args, "secretAccessKey")?;
    let bucket_input = get_str_or(&args, "bucket", "");
    let playlists = args.get("playlists").cloned().unwrap_or(Value::Null);
    let settings = args.get("settings").cloned().unwrap_or(Value::Null);

    let (base_endpoint, bucket) = resolve_endpoint_and_bucket(endpoint, &bucket_input)?;

    let now = chrono::Utc::now();
    let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let key_name = format!("mio-backup-{}.json", now.format("%Y-%m-%dT%H-%M-%S-%fZ"));

    let backup_data = serde_json::json!({ "version": 1, "timestamp": timestamp, "playlists": playlists, "settings": settings });
    let body = serde_json::to_string(&backup_data).map_err(|e| format!("序列化失败: {}", e))?;

    let url = format!("{}/{}/{}", base_endpoint, bucket, key_name);
    let host = extract_host(&url)?;
    let canonical_uri = format!("/{}/{}", bucket, key_name);
    let payload_hash = sha256_hex(body.as_bytes());

    let signer = S3Signer { access_key: access_key.to_string(), secret_key: secret_key.to_string(), region };
    let (auth, amz_date) = signer.sign("PUT", &host, &canonical_uri, "", &payload_hash, Some("application/json"));

    let client = reqwest::Client::new();
    let resp = client
        .put(&url)
        .header("Authorization", auth)
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-date", amz_date)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("上传失败: {}", e))?;

    if resp.status().is_success() {
        Ok(serde_json::json!({ "success": true, "timestamp": timestamp }))
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("上传失败: HTTP {} - {}", status, &text[..text.len().min(200)]))
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub async fn s3__restore(args: Value) -> Result<Value, String> {
    let endpoint = get_str(&args, "endpoint")?;
    let region = resolve_region(&get_str_or(&args, "region", "auto"));
    let access_key = get_str(&args, "accessKeyId")?;
    let secret_key = get_str(&args, "secretAccessKey")?;
    let bucket_input = get_str_or(&args, "bucket", "");

    let (base_endpoint, bucket) = resolve_endpoint_and_bucket(endpoint, &bucket_input)?;

    let base_url = format!("{}/{}", base_endpoint, bucket);
    let host = extract_host(&base_url)?;

    let signer = S3Signer { access_key: access_key.to_string(), secret_key: secret_key.to_string(), region: region.clone() };

    // List objects
    let query = "list-type=2&max-keys=1000&prefix=mio-backup-";
    let list_url = format!("{}?{}", base_url, query);
    let canonical_uri = format!("/{}", bucket);

    let (auth, amz_date) = signer.sign("GET", &host, &canonical_uri, query, EMPTY_HASH, None);

    let client = reqwest::Client::new();
    let resp = client
        .get(&list_url)
        .header("Authorization", auth)
        .header("x-amz-content-sha256", EMPTY_HASH)
        .header("x-amz-date", amz_date)
        .send()
        .await
        .map_err(|e| format!("列出备份失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("列出备份失败: HTTP {}", resp.status()));
    }

    let xml_body = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;
    let mut objects = parse_list_objects(&xml_body);
    if objects.is_empty() {
        return Err("未找到备份数据".to_string());
    }
    objects.sort_by(|a, b| b.1.cmp(&a.1));
    let latest_key = objects[0].0.clone();

    // Download latest backup
    let download_url = format!("{}/{}", base_url, latest_key);
    let download_uri = format!("/{}/{}", bucket, latest_key);

    let signer2 = S3Signer { access_key: access_key.to_string(), secret_key: secret_key.to_string(), region };
    let (auth, amz_date) = signer2.sign("GET", &host, &download_uri, "", EMPTY_HASH, None);

    let resp = client
        .get(&download_url)
        .header("Authorization", auth)
        .header("x-amz-content-sha256", EMPTY_HASH)
        .header("x-amz-date", amz_date)
        .send()
        .await
        .map_err(|e| format!("下载备份失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载备份失败: HTTP {}", resp.status()));
    }

    let body = resp.text().await.map_err(|e| format!("读取备份数据失败: {}", e))?;
    let data: Value = serde_json::from_str(&body).map_err(|e| format!("解析备份数据失败: {}", e))?;

    Ok(serde_json::json!({ "success": true, "data": data }))
}
