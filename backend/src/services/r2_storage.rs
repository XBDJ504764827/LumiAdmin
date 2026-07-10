use crate::config::Config;
use anyhow::Context;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// R2 存储客户端，提供文件上传和签名 URL 生成功能。
#[derive(Clone)]
pub struct R2Storage {
    backend: R2Backend,
    client: reqwest::Client,
}

#[derive(Clone)]
enum R2Backend {
    Worker {
        base_url: String,
        api_key: String,
        signing_key: String,
    },
    S3 {
        endpoint: String,
        bucket: String,
        access_key: String,
        secret_key: String,
    },
}

impl R2Storage {
    pub fn new(config: &Config) -> Option<Self> {
        if let (Some(base_url), Some(api_key), Some(signing_key)) = (
            config.r2_worker_url.as_ref(),
            config.r2_worker_api_key.as_ref(),
            config.r2_worker_signing_key.as_ref(),
        ) {
            return Some(Self {
                backend: R2Backend::Worker {
                    base_url: normalize_base_url(base_url),
                    api_key: api_key.trim().to_string(),
                    signing_key: signing_key.trim().to_string(),
                },
                client: reqwest::Client::new(),
            });
        }

        let endpoint = config.r2_endpoint.as_ref()?;
        let bucket = config.r2_bucket.as_ref()?;
        let access_key = config.r2_access_key_id.as_ref()?;
        let secret_key = config.r2_secret_access_key.as_ref()?;
        let endpoint = normalize_base_url(endpoint);
        let bucket = bucket.trim().to_string();

        Some(Self {
            backend: R2Backend::S3 {
                endpoint,
                bucket,
                access_key: access_key.trim().to_string(),
                secret_key: secret_key.trim().to_string(),
            },
            client: reqwest::Client::new(),
        })
    }

    /// 上传文件到 R2，返回存储 key
    pub async fn upload(
        &self,
        appeal_id: Uuid,
        file_name: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        self.upload_with_prefix("appeals", appeal_id, file_name, content_type, data)
            .await
    }

    /// 上传文件到 R2 指定业务目录，返回存储 key
    pub async fn upload_with_prefix(
        &self,
        prefix: &str,
        owner_id: Uuid,
        file_name: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        let key = format!(
            "{}/{}/{}-{}",
            prefix.trim_matches('/'),
            owner_id,
            Uuid::new_v4(),
            sanitize_filename(file_name)
        );
        let response = match &self.backend {
            R2Backend::Worker {
                base_url, api_key, ..
            } => self
                .client
                .post(format!("{base_url}/internal/upload"))
                .header("x-api-key", api_key)
                .header("x-object-key", &key)
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(data)
                .send()
                .await
                .context("R2 Worker upload request failed")?,
            R2Backend::S3 {
                endpoint, bucket, ..
            } => {
                let url = format!("{endpoint}/{bucket}/{key}");
                let now = Utc::now();
                let payload_hash = hex::encode(sha256_hash(&data));
                let headers =
                    self.build_signed_headers("PUT", &url, content_type, &payload_hash, &now)?;
                self.client
                    .put(&url)
                    .headers(headers)
                    .body(data)
                    .send()
                    .await
                    .context("R2 upload request failed")?
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("R2 upload failed: {status} - {body}");
        }

        Ok(key)
    }

    /// 生成预签名下载 URL（有效期 1 小时）
    pub fn presigned_url(&self, key: &str, expiry_secs: u32) -> String {
        if let R2Backend::Worker {
            base_url,
            signing_key,
            ..
        } = &self.backend
        {
            let expires = Utc::now().timestamp() + i64::from(expiry_secs.min(604_800));
            let message = format!("{key}\n{expires}");
            let signature = hex::encode(hmac_sha256(signing_key.as_bytes(), message.as_bytes()));
            return format!(
                "{base_url}/files/{}?expires={expires}&signature={signature}",
                uri_encode_path(key)
            );
        }

        let R2Backend::S3 {
            endpoint,
            bucket,
            access_key,
            ..
        } = &self.backend
        else {
            unreachable!();
        };
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = "auto";
        let service = "s3";
        let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
        let resource = format!("/{bucket}/{key}");
        let canonical_uri = uri_encode_path(&resource);
        let host = endpoint.strip_prefix("https://").unwrap_or(endpoint);
        let credential = format!("{access_key}/{credential_scope}");
        let expires = expiry_secs.min(604_800);
        let canonical_query = format!(
            "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={}&X-Amz-Date={}&X-Amz-Expires={}&X-Amz-SignedHeaders=host",
            uri_encode(&credential),
            amz_date,
            expires
        );
        let canonical_headers = format!("host:{host}\n");
        let canonical_request = format!(
            "GET\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\nhost\nUNSIGNED-PAYLOAD"
        );
        let canonical_request_hash = hex::encode(sha256_hash(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_request_hash}");
        let signing_key = self.derive_signing_key(&date_stamp, region, service);
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        format!("https://{host}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}")
    }

    /// 构建 AWS Signature V4 签名的请求头
    fn build_signed_headers(
        &self,
        method: &str,
        url: &str,
        content_type: &str,
        payload_hash: &str,
        now: &chrono::DateTime<Utc>,
    ) -> anyhow::Result<reqwest::header::HeaderMap> {
        let R2Backend::S3 { access_key, .. } = &self.backend else {
            anyhow::bail!("S3 signing is unavailable in R2 Worker mode");
        };
        let url_parts = url
            .strip_prefix("https://")
            .context("invalid R2 endpoint URL")?;
        let (host, path) = url_parts
            .split_once('/')
            .map(|(h, p)| (h, format!("/{}", p)))
            .unwrap_or((url_parts, "/".to_string()));
        let canonical_path = uri_encode_path(&path);

        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = "auto";
        let service = "s3";

        // 构建 canonical request
        let canonical_headers = format!(
            "content-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            content_type, host, payload_hash, amz_date
        );
        let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";

        let canonical_request = format!(
            "{method}\n{canonical_path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );

        // 构建 string to sign
        let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
        let canonical_request_hash = hex::encode(sha256_hash(canonical_request.as_bytes()));
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_request_hash}");

        // 计算签名
        let signing_key = self.derive_signing_key(&date_stamp, region, service);
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

        let auth_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            access_key, credential_scope, signed_headers, signature
        );

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            auth_header
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid AUTHORIZATION header: {e}"))?,
        );
        headers.insert(
            "x-amz-date",
            amz_date
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid x-amz-date header: {e}"))?,
        );
        headers.insert(
            "x-amz-content-sha256",
            payload_hash
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid x-amz-content-sha256 header: {e}"))?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            content_type
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid CONTENT_TYPE header: {e}"))?,
        );
        headers.insert(
            reqwest::header::HOST,
            host.parse()
                .map_err(|e| anyhow::anyhow!("invalid HOST header: {e}"))?,
        );

        Ok(headers)
    }

