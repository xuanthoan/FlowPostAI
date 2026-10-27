use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

const IMAGE_TIMEOUT_SECS: u64 = 180;
const IMAGE_DOWNLOAD_TIMEOUT_SECS: u64 = 120;
pub const IMAGE_PROMPT_MAX_BODY_CHARS: usize = 4000;
pub const IMAGE_PROMPT_MAX_TOTAL_CHARS: usize = 6000;
pub const ALLOWED_IMAGE_SIZES: [&str; 3] = ["1024x1024", "1024x1536", "1536x1024"];
pub const DEFAULT_IMAGE_SIZE: &str = "1024x1024";

const IMAGE_PROMPT_HEADER: &str = "Minh hoạ cho bài viết sau (không có chữ trong ảnh):";
const NO_IMAGE_DATA: &str = "Phản hồi tạo ảnh không có dữ liệu ảnh.";

fn invalid_size_message() -> String {
    format!(
        "Kích thước ảnh không hợp lệ. Chỉ hỗ trợ: {}",
        ALLOWED_IMAGE_SIZES.join(", ")
    )
}

// --- Config ---

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
    #[serde(default = "default_image_size")]
    pub size: String,
}

fn default_image_size() -> String {
    DEFAULT_IMAGE_SIZE.to_string()
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            provider_id: None,
            model: None,
            save_dir: None,
            style_id: None,
            size: default_image_size(),
        }
    }
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Trim user input and coerce an unsupported size back to the default.
/// Used on read only; writes validate the size instead of coercing it.
fn normalize_image_config(config: ImageConfig) -> ImageConfig {
    let size = config.size.trim().to_string();
    let size = if ALLOWED_IMAGE_SIZES.contains(&size.as_str()) {
        size
    } else {
        DEFAULT_IMAGE_SIZE.to_string()
    };
    ImageConfig {
        provider_id: trim_optional(config.provider_id),
        model: trim_optional(config.model),
        save_dir: trim_optional(config.save_dir),
        style_id: trim_optional(config.style_id),
        size,
    }
}

fn validate_image_size(size: &str) -> Result<String, String> {
    let trimmed = size.trim();
    if ALLOWED_IMAGE_SIZES.contains(&trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(invalid_size_message())
    }
}

fn image_config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("image-config.json"))
}

/// Never errors: a missing, unreadable, empty or corrupt file yields defaults
/// and the file is left untouched.
fn read_image_config_at(path: &Path) -> ImageConfig {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return ImageConfig::default(),
    };
    if contents.trim().is_empty() {
        return ImageConfig::default();
    }
    match serde_json::from_str::<ImageConfig>(&contents) {
        Ok(config) => normalize_image_config(config),
        Err(_) => ImageConfig::default(),
    }
}

fn write_image_config_at(path: &Path, config: &ImageConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_image_config<R: Runtime>(app: &AppHandle<R>) -> Result<ImageConfig, String> {
    let path = image_config_path(app)?;
    Ok(read_image_config_at(&path))
}

/// Nếu image-config đang trỏ tới provider bị xóa, bỏ providerId + model
/// (model của provider cũ không dùng được cho provider fallback).
pub(crate) fn clear_image_provider_at(path: &Path, provider_id: &str) -> Result<(), String> {
    let mut config = read_image_config_at(path);
    if config.provider_id.as_deref() != Some(provider_id) {
        return Ok(());
    }
    config.provider_id = None;
    config.model = None;
    write_image_config_at(path, &config)
}

pub(crate) fn clear_image_provider<R: Runtime>(
    app: &AppHandle<R>,
    provider_id: &str,
) -> Result<(), String> {
    let path = image_config_path(app)?;
    clear_image_provider_at(&path, provider_id)
}

// --- Commands ---

#[tauri::command]
pub fn get_image_config<R: Runtime>(app: AppHandle<R>) -> Result<ImageConfig, String> {
    read_image_config(&app)
}

#[tauri::command]
pub fn set_image_config<R: Runtime>(
    app: AppHandle<R>,
    config: ImageConfig,
) -> Result<ImageConfig, String> {
    validate_image_size(&config.size)?;
    let config = normalize_image_config(config);
    let path = image_config_path(&app)?;
    write_image_config_at(&path, &config)?;
    Ok(config)
}

#[tauri::command]
pub fn pick_image_folder() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Chọn thư mục lưu ảnh AI")
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string()))
}

// --- Pure helpers ---

pub(crate) fn build_image_prompt(style_content: &str, title: &str, body: &str) -> String {
    const HEADER: &str = IMAGE_PROMPT_HEADER;
    const TITLE_MAX_CHARS: usize = 1000;

    let style = style_content.trim();
    let title = title.trim();
    let header_chars = HEADER.chars().count();

    // Reserve the header, the separators, and the (capped) title so the composed
    // prompt can never be dominated by an unbounded style blob.
    let title_budget = if title.is_empty() { 0 } else { TITLE_MAX_CHARS };
    let style_budget =
        IMAGE_PROMPT_MAX_TOTAL_CHARS.saturating_sub(header_chars + 4 + title_budget + 1);
    let style_part: String = style.chars().take(style_budget).collect();
    let title_part: String = title.chars().take(title_budget).collect();

    let prefix = if title_part.is_empty() {
        format!("{}\n\n{}\n\n", style_part, HEADER)
    } else {
        format!("{}\n\n{}\n{}\n\n", style_part, HEADER, title_part)
    };

    let body_budget = IMAGE_PROMPT_MAX_BODY_CHARS
        .min(IMAGE_PROMPT_MAX_TOTAL_CHARS.saturating_sub(prefix.chars().count()));
    let body_part: String = body.trim().chars().take(body_budget).collect();

    let prompt = format!("{}{}", prefix, body_part);
    prompt.chars().take(IMAGE_PROMPT_MAX_TOTAL_CHARS).collect()
}

pub(crate) fn encode_image_request(model: &str, prompt: &str, size: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "prompt": prompt,
        "n": 1,
        "size": size,
        "response_format": "b64_json",
    })
}

pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    fn invalid() -> String {
        "Dữ liệu ảnh base64 không hợp lệ".to_string()
    }

    // Strip a leading `data:<mime>;base64,` prefix when present.
    let mut payload = input.trim();
    if payload.starts_with("data:") {
        match payload.find(',') {
            Some(idx) => payload = &payload[idx + 1..],
            None => return Err(invalid()),
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(payload.len() / 4 * 3 + 3);
    let mut group: [u8; 4] = [0; 4];
    let mut filled = 0usize;
    for ch in payload.chars() {
        if ch == '=' {
            break;
        }
        if ch.is_ascii_whitespace() {
            continue;
        }
        let value: u8 = match ch {
            'A'..='Z' => ch as u8 - b'A',
            'a'..='z' => ch as u8 - b'a' + 26,
            '0'..='9' => ch as u8 - b'0' + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return Err(invalid()),
        };
        group[filled] = value;
        filled += 1;
        if filled == 4 {
            out.push((group[0] << 2) | (group[1] >> 4));
            out.push((group[1] << 4) | (group[2] >> 2));
            out.push((group[2] << 6) | group[3]);
            filled = 0;
        }
    }

    match filled {
        0 => {}
        1 => return Err(invalid()),
        2 => out.push((group[0] << 2) | (group[1] >> 4)),
        3 => {
            out.push((group[0] << 2) | (group[1] >> 4));
            out.push((group[1] << 4) | (group[2] >> 2));
        }
        _ => return Err(invalid()),
    }

    Ok(out)
}

pub(crate) fn sniff_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    None
}