    fn derive_signing_key(&self, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
        let R2Backend::S3 { secret_key, .. } = &self.backend else {
            unreachable!();
        };
        let k_date = hmac_sha256(
            format!("AWS4{secret_key}").as_bytes(),
            date_stamp.as_bytes(),
        );
        let k_region = hmac_sha256(&k_date, region.as_bytes());
        let k_service = hmac_sha256(&k_region, service.as_bytes());
        hmac_sha256(&k_service, b"aws4_request")
    }
}

fn sha256_hash(data: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn normalize_base_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn uri_encode_path(path: &str) -> String {
    path.split('/')
        .map(uri_encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn uri_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            _ => encoded.push_str(&format!("%{:02X}", *byte)),
        }
    }
    encoded
}

/// 文件名清理：只保留安全字符
fn sanitize_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c == '-')
        .to_string();

    if sanitized.is_empty() {
        "file".to_string()
    } else {
        sanitized
    }
}

/// 根据文件扩展名推断 MIME 类型
pub fn guess_content_type(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".avi") {
        "video/x-msvideo"
    } else if lower.ends_with(".mov") {
        "video/quicktime"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".mkv") {
        "video/x-matroska"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else if lower.ends_with(".m4a") {
        "audio/mp4"
    } else if lower.ends_with(".flac") {
        "audio/flac"
    } else if lower.ends_with(".replay") {
        "application/vnd.gokz.replay"
    } else {
        "application/octet-stream"
    }
}

/// 判断文件是否为允许的类型
pub fn is_allowed_file_type(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    // 录像
    lower.ends_with(".mp4")
        || lower.ends_with(".avi")
        || lower.ends_with(".mov")
        || lower.ends_with(".webm")
        || lower.ends_with(".mkv")
        // 图片
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        // 音频
        || lower.ends_with(".mp3")
        || lower.ends_with(".wav")
        || lower.ends_with(".ogg")
        || lower.ends_with(".m4a")
        || lower.ends_with(".flac")
        // GOKZ / Source replay files
        || lower.ends_with(".replay")
        || lower.ends_with(".dem")
}

/// 基于文件扩展名返回文件分类
pub fn file_category(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".mp4")
        || lower.ends_with(".avi")
        || lower.ends_with(".mov")
        || lower.ends_with(".webm")
        || lower.ends_with(".mkv")
    {
        "video"
    } else if lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
    {
        "image"
    } else if lower.ends_with(".mp3")
        || lower.ends_with(".wav")
        || lower.ends_with(".ogg")
        || lower.ends_with(".m4a")
        || lower.ends_with(".flac")
    {
        "audio"
    } else if lower.ends_with(".replay") {
        "replay"
    } else if lower.ends_with(".dem") {
        "demo"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config() -> Config {
        Config::from_env()
    }

    #[tokio::test]
    #[ignore = "requires real R2 credentials and network access"]
    async fn test_r2_connection_and_upload() {
        let config = test_config();
        let r2 = match R2Storage::new(&config) {
            Some(r2) => r2,
            None => {
                eprintln!("R2 配置不完整，跳过测试");
                return;
            }
        };

        let test_data = b"Hello, R2! This is a test file for LumiAdmin ban appeal upload.".to_vec();
        let appeal_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let file_name = "test-upload.txt";
        let content_type = "text/plain";

        println!("正在上传测试文件到 R2...");
        match &r2.backend {
            R2Backend::Worker { base_url, .. } => println!("  Worker: {base_url}"),
            R2Backend::S3 {
                endpoint, bucket, ..
            } => {
                println!("  Endpoint: {endpoint}");
                println!("  Bucket: {bucket}");
            }
        }
        println!("  Key: appeals/{}/{}", appeal_id, file_name);

        match r2
            .upload(appeal_id, file_name, content_type, test_data)
            .await
        {
            Ok(key) => {
                println!("上传成功! storage_key: {key}");
                let presigned = r2.presigned_url(&key, 3600);
                println!(
                    "预签名 URL (前120字符): {}...",
                    &presigned[..presigned.len().min(120)]
                );
            }
            Err(e) => {
                panic!("R2 上传失败: {e}");
            }
        }
    }
}