pub(crate) fn generated_copy_filename(
    now: chrono::DateTime<chrono::Utc>,
    id: &str,
    ext: &str,
) -> String {
    format!(
        "flowpost-{}-{}.{}",
        now.format("%Y%m%d-%H%M%S"),
        &id[..8.min(id.len())],
        ext
    )
}

fn resolve_size(explicit: Option<&str>, configured: &str) -> Result<String, String> {
    let explicit = explicit.map(str::trim).filter(|s| !s.is_empty());
    if let Some(size) = explicit {
        if !ALLOWED_IMAGE_SIZES.contains(&size) {
            return Err(invalid_size_message());
        }
        return Ok(size.to_string());
    }
    let configured = configured.trim();
    if ALLOWED_IMAGE_SIZES.contains(&configured) {
        Ok(configured.to_string())
    } else {
        Ok(DEFAULT_IMAGE_SIZE.to_string())
    }
}

// --- HTTP ---

async fn request_image_generation(
    base: &str,
    key: &str,
    model: &str,
    prompt: &str,
    size: &str,
) -> Result<serde_json::Value, String> {
    let url = crate::providers::images_generations_url(base);
    let body = serde_json::to_string(&encode_image_request(model, prompt, size))
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(IMAGE_TIMEOUT_SECS))
        .http1_only()
        .build()
        .map_err(|e| format!("Không thể khởi tạo HTTP client: {}", e))?;
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối AI provider: {}", e))?;

    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if (200..300).contains(&status) {
        return serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| format!("Không parse được phản hồi tạo ảnh: {}", e));
    }

    match status {
        401 | 403 => Err("API key không hợp lệ hoặc hết hạn.".to_string()),
        429 => Err("Quá nhiều yêu cầu, vui lòng thử lại sau.".to_string()),
        500..=599 => Err("AI tạo ảnh đang gặp sự cố, vui lòng thử lại.".to_string()),
        _ => {
            let from_json = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|json| {
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .filter(|m| !m.trim().is_empty());
            let detail =
                from_json.unwrap_or_else(|| text.chars().take(300).collect::<String>());
            Err(format!("Lỗi tạo ảnh ({}): {}", status, detail))
        }
    }
}

async fn fetch_image_bytes(
    base: &str,
    key: &str,
    model: &str,
    prompt: &str,
    size: &str,
) -> Result<(Vec<u8>, Option<String>), String> {
    let json = request_image_generation(base, key, model, prompt, size).await?;

    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| NO_IMAGE_DATA.to_string())?;
    let first = data.first().ok_or_else(|| NO_IMAGE_DATA.to_string())?;

    let revised_prompt = first
        .get("revised_prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let Some(encoded) = first
        .get("b64_json")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok((decode_base64(encoded)?, revised_prompt));
    }

    if let Some(url) = first
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // Never send the API key to a third-party download host.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(IMAGE_DOWNLOAD_TIMEOUT_SECS))
            .http1_only()
            .build()
            .map_err(|e| format!("Không thể tải ảnh từ URL: {}", e))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Không thể tải ảnh từ URL: {}", e))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("Không thể tải ảnh từ URL: HTTP {}", status));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Không thể tải ảnh từ URL: {}", e))?;
        return Ok((bytes.to_vec(), revised_prompt));
    }

    Err(NO_IMAGE_DATA.to_string())
}

// --- Orchestration ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImagePayload {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub style_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub save_dir: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageResult {
    pub media_id: String,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub model: String,
    pub style_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_copy_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
}

#[tauri::command]
pub async fn generate_image<R: Runtime>(
    app: AppHandle<R>,
    payload: GenerateImagePayload,
) -> Result<GenerateImageResult, String> {
    if payload.title.trim().is_empty() && payload.body.trim().is_empty() {
        return Err("Chưa có nội dung bài viết để tạo ảnh.".to_string());
    }

    let cfg = read_image_config(&app)?;

    let provider_id = payload
        .provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| cfg.provider_id.clone());
    let (provider, key) = match provider_id {
        Some(id) => crate::providers::resolve_provider_by_id(&app, &id)?,
        None => crate::providers::resolve_active_provider(&app)?,
    };

    let model = crate::providers::resolve_image_model(
        &provider,
        payload.model.as_deref().or(cfg.model.as_deref()),
    );
    if model.trim().is_empty() {
        return Err(
            "Chưa cấu hình model tạo ảnh. Vào Cài đặt → tab Model AI để chọn model tạo ảnh."
                .to_string(),
        );
    }

    let styles = crate::image_styles::read_image_styles(&app)?;
    let explicit_style_id = payload
        .style_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let style = match explicit_style_id {
        Some(id) => crate::image_styles::find_image_style(&styles, &id)
            .ok_or_else(|| format!("Không tìm thấy style ảnh '{}'", id))?,
        None => {
            let configured = cfg
                .style_id
                .as_deref()
                .and_then(|id| crate::image_styles::find_image_style(&styles, id));
            match configured {
                Some(style) => style,
                None => crate::image_styles::find_image_style(
                    &styles,
                    crate::image_styles::DEFAULT_IMAGE_STYLE_ID,
                )
                .ok_or_else(|| "Không tìm thấy style ảnh mặc định".to_string())?,
            }
        }
    };

    let size = resolve_size(payload.size.as_deref(), &cfg.size)?;
    let prompt = build_image_prompt(&style.content, &payload.title, &payload.body);

    let (bytes, revised_prompt) =
        fetch_image_bytes(&provider.base_url, &key, &model, &prompt, &size).await?;

    let ext = sniff_image_extension(&bytes)
        .ok_or_else(|| {
            "Định dạng ảnh tạo ra không được hỗ trợ (chỉ PNG/JPEG/WEBP/GIF).".to_string()
        })?;
    if bytes.len() as u64 > crate::facebook::MAX_IMAGE_BYTES {
        return Err(
            "Ảnh tạo ra vượt quá 10MB, vui lòng giảm kích thước hoặc thử lại.".to_string(),
        );
    }

    let name = {
        let trimmed = payload.title.trim();
        if trimmed.is_empty() {
            format!("Anh AI.{}", ext)
        } else {
            format!("{} - AI.{}", trimmed.chars().take(60).collect::<String>(), ext)
        }
    };

    let item = crate::register_media_bytes(&app, &bytes, &name, ext, "image")?;

    let mut saved_copy_path: Option<String> = None;
    let mut copy_warning: Option<String> = None;
    let wanted = payload
        .save_dir
        .as_deref()
        .or(cfg.save_dir.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(dir) = wanted {
        let dir = PathBuf::from(dir);
        let attempt = (|| -> Result<PathBuf, String> {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let target = dir.join(generated_copy_filename(Utc::now(), &item.id, ext));
            fs::write(&target, &bytes).map_err(|e| e.to_string())?;
            Ok(target)
        })();
        match attempt {
            Ok(target) => saved_copy_path = Some(target.to_string_lossy().to_string()),
            Err(e) => {
                copy_warning =
                    Some(format!("Không thể lưu bản sao vào thư mục đã chọn: {}", e))
            }
        }
    }

    Ok(GenerateImageResult {
        media_id: item.id,
        path: item.path,
        name: item.name,
        extension: item.extension,
        size: item.size,
        model,
        style_id: style.id,
        saved_copy_path,
        copy_warning,
        revised_prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::time::{SystemTime, UNIX_EPOCH};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0,
        0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAAElFTkSuQmCC";

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_image_prompt_includes_style_title_and_body() {
        let prompt = build_image_prompt("  style nội dung  ", "  Tiêu đề  ", "  nội dung bài  ");
        assert_eq!(
            prompt,
            "style nội dung\n\nMinh hoạ cho bài viết sau (không có chữ trong ảnh):\nTiêu đề\n\nnội dung bài"
        );
    }

    #[test]
    fn build_image_prompt_omits_blank_title() {
        let prompt = build_image_prompt("st", "   ", "body");
        assert_eq!(
            prompt,
            "st\n\nMinh hoạ cho bài viết sau (không có chữ trong ảnh):\n\nbody"
        );
    }

    #[test]
    fn build_image_prompt_truncates_body_to_4000() {
        let body = "x".repeat(5000);
        let prompt = build_image_prompt("style", "", &body);
        let prefix = "style\n\nMinh hoạ cho bài viết sau (không có chữ trong ảnh):\n\n";
        assert_eq!(prompt, format!("{}{}", prefix, "x".repeat(4000)));
        let body_part = prompt.strip_prefix(prefix).unwrap();
        assert_eq!(body_part.chars().count(), IMAGE_PROMPT_MAX_BODY_CHARS);
    }

    #[test]
    fn build_image_prompt_caps_total_to_6000() {
        let style = "s".repeat(1000);
        let title = "t".repeat(1000);
        let body = "b".repeat(20_000);
        let prompt = build_image_prompt(&style, &title, &body);
        assert_eq!(prompt.chars().count(), IMAGE_PROMPT_MAX_TOTAL_CHARS);
        assert!(prompt.starts_with(&style));
        assert!(prompt.contains(&title));
    }

    #[test]
    fn build_image_prompt_caps_total_with_huge_style() {
        let style = "s".repeat(7000);
        let title = "t".repeat(7000);
        let body = "b".repeat(5000);
        let prompt = build_image_prompt(&style, &title, &body);
        assert!(prompt.chars().count() <= IMAGE_PROMPT_MAX_TOTAL_CHARS);
        assert!(prompt.contains("Minh hoạ cho bài viết sau"));
        // A huge style blob must not squeeze out the header/title.
        assert!(prompt.chars().any(|c| c != 's'));
    }

    #[test]
    fn build_image_prompt_caps_total_with_huge_title() {
        let style = "phong cách";
        let title = "t".repeat(7000);
        let body = "b".repeat(5000);
        let prompt = build_image_prompt(style, &title, &body);
        assert!(prompt.chars().count() <= IMAGE_PROMPT_MAX_TOTAL_CHARS);
        assert!(prompt.contains("Minh hoạ cho bài viết sau"));
    }

    #[test]
    fn build_image_prompt_body_still_capped_at_4000() {
        let body = "b".repeat(20_000);

        let normal = build_image_prompt("phong cách", "Tiêu đề", &body);
        let normal_run = normal.chars().rev().take_while(|c| *c == 'b').count();
        assert_eq!(normal_run, IMAGE_PROMPT_MAX_BODY_CHARS);

        let huge_style = "s".repeat(7000);
        let huge = build_image_prompt(&huge_style, "", &body);
        let huge_run = huge.chars().rev().take_while(|c| *c == 'b').count();
        assert!(huge_run <= IMAGE_PROMPT_MAX_BODY_CHARS);
    }

    #[test]
    fn encode_image_request_shape() {
        let value = encode_image_request("img-model", "a prompt", "1024x1536");
        assert_eq!(
            value,
            serde_json::json!({
                "model": "img-model",
                "prompt": "a prompt",
                "n": 1,
                "size": "1024x1536",
                "response_format": "b64_json",
            })
        );
        assert_eq!(value.get("n").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            value.get("response_format").and_then(|v| v.as_str()),
            Some("b64_json")
        );
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn decode_base64_known_vectors() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello".to_vec());
        assert_eq!(decode_base64("aGVsbG8").unwrap(), b"hello".to_vec());
        assert_eq!(decode_base64("aGVsbG8h").unwrap(), b"hello!".to_vec());
        assert_eq!(decode_base64("aGlsbG8=").unwrap(), b"hillo".to_vec());
        // URL-safe alphabet maps '-' -> 62 and '_' -> 63.
        assert_eq!(decode_base64("-_8").unwrap(), vec![0xFB, 0xFF]);
        assert_eq!(decode_base64("+/8=").unwrap(), vec![0xFB, 0xFF]);
        // Whitespace is skipped anywhere.
        assert_eq!(decode_base64("aGVs\n bG8=").unwrap(), b"hello".to_vec());
        assert_eq!(decode_base64(TINY_PNG_B64).unwrap(), TINY_PNG.to_vec());
        // Empty input is a valid empty payload.
        assert!(decode_base64("").unwrap().is_empty());
    }

    #[test]
    fn decode_base64_strips_data_uri_prefix() {
        assert_eq!(
            decode_base64("data:image/png;base64,aGVsbG8=").unwrap(),
            b"hello".to_vec()
        );
        let data_uri = format!("data:image/png;base64,{}", TINY_PNG_B64);
        assert_eq!(decode_base64(&data_uri).unwrap(), TINY_PNG.to_vec());
    }

    #[test]
    fn decode_base64_rejects_invalid_chars() {
        let message = "Dữ liệu ảnh base64 không hợp lệ";
        assert_eq!(decode_base64("aGV$bG8").unwrap_err(), message);
        assert_eq!(decode_base64("aGVsbG8*").unwrap_err(), message);
        // A trailing single leftover symbol is not a valid group.
        assert_eq!(decode_base64("aGVsbG8Aextra").unwrap_err(), message);
        assert_eq!(decode_base64("abcde").unwrap_err(), message);
    }

    #[test]
    fn sniff_image_extension_detects_png_jpeg_webp_gif_and_unknown() {
        assert_eq!(sniff_image_extension(TINY_PNG), Some("png"));
        assert_eq!(
            sniff_image_extension(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("jpg")
        );
        let mut webp = Vec::new();
        webp.extend_from_slice(b"RIFF");
        webp.extend_from_slice(&[0x24, 0x00, 0x00, 0x00]);
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(sniff_image_extension(&webp), Some("webp"));
        assert_eq!(sniff_image_extension(b"GIF87a...."), Some("gif"));
        assert_eq!(sniff_image_extension(b"GIF89a...."), Some("gif"));
        assert_eq!(sniff_image_extension(b"not an image"), None);
        assert_eq!(sniff_image_extension(&[]), None);
    }

    #[test]
    fn resolve_size_accepts_allowlist_and_rejects_unknown() {
        assert_eq!(resolve_size(None, "1024x1024").unwrap(), "1024x1024");
        assert_eq!(resolve_size(Some("1024x1536"), "1024x1024").unwrap(), "1024x1536");
        assert_eq!(resolve_size(Some("  1536x1024  "), "1024x1024").unwrap(), "1536x1024");
        assert_eq!(resolve_size(Some("   "), "1024x1536").unwrap(), "1024x1536");
        // Unknown configured values fall back to the default.
        assert_eq!(resolve_size(None, "512x512").unwrap(), DEFAULT_IMAGE_SIZE);
        assert_eq!(resolve_size(None, "  ").unwrap(), DEFAULT_IMAGE_SIZE);
        // Explicit values must be rejected, never coerced.
        assert_eq!(
            resolve_size(Some("512x512"), "1024x1024").unwrap_err(),
            "Kích thước ảnh không hợp lệ. Chỉ hỗ trợ: 1024x1024, 1024x1536, 1536x1024"
        );
    }

    #[test]
    fn image_config_defaults_when_missing() {
        let dir = unique_temp_dir("flowpost_image_cfg_missing");
        let path = dir.join("image-config.json");
        let config = read_image_config_at(&path);
        assert!(config.provider_id.is_none());
        assert!(config.model.is_none());
        assert!(config.save_dir.is_none());
        assert!(config.style_id.is_none());
        assert_eq!(config.size, DEFAULT_IMAGE_SIZE);
        assert!(!path.exists(), "read must not create the file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn image_config_corrupt_json_returns_defaults() {
        let dir = unique_temp_dir("flowpost_image_cfg_corrupt");
        let path = dir.join("image-config.json");
        fs::write(&path, "{not json").unwrap();
        let config = read_image_config_at(&path);
        assert_eq!(config.size, DEFAULT_IMAGE_SIZE);
        assert!(config.provider_id.is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");

        fs::write(&path, "   ").unwrap();
        let empty = read_image_config_at(&path);
        assert_eq!(empty.size, DEFAULT_IMAGE_SIZE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn set_image_config_rejects_invalid_size() {
        assert_eq!(
            validate_image_size("512x512").unwrap_err(),
            "Kích thước ảnh không hợp lệ. Chỉ hỗ trợ: 1024x1024, 1024x1536, 1536x1024"
        );
        assert!(validate_image_size("").is_err());
        for size in ALLOWED_IMAGE_SIZES {
            assert_eq!(validate_image_size(size).unwrap(), size);
        }
        assert_eq!(validate_image_size("  1024x1024  ").unwrap(), "1024x1024");
    }

    #[test]
    fn image_config_normalize_trims_and_clears_empty() {
        let config = normalize_image_config(ImageConfig {
            provider_id: Some("  provider  ".into()),
            model: Some("   ".into()),
            save_dir: Some("  ".into()),
            style_id: Some("  cinematic  ".into()),
            size: " 1024x1536 ".into(),
        });
        assert_eq!(config.provider_id.as_deref(), Some("provider"));
        assert_eq!(config.model, None);
        assert_eq!(config.save_dir, None);
        assert_eq!(config.style_id.as_deref(), Some("cinematic"));
        assert_eq!(config.size, "1024x1536");

        let coerced = normalize_image_config(ImageConfig {
            size: "512x512".into(),
            ..Default::default()
        });
        assert_eq!(coerced.size, DEFAULT_IMAGE_SIZE);
    }

    #[test]
    fn generated_copy_filename_shape() {
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(
            generated_copy_filename(now, "abcdefgh-1234-5678-9abc-def012345678", "png"),
            "flowpost-20260102-030405-abcdefgh.png"
        );
        assert_eq!(
            generated_copy_filename(now, "abc", "jpg"),
            "flowpost-20260102-030405-abc.jpg"
        );
    }

    #[test]
    fn image_config_roundtrip_camel_case() {
        let dir = unique_temp_dir("flowpost_image_cfg_roundtrip");
        let path = dir.join("image-config.json");
        let config = ImageConfig {
            provider_id: Some("acme".into()),
            model: Some("img-model".into()),
            save_dir: Some("C:\\images".into()),
            style_id: Some("minimal".into()),
            size: "1024x1536".into(),
        };
        write_image_config_at(&path, &config).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"providerId\""), "raw={}", raw);
        assert!(raw.contains("\"model\""));
        assert!(raw.contains("\"saveDir\""), "raw={}", raw);
        assert!(raw.contains("\"styleId\""), "raw={}", raw);
        assert!(raw.contains("\"size\""));
        assert!(!path.with_extension("json.tmp").exists());

        let read = read_image_config_at(&path);
        assert_eq!(read.provider_id.as_deref(), Some("acme"));
        assert_eq!(read.model.as_deref(), Some("img-model"));
        assert_eq!(read.save_dir.as_deref(), Some("C:\\images"));
        assert_eq!(read.style_id.as_deref(), Some("minimal"));
        assert_eq!(read.size, "1024x1536");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn image_config_old_json_without_size_uses_default() {
        let dir = unique_temp_dir("flowpost_image_cfg_old");
        let path = dir.join("image-config.json");
        fs::write(&path, r#"{"providerId":"acme","model":"img-model"}"#).unwrap();
        let config = read_image_config_at(&path);
        assert_eq!(config.provider_id.as_deref(), Some("acme"));
        assert_eq!(config.model.as_deref(), Some("img-model"));
        assert_eq!(config.save_dir, None);
        assert_eq!(config.style_id, None);
        assert_eq!(config.size, DEFAULT_IMAGE_SIZE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clear_image_provider_clears_matching_provider_and_model() {
        let dir = unique_temp_dir("flowpost_image_cfg_clear_provider");
        let path = dir.join("image-config.json");
        let config = ImageConfig {
            provider_id: Some("p2".into()),
            model: Some("m".into()),
            save_dir: Some("D".into()),
            style_id: Some("s".into()),
            size: "1024x1024".into(),
        };
        write_image_config_at(&path, &config).unwrap();

        clear_image_provider_at(&path, "other").unwrap();
        let unchanged = read_image_config_at(&path);
        assert_eq!(unchanged.provider_id.as_deref(), Some("p2"));
        assert_eq!(unchanged.model.as_deref(), Some("m"));

        clear_image_provider_at(&path, "p2").unwrap();
        let cleared = read_image_config_at(&path);
        assert_eq!(cleared.provider_id, None);
        assert_eq!(cleared.model, None);
        assert_eq!(cleared.save_dir.as_deref(), Some("D"));
        assert_eq!(cleared.style_id.as_deref(), Some("s"));
        assert_eq!(cleared.size, "1024x1024");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- wiremock ---

    #[tokio::test]
    async fn generate_image_posts_bearer_and_expected_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"b64_json": TINY_PNG_B64}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let base = format!("{}/v1", server.uri());
        let (bytes, revised_prompt) =
            fetch_image_bytes(&base, "test-key", "img-model", "a prompt", "1024x1024")
                .await
                .unwrap();
        assert_eq!(bytes, TINY_PNG.to_vec());
        assert!(revised_prompt.is_none());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = String::from_utf8_lossy(&requests[0].body).to_string();
        assert!(
            body.contains("\"response_format\":\"b64_json\""),
            "body={}",
            body
        );
        assert!(body.contains("\"n\":1"), "body={}", body);
        assert!(body.contains("\"model\":\"img-model\""), "body={}", body);
        assert!(body.contains("\"prompt\":\"a prompt\""), "body={}", body);
        assert!(body.contains("\"size\":\"1024x1024\""), "body={}", body);
    }

    #[tokio::test]
    async fn generate_image_maps_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"message": "invalid key"}
            })))
            .mount(&server)
            .await;

        let err = fetch_image_bytes(
            &format!("{}/v1", server.uri()),
            "test-key",
            "img-model",
            "prompt",
            "1024x1024",
        )
        .await
        .unwrap_err();
        assert_eq!(err, "API key không hợp lệ hoặc hết hạn.");
    }

    #[tokio::test]
    async fn generate_image_maps_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let err = fetch_image_bytes(
            &format!("{}/v1", server.uri()),
            "test-key",
            "img-model",
            "prompt",
            "1024x1024",
        )
        .await
        .unwrap_err();
        assert_eq!(err, "Quá nhiều yêu cầu, vui lòng thử lại sau.");
    }

    #[tokio::test]
    async fn generate_image_follows_url_without_authorization() {
        let download = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/img.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(TINY_PNG.to_vec()))
            .expect(1)
            .mount(&download)
            .await;

        let api = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"url": format!("{}/img.png", download.uri())}]
            })))
            .expect(1)
            .mount(&api)
            .await;

        let (bytes, revised_prompt) = fetch_image_bytes(
            &format!("{}/v1", api.uri()),
            "test-key",
            "img-model",
            "prompt",
            "1024x1024",
        )
        .await
        .unwrap();
        assert_eq!(bytes, TINY_PNG.to_vec());
        assert!(revised_prompt.is_none());

        let requests = download.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].headers.get("authorization").is_none(),
            "download request must not carry the API key"
        );
    }
}
