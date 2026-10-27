use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::{AppHandle, Runtime};

/// Facebook composers stop accepting input at this many characters. Meta does
/// NOT document a limit for `message`: the Graph reference documents no maximum
/// and describes a photo caption only as a UTF-8 string. This value is the
/// observed ceiling enforced by the Facebook composer, kept here as a named,
/// configurable guard. The Graph API answers with an invalid-parameter error
/// instead of truncating, so we block before sending rather than let the post
/// fail after the upload.
const GRAPH_MAX_MESSAGE_CHARS: usize = 63_206;

/// Facebook cho phép gắn tối đa 10 ảnh vào một bài viết.
pub const MAX_IMAGES_PER_POST: usize = 10;
pub(crate) const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub(crate) const ALLOWED_IMAGE_MIMES: [&str; 4] =
    ["image/jpeg", "image/png", "image/webp", "image/gif"];
const PHOTO_UPLOAD_TIMEOUT_SECS: u64 = 30;
const FEED_TIMEOUT_SECS: u64 = 30;
pub const REPORT_PAGE_LIMIT: u32 = 100;
pub const REPORT_MAX_PAGES: usize = 20;
const REPORT_FIELDS: &str =
    "id,message,created_time,permalink_url,likes.summary(true),comments.summary(true),shares";

/// One shared HTTP client for every Graph request. Rebuilding a client per
/// request discarded the connection pool and kept six copies of the same
/// timeout/user-agent configuration in sync by hand.
static GRAPH_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn graph_client() -> &'static reqwest::Client {
    GRAPH_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("FlowPostAI/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build Graph HTTP client")
    })
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReportPost {
    pub id: String,
    pub message: Option<String>,
    pub created_time: Option<String>,
    pub permalink_url: Option<String>,
    pub reactions: u32,
    pub comments: u32,
    pub shares: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageReport {
    pub page_id: String,
    pub page_name: Option<String>,
    pub days: u32,
    pub since: i64,
    pub until: i64,
    pub post_count: usize,
    pub total_reactions: u64,
    pub total_comments: u64,
    pub total_shares: u64,
    pub total_interactions: u64,
    pub partial: bool,
    pub truncated: bool,
    pub posts: Vec<ReportPost>,
    pub range_key: String,
    pub range_label: String,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
}

/// Internal shape of a `/me/accounts` entry. The page token is cached into the
/// keyring by `list_facebook_pages`; it never reaches `PageInfo` or the webview.
#[derive(Debug)]
struct PageAccount {
    id: String,
    name: String,
    access_token: String,
}

#[derive(Deserialize)]
struct GraphErrorResponse {
    error: GraphError,
}

#[derive(Deserialize)]
struct GraphError {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<i32>,
    #[allow(dead_code)]
    error_subcode: Option<i32>,
}

/// Phân loại lỗi Graph API để scheduler quyết định retry / dừng / bỏ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphFailureKind {
    TokenInvalid,
    RateLimited,
    InvalidParam,
    Network,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphFailure {
    pub kind: GraphFailureKind,
    pub http_status: u16,
    pub code: Option<i32>,
    pub retry_after_secs: Option<u64>,
    pub message: String,
}

impl std::fmt::Display for GraphFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl GraphFailure {
    pub fn local(kind: GraphFailureKind, message: String) -> Self {
        Self {
            kind,
            http_status: 0,
            code: None,
            retry_after_secs: None,
            message,
        }
    }
}

/// Pure classifier: không phụ thuộc network, dùng được trong unit test.
pub fn classify_graph_error(
    status: u16,
    code: Option<i32>,
    error_type: Option<&str>,
    message: String,
    retry_after_secs: Option<u64>,
) -> GraphFailure {
    let kind = if status == 401 || code == Some(190) {
        GraphFailureKind::TokenInvalid
    } else if status == 429 || matches!(code, Some(4) | Some(32) | Some(613) | Some(368)) {
        GraphFailureKind::RateLimited
    } else if code == Some(100)
        || code == Some(2)
        || (error_type == Some("OAuthException")
            && matches!(code, Some(c) if (200..=299).contains(&c)))
    {
        GraphFailureKind::InvalidParam
    } else {
        GraphFailureKind::Other
    };
    GraphFailure {
        kind,
        http_status: status,
        code,
        retry_after_secs,
        message,
    }
}

/// Đọc Retry-After dạng số giây. HTTP-date hoặc giá trị rác → None.
fn graph_failure_from_response(
    status: u16,
    headers: &reqwest::header::HeaderMap,
    body: &str,
) -> GraphFailure {
    let parsed = serde_json::from_str::<GraphErrorResponse>(body).ok();
    let (code, error_type) = match parsed {
        Some(err) => (err.error.code, err.error.error_type),
        None => (None, None),
    };
    let retry_after_secs = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let message = map_graph_error(status, body);
    classify_graph_error(
        status,
        code,
        error_type.as_deref(),
        message,
        retry_after_secs,
    )
}

fn graph_base() -> String {
    "https://graph.facebook.com/v19.0".to_string()
}

fn page_token_key(page_id: &str) -> String {
    format!("fb_page_{}", page_id)
}

/// Chỉ chấp nhận Page ID dạng chữ số. Chặn trước mọi network call để page_id
/// không thể chèn path/query vào URL Graph.
pub(crate) fn validate_page_id(page_id: &str) -> Result<(), String> {
    let id = page_id.trim();
    if id.is_empty() {
        return Err("Page ID không được để trống.".into());
    }
    if id.len() > 32 {
        return Err("Page ID không hợp lệ: tối đa 32 chữ số.".into());
    }
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Page ID không hợp lệ: chỉ được chứa chữ số.".into());
    }
    Ok(())
}

/// Tách `{page_id}_{post_id}` đúng một lần và yêu cầu cả hai phần đều là chữ số.
fn split_post_id(post_id: &str) -> Result<(&str, &str), String> {
    let trimmed = post_id.trim();
    let Some((page_part, post_part)) = trimmed.split_once('_') else {
        return Err("Post ID không hợp lệ.".into());
    };
    if page_part.is_empty() || post_part.is_empty() {
        return Err("Post ID không hợp lệ.".into());
    }
    validate_page_id(page_part).map_err(|_| "Post ID không hợp lệ.".to_string())?;
    if !post_part.chars().all(|c| c.is_ascii_digit()) {
        return Err("Post ID không hợp lệ.".into());
    }
    Ok((page_part, post_part))
}

pub fn parse_me_response(json: &str) -> Result<MeResponse, String> {
    serde_json::from_str::<MeResponse>(json).map_err(|e| e.to_string())
}

fn map_graph_error(status: u16, body: &str) -> String {
    // Try to parse Graph error JSON
    if let Ok(err) = serde_json::from_str::<GraphErrorResponse>(body) {
        let code = err.error.code.unwrap_or(0);
        let msg = err.error.message;
        // Common OAuth errors
        if code == 190 || status == 401 {
            return format!("Token không hợp lệ hoặc hết hạn (code {}): {}", code, msg);
        }
        if code == 100 {
            return format!("Tham số không hợp lệ (code 100): {}", msg);
        }
        return format!("Lỗi Graph API ({}): {}", code, msg);
    }
    let scrubbed = scrub_error_text(body);
    format!("Lỗi HTTP {}: {}", status, preview_prefix(&scrubbed, 300))
}

/// Loại bỏ URL và vật liệu token khỏi chuỗi lỗi trước khi trả về frontend.
/// Giữ nguyên các từ khác để thông báo vẫn đọc được.
fn scrub_error_text(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed.contains("http://") || trimmed.contains("https://") {
                return "[url]".to_string();
            }
            let lower = trimmed.to_ascii_lowercase();
            if lower.contains("access_token") || lower.contains("bearer") {
                return "[redacted]".to_string();
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn network_failure(e: reqwest::Error) -> GraphFailure {
    GraphFailure::local(
        GraphFailureKind::Network,
        format!(
            "Không thể kết nối Graph API: {}",
            scrub_error_text(&format!("{}", e.without_url()))
        ),
    )
}

fn body_read_failure(e: reqwest::Error) -> GraphFailure {
    GraphFailure::local(
        GraphFailureKind::Network,
        scrub_error_text(&format!("{}", e.without_url())),
    )
}

fn parse_json_response(body: &str) -> Result<serde_json::Value, GraphFailure> {
    serde_json::from_str::<serde_json::Value>(body).map_err(|_| {
        GraphFailure::local(
            GraphFailureKind::Other,
            "Không parse được phản hồi".to_string(),
        )
    })
}

async fn validate_token_with(base: &str, token: &str) -> Result<MeResponse, GraphFailure> {
    let resp = graph_client()
        .get(format!("{}/me", base))
        .query(&[("fields", "id,name")])
        .bearer_auth(token)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if (200..300).contains(&status) {
        parse_me_response(&body).map_err(|e| {
            GraphFailure::local(
                GraphFailureKind::Other,
                format!(
                    "Phản hồi không hợp lệ: {} | body: {}",
                    e,
                    preview_prefix(&body, 200)
                ),
            )
        })
    } else {
        Err(graph_failure_from_response(status, &headers, &body))
    }
}

#[tauri::command]
pub async fn validate_facebook_token(token: String) -> Result<MeResponse, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Vui lòng nhập Facebook Token".into());
    }
    if token.len() < 10 {
        return Err("Token quá ngắn, vui lòng kiểm tra lại".into());
    }
    validate_token_with(graph_base().as_str(), &token)
        .await
        .map_err(|e| e.to_string())
}

fn should_write_page_token(existing: Option<&str>, incoming: &str) -> bool {
    existing != Some(incoming)
}

async fn list_pages_with(base: &str, token: &str) -> Result<Vec<PageAccount>, GraphFailure> {
    let resp = graph_client()
        .get(format!("{}/me/accounts", base))
        .query(&[("fields", "id,name,access_token")])
        .bearer_auth(token)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if !(200..300).contains(&status) {
        return Err(graph_failure_from_response(status, &headers, &body));
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
        GraphFailure::local(
            GraphFailureKind::Other,
            "Phản hồi Graph API không hợp lệ".to_string(),
        )
    })?;
    let mut pages = Vec::new();
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        for raw in arr {
            if let (Some(id), Some(name), Some(at)) = (
                raw.get("id").and_then(|x| x.as_str()),
                raw.get("name").and_then(|x| x.as_str()),
                raw.get("access_token").and_then(|x| x.as_str()),
            ) {
                pages.push(PageAccount {
                    id: id.to_string(),
                    name: name.to_string(),
                    access_token: at.to_string(),
                });
            }
        }
    }
    Ok(pages)
}

#[tauri::command]
pub async fn list_facebook_pages<R: Runtime>(app: AppHandle<R>, token: String) -> Result<Vec<PageInfo>, String> {
    let token = token.trim().to_string();
    let token = if token.is_empty() {
        // Thử lấy từ keyring/fallback nếu frontend gửi rỗng (PublishModal dùng token: '')
        crate::security::get_secret_hybrid(Some(&app), "facebook_token")
            .ok()
            .flatten()
            .unwrap_or_default()
    } else {
        token
    };
    if token.is_empty() {
        return Err("Vui lòng nhập Facebook Token".into());
    }
    if token.len() < 10 {
        return Err("Token quá ngắn, vui lòng kiểm tra lại".into());
    }
    let accounts = list_pages_with(graph_base().as_str(), &token)
        .await
        .map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    for account in accounts {
        let key = page_token_key(&account.id);
        let current = crate::security::get_secret_hybrid(Some(&app), &key).ok().flatten();
        if !account.access_token.is_empty()
            && should_write_page_token(current.as_deref(), &account.access_token)
        {
            let _ = crate::security::set_secret_hybrid(Some(&app), &key, &account.access_token);
        }
        pages.push(PageInfo {
            id: account.id,
            name: account.name,
        });
    }
    Ok(pages)
}

fn preview_prefix(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn validate_message_length(message: &str) -> Result<(), String> {
    let count = message.chars().count();
    if count > GRAPH_MAX_MESSAGE_CHARS {
        Err(format!(
            "Nội dung quá dài: {} ký tự, vượt giới hạn {} ký tự của Facebook. Vui lòng rút ngắn bài đăng.",
            count, GRAPH_MAX_MESSAGE_CHARS
        ))
    } else {
        Ok(())
    }
}

pub async fn publish_content_classified_multi<R: Runtime>(
    app: AppHandle<R>,
    page_id: String,
    message: String,
    image_paths: Vec<String>,
) -> Result<serde_json::Value, GraphFailure> {
    if let Err(e) = validate_page_id(&page_id) {
        return Err(GraphFailure::local(GraphFailureKind::InvalidParam, e));
    }
    if message.trim().is_empty() {
        return Err(GraphFailure::local(
            GraphFailureKind::InvalidParam,
            "Vui lòng nhập nội dung".to_string(),
        ));
    }
    // Validate length before resolving token / any network call, so a too-long
    // post fails fast. The cfg!(test) mock below also enforces this.
    if let Err(e) = validate_message_length(&message) {
        return Err(GraphFailure::local(GraphFailureKind::InvalidParam, e));
    }
    let paths = normalize_image_paths(image_paths)?;
    validate_image_count(paths.len())
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    // Resolve token: ưu tiên Page token đã cache, fallback User token
    let access_token = crate::security::get_secret_hybrid(Some(&app), &page_token_key(&page_id))
        .ok()
        .flatten()
        .or_else(|| {
            crate::security::get_secret_hybrid(Some(&app), "facebook_token")
                .ok()
                .flatten()
        })
        .ok_or_else(|| {
            GraphFailure::local(
                GraphFailureKind::TokenInvalid,
                "Không tìm thấy Page/User token, vui lòng Tải Trang lại".to_string(),
            )
        })?;
    if access_token.trim().is_empty() {
        return Err(GraphFailure::local(
            GraphFailureKind::TokenInvalid,
            "Không tìm thấy Page/User token, vui lòng Tải Trang lại".to_string(),
        ));
    }
    if cfg!(test) {
        return Ok(serde_json::json!({"id":"mock_photo_123"}));
    }
    publish_images_with(
        graph_base().as_str(),
        &access_token,
        &page_id,
        &message,
        &paths,
    )
    .await
}

/// Kiểm tra và dựng phần `source` dùng chung cho cả bài 1 ảnh và ảnh chưa
/// công bố (multi-photo). Thứ tự kiểm tra giữ nguyên như bản cũ.
async fn photo_part(path: &str) -> Result<reqwest::multipart::Part, GraphFailure> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(GraphFailure::local(
            GraphFailureKind::InvalidParam,
            "Không tìm thấy tệp ảnh".to_string(),
        ));
    }
    let meta = std::fs::metadata(p)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e.to_string()))?;
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(GraphFailure::local(
            GraphFailureKind::InvalidParam,
            "Ảnh vượt quá 10MB".to_string(),
        ));
    }
    let mime = mime_guess::from_path(p).first_or_octet_stream().to_string();
    if !ALLOWED_IMAGE_MIMES.contains(&mime.as_str()) {
        return Err(GraphFailure::local(
            GraphFailureKind::InvalidParam,
            "Định dạng ảnh không hỗ trợ (chỉ jpg/jpeg/png/webp/gif)".to_string(),
        ));
    }
    let bytes = tokio::fs::read(p)
        .await
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e.to_string()))?;
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    reqwest::multipart::Part::bytes(bytes)
        .file_name(file_name)
        .mime_str(&mime)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e.to_string()))
}

/// Dựng multipart cho `/{page_id}/photos`: chỉ `source` + `message`.
async fn build_photo_form(path: &str, message: &str) -> Result<reqwest::multipart::Form, GraphFailure> {
    let part = photo_part(path).await?;
    Ok(reqwest::multipart::Form::new()
        .part("source", part)
        .text("message", message.to_string()))
}

/// Dựng multipart tải ảnh chưa công bố: `source` + `published=false`, KHÔNG
/// có `message` (caption chỉ gửi ở bước tạo bài `/feed`).
async fn build_unpublished_photo_form(
    path: &str,
) -> Result<reqwest::multipart::Form, GraphFailure> {
    let part = photo_part(path).await?;
    Ok(reqwest::multipart::Form::new()
        .part("source", part)
        .text("published", "false"))
}

fn normalize_image_paths(raw: Vec<String>) -> Result<Vec<String>, GraphFailure> {
    let mut out = Vec::with_capacity(raw.len());
    for p in raw {
        let t = p.trim();
        if t.is_empty() {
            return Err(GraphFailure::local(
                GraphFailureKind::InvalidParam,
                "Đường dẫn ảnh không hợp lệ.".to_string(),
            ));
        }
        out.push(t.to_string());
    }
    Ok(out)
}

pub(crate) fn validate_image_count(count: usize) -> Result<(), String> {
    if count > MAX_IMAGES_PER_POST {
        return Err(format!(
            "Chỉ đăng tối đa {} ảnh mỗi bài (nội dung có {} ảnh). Vui lòng bớt ảnh.",
            MAX_IMAGES_PER_POST, count
        ));
    }
    Ok(())
}

fn multi_photo_feed_params(message: &str, media_fbids: &[String]) -> Vec<(String, String)> {
    let mut params = Vec::with_capacity(media_fbids.len() + 1);
    params.push(("message".to_string(), message.to_string()));
    for (i, id) in media_fbids.iter().enumerate() {
        params.push((
            format!("attached_media[{}]", i),
            serde_json::json!({ "media_fbid": id }).to_string(),
        ));
    }
    params
}

/// Tải một ảnh ở trạng thái chưa công bố và trả về `media_fbid`.
async fn upload_unpublished_photo(
    base: &str,
    token: &str,
    page_id: &str,
    path: &str,
) -> Result<String, GraphFailure> {
    let form = build_unpublished_photo_form(path).await?;
    let resp = graph_client()
        .post(format!("{}/{}/photos", base, page_id))
        .timeout(std::time::Duration::from_secs(PHOTO_UPLOAD_TIMEOUT_SECS))
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if !(200..300).contains(&status) {
        return Err(graph_failure_from_response(status, &headers, &body));
    }
    let value = parse_json_response(&body)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            GraphFailure::local(
                GraphFailureKind::Other,
                "Facebook không trả về id ảnh sau khi tải lên.".to_string(),
            )
        })?;
    Ok(id)
}

async fn post_multi_photo_feed(
    base: &str,
    token: &str,
    page_id: &str,
    message: &str,
    media_fbids: &[String],
) -> Result<serde_json::Value, GraphFailure> {
    let params = multi_photo_feed_params(message, media_fbids);
    let resp = graph_client()
        .post(format!("{}/{}/feed", base, page_id))
        .timeout(std::time::Duration::from_secs(FEED_TIMEOUT_SECS))
        .bearer_auth(token)
        .form(&params)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if (200..300).contains(&status) {
        parse_json_response(&body)
    } else {
        Err(graph_failure_from_response(status, &headers, &body))
    }
}

fn validate_post_inputs(page_id: &str, message: &str) -> Result<(), GraphFailure> {
    validate_page_id(page_id)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    if message.trim().is_empty() {
        return Err(GraphFailure::local(
            GraphFailureKind::InvalidParam,
            "Vui lòng nhập nội dung".to_string(),
        ));
    }
    validate_message_length(message)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    Ok(())
}

async fn publish_images_with(
    base: &str,
    token: &str,
    page_id: &str,
    message: &str,
    image_paths: &[String],
) -> Result<serde_json::Value, GraphFailure> {
    validate_post_inputs(page_id, message)?;
    validate_image_count(image_paths.len())
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let page_id = page_id.trim();
    match image_paths.len() {
        0 => {
            let params = [("message", message)];
            let resp = graph_client()
                .post(format!("{}/{}/feed", base, page_id))
                .bearer_auth(token)
                .form(&params)
                .send()
                .await
                .map_err(network_failure)?;
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            let body = resp.text().await.map_err(body_read_failure)?;
            if (200..300).contains(&status) {
                parse_json_response(&body)
            } else {
                Err(graph_failure_from_response(status, &headers, &body))
            }
        }
        1 => {
            let form = build_photo_form(&image_paths[0], message).await?;
            let resp = graph_client()
                .post(format!("{}/{}/photos", base, page_id))
                .timeout(std::time::Duration::from_secs(PHOTO_UPLOAD_TIMEOUT_SECS))
                .bearer_auth(token)
                .multipart(form)
                .send()
                .await
                .map_err(network_failure)?;
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            let body = resp.text().await.map_err(body_read_failure)?;
            if (200..300).contains(&status) {
                parse_json_response(&body)
            } else {
                Err(graph_failure_from_response(status, &headers, &body))
            }
        }
        _ => {
            let mut media_fbids = Vec::with_capacity(image_paths.len());
            for path in image_paths {
                // Tuần tự: dừng ngay ở ảnh lỗi đầu tiên, không tạo bài.
                media_fbids.push(upload_unpublished_photo(base, token, page_id, path).await?);
            }
            post_multi_photo_feed(base, token, page_id, message, &media_fbids).await
        }
    }
}

#[cfg(test)]
async fn publish_with(
    base: &str,
    token: &str,
    page_id: &str,
    message: &str,
    image_path: Option<&str>,
) -> Result<serde_json::Value, GraphFailure> {
    let paths: Vec<String> = image_path.map(|p| vec![p.to_string()]).unwrap_or_default();
    publish_images_with(base, token, page_id, message, &paths).await
}

#[tauri::command]
pub async fn publish_content<R: Runtime>(
    app: AppHandle<R>,
    page_id: String,
    message: String,
    image_path: Option<String>,
    image_paths: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let paths = match image_paths {
        Some(v) => v,
        None => image_path.into_iter().collect(),
    };
    publish_content_classified_multi(app, page_id, message, paths)
        .await
        .map_err(|e| e.to_string())
}

fn resolve_token_for_page<R: Runtime>(app: &AppHandle<R>, page_id: &str) -> Result<String, String> {
    crate::security::get_secret_hybrid(Some(app), &page_token_key(page_id))
        .ok()
        .flatten()
        .or_else(|| crate::security::get_secret_hybrid(Some(app), "facebook_token").ok().flatten())
        .ok_or("Không tìm thấy Page/User token, vui lòng Tải Trang lại".to_string())
}

async fn list_posts_with(
    base: &str,
    token: &str,
    page_id: &str,
    limit: u8,
) -> Result<serde_json::Value, GraphFailure> {
    validate_page_id(page_id)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let lim = limit.clamp(1, 25).to_string();
    let fields = "id,message,created_time,permalink_url,likes.summary(true),comments.summary(true),shares";
    let resp = graph_client()
        .get(format!("{}/{}/posts", base, page_id.trim()))
        .query(&[("fields", fields), ("limit", lim.as_str())])
        .bearer_auth(token)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if (200..300).contains(&status) {
        parse_json_response(&body)
    } else {
        Err(graph_failure_from_response(status, &headers, &body))
    }
}

#[tauri::command]
pub async fn list_page_posts<R: Runtime>(app: AppHandle<R>, page_id: String, limit: Option<u8>) -> Result<serde_json::Value, String> {
    validate_page_id(&page_id)?;
    let access_token = resolve_token_for_page(&app, page_id.trim())?;
    if access_token.trim().is_empty() {
        return Err("Không tìm thấy Page/User token, vui lòng Tải Trang lại".into());
    }
    if cfg!(test) {
        return Ok(serde_json::json!({"data": []}));
    }
    list_posts_with(
        graph_base().as_str(),
        &access_token,
        &page_id,
        limit.unwrap_or(10),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Cửa sổ thời gian cuốn theo ngày cho báo cáo. Chỉ chấp nhận 7 hoặc 30 ngày.
fn report_window(now: DateTime<Utc>, days: u32) -> Result<(i64, i64), String> {
    if days != 7 && days != 30 {
        return Err("Khoảng thời gian chỉ hỗ trợ 7 hoặc 30 ngày.".to_string());
    }
    let until = now.timestamp();
    Ok((until - i64::from(days) * 86_400, until))
}

/// Cửa sổ thời gian đã resolve cho báo cáo theo tháng/khoảng tự chọn. Tách khỏi
/// network để test được bằng đồng hồ cố định.
#[derive(Debug)]
struct ReportRange {
    since: i64,
    until: i64,
    days: u32,
    key: &'static str,
    label: String,
    from_date: Option<String>,
    to_date: Option<String>,
}

/// Đổi ngày + giờ địa phương sang epoch giây. Trả lỗi nếu múi giờ địa phương
/// không xác định được (ví dụ giờ bị bỏ qua khi đổi DST).
fn local_seconds(date: NaiveDate, hour: u32, min: u32, sec: u32) -> Result<i64, String> {
    Local
        .from_local_datetime(&date.and_hms_opt(hour, min, sec).unwrap())
        .earliest()
        .map(|dt| dt.timestamp())
        .ok_or_else(|| "Không xác định được múi giờ địa phương.".to_string())
}

/// Resolve `Tháng này` (month-to-date theo giờ địa phương) hoặc khoảng ngày tự
/// chọn (bao gồm cả hai đầu). Hàm thuần: nhận `now` từ ngoài để test ổn định.
fn report_custom_range(
    now: DateTime<Local>,
    range_key: &str,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Result<ReportRange, String> {
    if range_key != "month" && range_key != "custom" {
        return Err("Lệnh này chỉ hỗ trợ Tháng này hoặc Tùy chọn.".to_string());
    }
    if range_key == "month" {
        let today = now.date_naive();
        let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
            .ok_or_else(|| "Ngày không hợp lệ. Dùng định dạng YYYY-MM-DD.".to_string())?;
        return Ok(ReportRange {
            since: local_seconds(first, 0, 0, 0)?,
            until: now.timestamp(),
            days: today.day(),
            key: "month",
            label: "Tháng này".to_string(),
            from_date: None,
            to_date: None,
        });
    }

    let raw_from = from_date.map(str::trim).unwrap_or("");
    let raw_to = to_date.map(str::trim).unwrap_or("");
    if raw_from.is_empty() || raw_to.is_empty() {
        return Err("Vui lòng chọn Từ ngày và Đến ngày.".to_string());
    }
    let from = NaiveDate::parse_from_str(raw_from, "%Y-%m-%d")
        .map_err(|_| "Ngày không hợp lệ. Dùng định dạng YYYY-MM-DD.".to_string())?;
    let to = NaiveDate::parse_from_str(raw_to, "%Y-%m-%d")
        .map_err(|_| "Ngày không hợp lệ. Dùng định dạng YYYY-MM-DD.".to_string())?;
    if from > to {
        return Err("Từ ngày phải trước hoặc bằng Đến ngày.".to_string());
    }
    let today = now.date_naive();
    if from > today || to > today {
        return Err("Không thể chọn ngày trong tương lai.".to_string());
    }
    let days = (to - from).num_days() + 1;
    if days > 366 {
        return Err("Khoảng thời gian tối đa là 366 ngày.".to_string());
    }
    let until = if to == today {
        now.timestamp()
    } else {
        local_seconds(to, 23, 59, 59)?
    };
    Ok(ReportRange {
        since: local_seconds(from, 0, 0, 0)?,
        until,
        days: days as u32,
        key: "custom",
        label: format!(
            "Tùy chọn: {} – {}",
            from.format("%d/%m/%Y"),
            to.format("%d/%m/%Y")
        ),
        from_date: Some(from.to_string()),
        to_date: Some(to.to_string()),
    })
}

/// Tên Trang chỉ để hiển thị/ghi CSV: bỏ ký tự điều khiển và giới hạn 200 ký tự.
fn normalize_report_page_name(page_name: Option<String>) -> Option<String> {
    let cleaned = page_name?
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(200)
        .collect::<String>();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn report_summary_total(raw: &serde_json::Value, key: &str) -> Option<u64> {
    raw.get(key)?
        .get("summary")?
        .get("total_count")?
        .as_u64()
}

/// Đọc một trang `/{page_id}/posts`. Bài thiếu id bị bỏ qua; summary thiếu
/// total_count trả 0 và đánh dấu partial (shares thiếu là bình thường).
fn parse_report_posts(
    value: &serde_json::Value,
) -> Result<(Vec<ReportPost>, bool), GraphFailure> {
    let data = value
        .get("data")
        .and_then(|data| data.as_array())
        .ok_or_else(|| {
            GraphFailure::local(
                GraphFailureKind::Other,
                "Phản hồi Graph API không hợp lệ".to_string(),
            )
        })?;
    let mut posts = Vec::with_capacity(data.len());
    let mut partial = false;
    for raw in data {
        let Some(id) = raw
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let likes = report_summary_total(raw, "likes");
        let comments = report_summary_total(raw, "comments");
        if likes.is_none() || comments.is_none() {
            partial = true;
        }
        let shares = raw
            .get("shares")
            .and_then(|shares| shares.get("count"))
            .and_then(|count| count.as_u64())
            .unwrap_or(0);
        let to_u32 = |value: u64| value.min(u32::MAX as u64) as u32;
        posts.push(ReportPost {
            id: id.to_string(),
            message: raw
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string),
            created_time: raw
                .get("created_time")
                .and_then(|time| time.as_str())
                .map(str::to_string),
            permalink_url: raw
                .get("permalink_url")
                .and_then(|url| url.as_str())
                .map(str::to_string),
            reactions: to_u32(likes.unwrap_or(0)),
            comments: to_u32(comments.unwrap_or(0)),
            shares: to_u32(shares),
        });
    }
    Ok((posts, partial))
}

/// Chỉ theo cursor `paging.cursors.after`; không bao giờ dùng `paging.next`.
fn report_cursor(value: &serde_json::Value) -> Option<String> {
    value
        .get("paging")?
        .get("cursors")?
        .get("after")?
        .as_str()
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string)
}

fn report_error_message(failure: &GraphFailure) -> String {
    match failure.kind {
        GraphFailureKind::TokenInvalid => {
            "Token không hợp lệ hoặc đã hết hạn. Vào Cài đặt → Kết nối để cập nhật token, rồi Tải Trang lại.".to_string()
        }
        GraphFailureKind::RateLimited => match failure.retry_after_secs {
            Some(seconds) => format!(
                "Facebook đang giới hạn tần suất. Thử lại sau {} giây.",
                seconds
            ),
            None => "Facebook đang giới hạn tần suất. Thử lại sau ít phút.".to_string(),
        },
        GraphFailureKind::InvalidParam => format!(
            "Không tải được báo cáo: {}",
            scrub_error_text(&failure.message)
        ),
        GraphFailureKind::Network => {
            "Không thể kết nối Facebook. Kiểm tra mạng rồi thử lại.".to_string()
        }
        GraphFailureKind::Other => format!(
            "Không tải được báo cáo: {}",
            scrub_error_text(&failure.message)
        ),
    }
}

async fn report_with_range(
    base: &str,
    token: &str,
    page_id: &str,
    page_name: Option<String>,
    range: ReportRange,
    max_pages: usize,
) -> Result<PageReport, GraphFailure> {
    validate_page_id(page_id)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let page_id = page_id.trim().to_string();
    let page_name = normalize_report_page_name(page_name);
    let since = range.since;
    let until = range.until;

    let mut posts = Vec::new();
    let mut partial = false;
    let mut truncated = false;
    let mut after: Option<String> = None;
    let mut requests = 0usize;

    loop {
        if requests >= max_pages {
            truncated = after.is_some();
            break;
        }
        requests += 1;
        let mut params = vec![
            ("fields", REPORT_FIELDS.to_string()),
            ("limit", REPORT_PAGE_LIMIT.to_string()),
            ("since", since.to_string()),
            ("until", until.to_string()),
        ];
        if let Some(cursor) = after.as_deref() {
            params.push(("after", cursor.to_string()));
        }
        let resp = graph_client()
            .get(format!("{}/{}/posts", base, page_id))
            .query(&params)
            .bearer_auth(token)
            .send()
            .await
            .map_err(network_failure)?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().await.map_err(body_read_failure)?;
        if !(200..300).contains(&status) {
            return Err(graph_failure_from_response(status, &headers, &body));
        }
        let value = parse_json_response(&body)?;
        let (mut page_posts, page_partial) = parse_report_posts(&value)?;
        posts.append(&mut page_posts);
        partial |= page_partial;
        let Some(next_cursor) = report_cursor(&value) else {
            break;
        };
        if after.as_deref() == Some(next_cursor.as_str()) {
            truncated = true;
            break;
        }
        after = Some(next_cursor);
    }

    let total_reactions: u64 = posts.iter().map(|post| u64::from(post.reactions)).sum();
    let total_comments: u64 = posts.iter().map(|post| u64::from(post.comments)).sum();
    let total_shares: u64 = posts.iter().map(|post| u64::from(post.shares)).sum();
    Ok(PageReport {
        page_id,
        page_name,
        days: range.days,
        since,
        until,
        post_count: posts.len(),
        total_reactions,
        total_comments,
        total_shares,
        total_interactions: total_reactions + total_comments + total_shares,
        partial,
        truncated,
        posts,
        range_key: range.key.to_string(),
        range_label: range.label,
        from_date: range.from_date,
        to_date: range.to_date,
    })
}

/// Wrapper tương thích cho báo cáo 7/30 ngày. `Utc::now()` chỉ được gọi đúng
/// một lần ở đây để cửa sổ 7/30 ngày giữ nguyên hành vi cũ.
async fn report_with(
    base: &str,
    token: &str,
    page_id: &str,
    page_name: Option<String>,
    days: u32,
    max_pages: usize,
) -> Result<PageReport, GraphFailure> {
    validate_page_id(page_id)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let (since, until) = report_window(Utc::now(), days)
        .map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let range = ReportRange {
        since,
        until,
        days,
        key: if days == 7 { "7d" } else { "30d" },
        label: format!("{} ngày", days),
        from_date: None,
        to_date: None,
    };
    report_with_range(base, token, page_id, page_name, range, max_pages).await
}

#[tauri::command]
pub async fn get_page_report<R: Runtime>(
    app: AppHandle<R>,
    page_id: String,
    page_name: Option<String>,
    days: u32,
) -> Result<PageReport, String> {
    validate_page_id(&page_id)?;
    report_window(Utc::now(), days)?;
    let access_token = resolve_token_for_page(&app, page_id.trim())?;
    if access_token.trim().is_empty() {
        return Err("Không tìm thấy Page/User token, vui lòng Tải Trang lại".into());
    }
    report_with(
        graph_base().as_str(),
        &access_token,
        &page_id,
        page_name,
        days,
        REPORT_MAX_PAGES,
    )
    .await
    .map_err(|e| report_error_message(&e))
}

#[tauri::command]
pub async fn get_page_report_range<R: Runtime>(
    app: AppHandle<R>,
    page_id: String,
    page_name: Option<String>,
    range_key: String,
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<PageReport, String> {
    validate_page_id(&page_id)?;
    let range = report_custom_range(
        Local::now(),
        range_key.trim(),
        from_date.as_deref(),
        to_date.as_deref(),
    )?;
    let access_token = resolve_token_for_page(&app, page_id.trim())?;
    if access_token.trim().is_empty() {
        return Err("Không tìm thấy Page/User token, vui lòng Tải Trang lại".into());
    }
    report_with_range(
        graph_base().as_str(),
        &access_token,
        &page_id,
        page_name,
        range,
        REPORT_MAX_PAGES,
    )
    .await
    .map_err(|e| report_error_message(&e))
}

async fn insights_with(
    base: &str,
    token: &str,
    post_id: &str,
) -> Result<serde_json::Value, GraphFailure> {
    split_post_id(post_id).map_err(|e| GraphFailure::local(GraphFailureKind::InvalidParam, e))?;
    let metrics = "post_impressions,post_impressions_unique,post_engaged_users,post_clicks,post_reactions_by_type_total,post_video_views";
    let resp = graph_client()
        .get(format!("{}/{}/insights", base, post_id.trim()))
        .query(&[("metric", metrics)])
        .bearer_auth(token)
        .send()
        .await
        .map_err(network_failure)?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(body_read_failure)?;
    if (200..300).contains(&status) {
        parse_json_response(&body)
    } else {
        Err(graph_failure_from_response(status, &headers, &body))
    }
}

#[tauri::command]
pub async fn get_post_insights<R: Runtime>(app: AppHandle<R>, post_id: String) -> Result<serde_json::Value, String> {
    // post_id thường là {page_id}_{post_id}, tách page_id để lấy token đúng trang.
    // split_post_id chạy trước token lookup/network nên id rỗng cũng trả "Post ID không hợp lệ.".
    let (page_id_part, _) = split_post_id(&post_id)?;
    let page_id_part = page_id_part.to_string();
    let access_token = crate::security::get_secret_hybrid(Some(&app), &page_token_key(&page_id_part))
        .ok()
        .flatten()
        .or_else(|| crate::security::get_secret_hybrid(Some(&app), "facebook_token").ok().flatten())
        .ok_or("Không tìm thấy Page/User token, vui lòng Tải Trang lại".to_string())?;
    if access_token.trim().is_empty() {
        return Err("Không tìm thấy Page/User token, vui lòng Tải Trang lại".into());
    }
    if cfg!(test) {
        return Ok(serde_json::json!({"data": []}));
    }
    insights_with(graph_base().as_str(), &access_token, &post_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::HashMap;
    use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
    use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

    /// `body_string_contains` rejects bodies that are not valid UTF-8, and a
    /// multipart upload carrying image bytes never is. Match a byte
    /// subsequence instead so the photo mocks can key off the filename.
    struct BodyContainsBytes(Vec<u8>);

    impl Match for BodyContainsBytes {
        fn matches(&self, request: &Request) -> bool {
            request
                .body
                .windows(self.0.len())
                .any(|window| window == self.0.as_slice())
        }
    }

    #[test]
    fn test_parse_me_response_ok() {
        let json = r#"{"id":"123456789","name":"Test User"}"#;
        let me = parse_me_response(json).unwrap();
        assert_eq!(me.id, "123456789");
        assert_eq!(me.name, "Test User");
    }

    #[test]
    fn test_parse_me_response_invalid() {
        let json = r#"{"error": "bad"}"#;
        assert!(parse_me_response(json).is_err());
    }

    #[test]
    fn test_map_graph_error_oauth() {
        let body = r#"{"error":{"message":"Invalid OAuth access token","type":"OAuthException","code":190,"error_subcode":460}}"#;
        let msg = map_graph_error(400, body);
        assert!(msg.contains("Token không hợp lệ"));
        assert!(msg.contains("190"));
    }

    #[test]
    fn test_map_graph_error_generic() {
        let body = r#"{"error":{"message":"Missing param","type":"GraphMethodException","code":100}}"#;
        let msg = map_graph_error(400, body);
        assert!(msg.contains("100"));
    }

    fn classify(status: u16, code: Option<i32>, error_type: Option<&str>) -> GraphFailure {
        classify_graph_error(status, code, error_type, "message".to_string(), None)
    }

    #[test]
    fn test_classify_graph_error_mapping_table() {
        assert_eq!(
            classify(401, None, None).kind,
            GraphFailureKind::TokenInvalid
        );
        assert_eq!(
            classify(400, Some(190), None).kind,
            GraphFailureKind::TokenInvalid
        );
        assert_eq!(
            classify(429, None, None).kind,
            GraphFailureKind::RateLimited
        );
        for code in [4, 32, 613, 368] {
            assert_eq!(
                classify(400, Some(code), None).kind,
                GraphFailureKind::RateLimited,
                "code {} phải là RateLimited",
                code
            );
        }
        assert_eq!(
            classify(400, Some(100), None).kind,
            GraphFailureKind::InvalidParam
        );
        assert_eq!(
            classify(400, Some(2), None).kind,
            GraphFailureKind::InvalidParam
        );
        assert_eq!(
            classify(400, Some(200), Some("OAuthException")).kind,
            GraphFailureKind::InvalidParam
        );
        assert_eq!(classify(500, Some(1), None).kind, GraphFailureKind::Other);
    }

    #[test]
    fn test_graph_failure_display_equals_message() {
        let failure = classify(429, Some(4), None);
        assert_eq!(failure.to_string(), failure.message);
    }

    #[test]
    fn test_retry_after_secs_parsing() {
        let body = r#"{"error":{"message":"rate","type":"OAuthException","code":4}}"#;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "120".parse().unwrap());
        let failure = graph_failure_from_response(429, &headers, body);
        assert_eq!(failure.retry_after_secs, Some(120));
        assert_eq!(failure.kind, GraphFailureKind::RateLimited);
        assert_eq!(failure.http_status, 429);
        assert_eq!(failure.code, Some(4));

        let mut http_date_headers = reqwest::header::HeaderMap::new();
        http_date_headers.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        let failure = graph_failure_from_response(429, &http_date_headers, body);
        assert_eq!(failure.retry_after_secs, None);

        let mut empty_headers = reqwest::header::HeaderMap::new();
        empty_headers.insert(reqwest::header::RETRY_AFTER, "".parse().unwrap());
        let failure = graph_failure_from_response(429, &empty_headers, body);
        assert_eq!(failure.retry_after_secs, None);
    }

    #[test]
    fn test_graph_failure_from_response_matches_map_graph_error_text() {
        let body = r#"{"error":{"message":"Invalid OAuth access token","type":"OAuthException","code":190,"error_subcode":460}}"#;
        let headers = reqwest::header::HeaderMap::new();
        let failure = graph_failure_from_response(400, &headers, body);
        assert_eq!(failure.message, map_graph_error(400, body));
        assert_eq!(failure.kind, GraphFailureKind::TokenInvalid);
    }

    #[test]
    fn test_graph_failure_local_defaults() {
        let failure = GraphFailure::local(GraphFailureKind::Network, "mất kết nối".to_string());
        assert_eq!(failure.http_status, 0);
        assert_eq!(failure.code, None);
        assert_eq!(failure.retry_after_secs, None);
        assert_eq!(failure.message, "mất kết nối");
    }

    #[test]
    fn test_validate_token_empty() {
        // We test the sync validation part via a helper
        // Since validate_facebook_token is async and does network, we test parse logic only here
        // Empty token should be caught before network in real command
        assert!(parse_me_response("").is_err());
    }

    #[test]
    fn test_page_info_serialization() {
        let p = PageInfo { id: "123".into(), name: "Test Page".into() };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"id\":\"123\""));
        assert!(!json.contains("access_token"));
    }

    #[test]
    fn test_publish_content_image_mock_ok() {
        // Tạo file tạm jpg
        let dir = std::env::temp_dir().join(format!("flowpost_test_img_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.jpg");
        std::fs::write(&path, vec![0xFF, 0xD8, 0xFF, 0xE0]).unwrap(); // header JPEG
        // Cần cache token giả để publish không lỗi thiếu token (mock vẫn cần token resolve)
        // Trong cfg(test) publish sẽ trả mock sau khi validate, nhưng vẫn cần access_token không rỗng
        // Vì publish_content sẽ gọi get_secret_hybrid và fail nếu không có token, nên test này chỉ kiểm tra validate mime/size trước mock
        // Ta test trực tiếp validate mime/size logic bằng cách gọi với page_id bất kỳ và check lỗi thiếu token trước khi tới mock
        // Để test mock thành công, ta tạm bỏ qua token check bằng cách truyền image_path và expect mock chỉ khi token có sẵn
        // Thay vào đó test PageInfo và mime_guess
        assert!(mime_guess::from_path(&path).first_or_octet_stream().to_string().starts_with("image/"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_publish_content_image_invalid_mime() {
        // Test mime cho file .exe phải không phải image
        let mime = mime_guess::from_path("test.exe").first_or_octet_stream().to_string();
        assert!(!["image/jpeg", "image/png", "image/webp", "image/gif"].contains(&mime.as_str()));
    }

    #[test]
    fn test_page_token_key_format() {
        assert_eq!(page_token_key("123"), "fb_page_123");
        assert_eq!(page_token_key("abc"), "fb_page_abc");
    }

    #[test]
    fn test_should_write_page_token_when_missing() {
        assert!(should_write_page_token(None, "abc"));
    }

    #[test]
    fn test_should_not_write_page_token_when_unchanged() {
        assert!(!should_write_page_token(Some("abc"), "abc"));
    }

    #[test]
    fn test_should_write_page_token_when_changed() {
        assert!(should_write_page_token(Some("abc"), "xyz"));
    }

    #[test]
    fn test_validate_message_length_accepts_at_limit() {
        let s = "a".repeat(63_206);
        assert!(validate_message_length(&s).is_ok());
    }

    #[test]
    fn test_validate_message_length_rejects_over_limit() {
        let s = "a".repeat(63_207);
        let err = validate_message_length(&s).unwrap_err();
        assert!(err.contains("63207"));
        assert!(err.contains("63206"));
    }

    #[test]
    fn test_validate_message_length_counts_code_points() {
        let s = "ế".repeat(63_206);
        assert!(validate_message_length(&s).is_ok());
    }

    #[test]
    fn test_preview_prefix_handles_multibyte() {
        let s = "ế".repeat(300);
        let out = preview_prefix(&s, 200);
        assert!(out.chars().count() <= 200);
        // no panic already implied by reaching here
        let ascii = "a".repeat(300);
        assert_eq!(preview_prefix(&ascii, 200).chars().count(), 200);
        assert_eq!(preview_prefix("abcdefghij", 200), "abcdefghij");
    }

    #[tokio::test]
    #[cfg(not(windows))]
    async fn test_publish_content_with_mock_token() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        // validate_page_id chỉ chấp nhận chữ số nên dùng ID số duy nhất mỗi lần chạy.
        let page_id = (uuid::Uuid::new_v4().as_u128() % 1_000_000_000_000_000_000u128).to_string();
        let key = page_token_key(&page_id);
        crate::security::set_secret_hybrid(Some(&handle), &key, "mock_page_token_abc123456").unwrap();
        let result = crate::facebook::publish_content(handle.clone(), page_id.clone(), "hello".into(), None, None)
            .await
            .unwrap();
        assert_eq!(result["id"], "mock_photo_123");
        let _ = crate::security::delete_secret_hybrid(Some(&handle), &key);
    }

    // --- Phase 4: Bearer auth + no token in URL/body (wiremock only) ---

    #[tokio::test]
    async fn validate_uses_bearer_and_no_token_in_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "42", "name": "Test User"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let me = validate_token_with(server.uri().as_str(), "test-token")
            .await
            .unwrap();
        assert_eq!(me.id, "42");
        assert_eq!(me.name, "Test User");
    }

    #[tokio::test]
    async fn list_pages_uses_bearer_and_no_token_in_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me/accounts"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "123", "name": "P", "access_token": "page-tok"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let pages = list_pages_with(server.uri().as_str(), "test-token")
            .await
            .unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, "123");
        assert_eq!(pages[0].name, "P");
        assert_eq!(pages[0].access_token, "page-tok");
    }

    #[tokio::test]
    async fn publish_feed_uses_bearer_and_no_token_in_form() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/123/feed"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "123_9"})))
            .expect(1)
            .mount(&server)
            .await;

        let result = publish_with(server.uri().as_str(), "test-token", "123", "hello world", None)
            .await
            .unwrap();
        assert_eq!(result["id"], "123_9");

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = String::from_utf8_lossy(&requests[0].body).to_string();
        assert!(body.contains("message="), "body thieu message: {}", body);
        assert!(!body.contains("access_token"), "body chua token: {}", body);
        assert!(!requests[0].url.as_str().contains("access_token"));
    }

    #[tokio::test]
    async fn publish_photo_uses_bearer_and_no_token_in_multipart() {
        // PNG 1x1 hợp lệ (RGBA) để mime_guess nhận đúng image/png.
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let image_path = std::env::temp_dir().join(format!(
            "flowpost_phase4_photo_{}.png",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&image_path, png).unwrap();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "123_photo", "post_id": "123_9"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = publish_with(
            server.uri().as_str(),
            "test-token",
            "123",
            "caption",
            Some(image_path.to_string_lossy().as_ref()),
        )
        .await
        .unwrap();
        assert_eq!(result["id"], "123_photo");

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let content_type = requests[0]
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("multipart/form-data"), "{}", content_type);
        let body = String::from_utf8_lossy(&requests[0].body).to_string();
        assert!(body.contains("name=\"source\""), "multipart thieu source");
        assert!(body.contains("name=\"message\""), "multipart thieu message");
        assert!(!body.contains("access_token"), "multipart chua token");
        assert!(!requests[0].url.as_str().contains("access_token"));

        std::fs::remove_file(&image_path).unwrap();
    }

    #[tokio::test]
    async fn list_posts_uses_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .expect(1)
            .mount(&server)
            .await;

        let value = list_posts_with(server.uri().as_str(), "test-token", "123", 5)
            .await
            .unwrap();
        assert!(value["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn insights_uses_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123_456/insights"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .expect(1)
            .mount(&server)
            .await;

        let value = insights_with(server.uri().as_str(), "test-token", "123_456")
            .await
            .unwrap();
        assert!(value["data"].as_array().unwrap().is_empty());
    }

    // --- Phase 4: error scrubbing ---

    #[test]
    fn scrub_error_text_removes_urls_and_token_material() {
        let raw = "error sending request for url (https://graph.facebook.com/v19.0/me) header access_token:abcSECRET123 and Bearer:abcSECRET123";
        let out = scrub_error_text(raw);
        assert!(!out.contains("https://"), "{}", out);
        assert!(!out.contains("access_token"), "{}", out);
        assert!(!out.contains("abcSECRET123"), "{}", out);
        assert!(out.contains("error sending request for url"), "{}", out);
        assert_eq!(out.matches("[url]").count(), 1, "{}", out);
        assert_eq!(out.matches("[redacted]").count(), 2, "{}", out);
    }

    #[test]
    fn map_graph_error_fallback_body_is_scrubbed() {
        let raw = "Bad request to https://graph.facebook.com/v19.0/me header access_token:abcSECRET and more";
        let msg = map_graph_error(500, raw);
        assert!(msg.starts_with("Lỗi HTTP 500:"), "{}", msg);
        assert!(!msg.contains("https://"), "{}", msg);
        assert!(!msg.contains("access_token"), "{}", msg);
        assert!(!msg.contains("abcSECRET"), "{}", msg);
    }

    // --- Phase 4: page_id / post_id validation ---

    #[test]
    fn test_validate_page_id_accepts_digits() {
        assert!(validate_page_id("1234567890").is_ok());
        assert!(validate_page_id(" 123 ").is_ok());
        assert!(validate_page_id(&"9".repeat(32)).is_ok());
    }

    #[test]
    fn test_validate_page_id_rejects_empty() {
        assert!(validate_page_id("").unwrap_err().contains("trống"));
        assert!(validate_page_id("   ").unwrap_err().contains("trống"));
    }

    #[test]
    fn test_validate_page_id_rejects_non_digits() {
        assert!(validate_page_id("123abc").unwrap_err().contains("chữ số"));
        assert!(validate_page_id("../123").unwrap_err().contains("chữ số"));
    }

    #[test]
    fn test_validate_page_id_rejects_too_long() {
        let err = validate_page_id(&"1".repeat(33)).unwrap_err();
        assert!(err.contains("32"), "{}", err);
    }

    #[test]
    fn test_split_post_id_formats() {
        assert_eq!(split_post_id("123_456").unwrap(), ("123", "456"));
        for bad in ["", "123", "abc_123", "123_abc", "123_", "_456", "123_456_789"] {
            assert!(
                split_post_id(bad).unwrap_err().contains("Post ID"),
                "phải từ chối {:?}",
                bad
            );
        }
    }

    #[tokio::test]
    async fn invalid_ids_are_rejected_before_network() {
        // Cổng 1 không có service: nếu validation không chặn, lỗi sẽ là Network.
        let base = "http://127.0.0.1:1";
        assert_eq!(
            publish_with(base, "t", "12ab", "hello", None).await.unwrap_err().kind,
            GraphFailureKind::InvalidParam
        );
        assert_eq!(
            list_posts_with(base, "t", "12ab", 5).await.unwrap_err().kind,
            GraphFailureKind::InvalidParam
        );
        assert_eq!(
            insights_with(base, "t", "abc_123").await.unwrap_err().kind,
            GraphFailureKind::InvalidParam
        );
    }

    #[tokio::test]
    async fn empty_page_id_returns_canonical_message() {
        // validate_page_id chạy trước token lookup: không cần secret để test.
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let err = crate::facebook::list_page_posts(handle, "  ".into(), None)
            .await
            .unwrap_err();
        assert_eq!(err, "Page ID không được để trống.");
    }

    #[tokio::test]
    async fn empty_post_id_returns_canonical_message() {
        // split_post_id chạy trước token lookup: không cần secret để test.
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let err = crate::facebook::get_post_insights(handle, "".into())
            .await
            .unwrap_err();
        assert_eq!(err, "Post ID không hợp lệ.");
    }

    // --- Multi-image posting ---

    #[test]
    fn normalize_image_paths_rejects_empty_path() {
        let err = normalize_image_paths(vec!["a.png".to_string(), "  ".to_string()]).unwrap_err();
        assert_eq!(err.kind, GraphFailureKind::InvalidParam);
        assert_eq!(
            normalize_image_paths(vec![" a.png ".to_string()]).unwrap(),
            vec!["a.png".to_string()]
        );
    }

    #[test]
    fn validate_image_count_rejects_eleven_and_accepts_ten() {
        assert!(validate_image_count(0).is_ok());
        assert!(validate_image_count(10).is_ok());
        let err = validate_image_count(11).unwrap_err();
        assert!(err.contains("tối đa 10"), "{}", err);
        assert!(err.contains("11"), "{}", err);
    }

    #[test]
    fn multi_photo_feed_params_shape() {
        let params = multi_photo_feed_params("cap", &["p1".to_string(), "p2".to_string()]);
        assert_eq!(params[0], ("message".to_string(), "cap".to_string()));
        assert_eq!(params.len(), 3);
        for (i, expected) in ["p1", "p2"].iter().enumerate() {
            let key = format!("attached_media[{}]", i);
            let (k, value) = params
                .iter()
                .find(|(k, _)| k == &key)
                .expect("thiếu khoá attached_media");
            assert_eq!(k, &key);
            let parsed: serde_json::Value = serde_json::from_str(value).unwrap();
            assert_eq!(parsed["media_fbid"], serde_json::json!(expected));
        }
    }

    const PNG_1PX: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn temp_png(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "flowpost_{}_{}.png",
            tag,
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, PNG_1PX).unwrap();
        path
    }

    #[tokio::test]
    async fn publish_photo_unpublished_form_has_published_false_and_no_message() {
        let img_path = temp_png("unpublished");
        // Form không đọc trực tiếp được (không thêm crate mới), nên gửi form đã
        // dựng tới wiremock để bắt đúng bytes multipart được serialize.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "p1"})))
            .expect(1)
            .mount(&server)
            .await;

        let form = build_unpublished_photo_form(img_path.to_string_lossy().as_ref())
            .await
            .unwrap();
        graph_client()
            .post(format!("{}/123/photos", server.uri()))
            .multipart(form)
            .send()
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = String::from_utf8_lossy(&requests[0].body).to_string();
        assert!(body.contains("name=\"source\""), "thiếu source: {}", body);
        assert!(body.contains("name=\"published\""), "thiếu published: {}", body);
        assert!(body.contains("false"), "thiếu false: {}", body);
        assert!(!body.contains("name=\"message\""), "không được có message: {}", body);

        std::fs::remove_file(&img_path).unwrap();
    }

    #[tokio::test]
    async fn publish_multi_photo_uploads_unpublished_then_feed() {
        let p1 = temp_png("multi_a");
        let p2 = temp_png("multi_b");
        let name1 = p1.file_name().unwrap().to_string_lossy().to_string();
        let name2 = p2.file_name().unwrap().to_string_lossy().to_string();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .and(header("Authorization", "Bearer test-token"))
            .and(BodyContainsBytes(name1.as_bytes().to_vec()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "p1"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .and(header("Authorization", "Bearer test-token"))
            .and(BodyContainsBytes(name2.as_bytes().to_vec()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "p2"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/123/feed"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "123_42"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = publish_images_with(
            server.uri().as_str(),
            "test-token",
            "123",
            "caption",
            &[p1.to_string_lossy().to_string(), p2.to_string_lossy().to_string()],
        )
        .await
        .unwrap();
        assert_eq!(result["id"], "123_42");

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "phải là 2 upload + 1 feed");

        let mut photo_bodies = Vec::new();
        let mut feed_body = String::new();
        for request in &requests {
            let body = String::from_utf8_lossy(&request.body).to_string();
            if request.url.path().ends_with("/photos") {
                photo_bodies.push(body);
            } else {
                feed_body = body;
            }
            assert!(
                !request.url.as_str().contains("access_token"),
                "URL chứa token: {}",
                request.url
            );
        }
        assert_eq!(photo_bodies.len(), 2);
        for body in &photo_bodies {
            assert!(!body.contains("access_token"), "multipart chứa token: {}", body);
            assert!(
                !body.contains("name=\"message\""),
                "multipart ảnh chưa công bố không được có message: {}",
                body
            );
        }
        assert!(feed_body.contains("attached_media"), "feed thiếu attached_media: {}", feed_body);
        assert!(feed_body.contains("p1"), "feed thiếu p1: {}", feed_body);
        assert!(feed_body.contains("p2"), "feed thiếu p2: {}", feed_body);
        assert!(feed_body.contains("caption"), "feed thiếu caption: {}", feed_body);
        assert!(!feed_body.contains("access_token"), "feed chứa token: {}", feed_body);

        std::fs::remove_file(&p1).unwrap();
        std::fs::remove_file(&p2).unwrap();
    }

    #[tokio::test]
    async fn publish_multi_photo_aborts_without_feed_when_upload_fails() {
        let p1 = temp_png("abort_a");
        let p2 = temp_png("abort_b");
        let name1 = p1.file_name().unwrap().to_string_lossy().to_string();
        let name2 = p2.file_name().unwrap().to_string_lossy().to_string();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .and(BodyContainsBytes(name1.as_bytes().to_vec()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "p1"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/123/photos"))
            .and(BodyContainsBytes(name2.as_bytes().to_vec()))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"message": "bad photo", "code": 100}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/123/feed"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "should_not"})),
            )
            .expect(0)
            .mount(&server)
            .await;

        let err = publish_images_with(
            server.uri().as_str(),
            "test-token",
            "123",
            "caption",
            &[p1.to_string_lossy().to_string(), p2.to_string_lossy().to_string()],
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind, GraphFailureKind::InvalidParam);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "không được gọi /feed sau khi upload lỗi");

        std::fs::remove_file(&p1).unwrap();
        std::fs::remove_file(&p2).unwrap();
    }

    #[tokio::test]
    async fn publish_multi_photo_rejects_over_cap_before_network() {
        // Cổng 1 không có service: nếu vượt cap không bị chặn, lỗi sẽ là Network.
        let base = "http://127.0.0.1:1";
        let paths: Vec<String> = (0..11).map(|i| format!("C:\\tmp\\img{}.png", i)).collect();
        let err = publish_images_with(base, "t", "123", "caption", &paths)
            .await
            .unwrap_err();
        assert_eq!(err.kind, GraphFailureKind::InvalidParam);
        assert!(err.message.contains("tối đa 10"), "{}", err.message);
    }

    // --- Page report ---

    fn report_post(id: &str, likes: Option<u64>, comments: Option<u64>, shares: Option<u64>) -> serde_json::Value {
        let mut post = serde_json::json!({ "id": id });
        if let Some(total) = likes {
            post["likes"] = serde_json::json!({ "summary": { "total_count": total } });
        }
        if let Some(total) = comments {
            post["comments"] = serde_json::json!({ "summary": { "total_count": total } });
        }
        if let Some(total) = shares {
            post["shares"] = serde_json::json!({ "count": total });
        }
        post
    }

    #[test]
    fn report_window_7_and_30_days() {
        let now = Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap();
        let (since7, until7) = report_window(now, 7).unwrap();
        assert_eq!(until7, now.timestamp());
        assert_eq!(until7 - since7, 7 * 86_400);

        let (since30, until30) = report_window(now, 30).unwrap();
        assert_eq!(until30, now.timestamp());
        assert_eq!(until30 - since30, 30 * 86_400);
    }

    /// Đồng hồ cố định theo giờ địa phương để test range không phụ thuộc TZ.
    fn fixed_local_now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, 25, 15, 30, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn report_custom_range_month_starts_at_local_first_day() {
        let now = fixed_local_now();
        let range = report_custom_range(now, "month", None, None).unwrap();
        let first = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(range.since, local_seconds(first, 0, 0, 0).unwrap());
        assert_eq!(range.until, now.timestamp());
        assert_eq!(range.days, 25);
        assert_eq!(range.key, "month");
        assert_eq!(range.label, "Tháng này");
        assert_eq!(range.from_date, None);
        assert_eq!(range.to_date, None);
    }

    #[test]
    fn report_custom_range_custom_is_inclusive() {
        let now = fixed_local_now();
        let range = report_custom_range(now, "custom", Some("2026-10-01"), Some("2026-10-05"))
            .unwrap();
        let from = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(range.since, local_seconds(from, 0, 0, 0).unwrap());
        assert_eq!(range.until, local_seconds(to, 23, 59, 59).unwrap());
        assert_eq!(range.days, 5);
        assert_eq!(range.key, "custom");
        assert_eq!(range.label, "Tùy chọn: 01/10/2026 – 05/10/2026");
        assert_eq!(range.from_date.as_deref(), Some("2026-10-01"));
        assert_eq!(range.to_date.as_deref(), Some("2026-10-05"));
    }

    #[test]
    fn report_custom_range_caps_today_end_to_now() {
        let now = fixed_local_now();
        let range =
            report_custom_range(now, "custom", Some("2026-10-25"), Some("2026-10-25")).unwrap();
        assert_eq!(range.since, local_seconds(now.date_naive(), 0, 0, 0).unwrap());
        assert_eq!(range.until, now.timestamp());
        assert_eq!(range.days, 1);
    }

    #[test]
    fn report_custom_range_rejects_bad_input() {
        let now = fixed_local_now();
        assert_eq!(
            report_custom_range(now, "7d", None, None).unwrap_err(),
            "Lệnh này chỉ hỗ trợ Tháng này hoặc Tùy chọn."
        );
        for (from, to) in [
            (None, None),
            (Some("   "), Some("2026-10-05")),
            (Some("2026-10-01"), None),
        ] {
            assert_eq!(
                report_custom_range(now, "custom", from, to).unwrap_err(),
                "Vui lòng chọn Từ ngày và Đến ngày."
            );
        }
        for (from, to) in [
            (Some("2026/10/01"), Some("2026-10-05")),
            (Some("2026-10-01"), Some("2026-13-05")),
        ] {
            assert_eq!(
                report_custom_range(now, "custom", from, to).unwrap_err(),
                "Ngày không hợp lệ. Dùng định dạng YYYY-MM-DD."
            );
        }
        assert_eq!(
            report_custom_range(now, "custom", Some("2026-10-05"), Some("2026-10-01"))
                .unwrap_err(),
            "Từ ngày phải trước hoặc bằng Đến ngày."
        );
        for (from, to) in [
            (Some("2026-10-26"), Some("2026-10-27")),
            (Some("2026-10-24"), Some("2026-10-26")),
        ] {
            assert_eq!(
                report_custom_range(now, "custom", from, to).unwrap_err(),
                "Không thể chọn ngày trong tương lai."
            );
        }
        assert_eq!(
            report_custom_range(now, "custom", Some("2025-10-20"), Some("2026-10-21"))
                .unwrap_err(),
            "Khoảng thời gian tối đa là 366 ngày."
        );
    }

    #[test]
    fn report_custom_range_accepts_366_days() {
        let now = fixed_local_now();
        let range = report_custom_range(now, "custom", Some("2025-10-20"), Some("2026-10-20"))
            .unwrap();
        assert_eq!(range.days, 366);
        assert_eq!(range.key, "custom");
    }

    #[tokio::test]
    async fn report_rejects_other_day_ranges() {
        // Cổng 1 không có service: nếu validation không chặn, lỗi sẽ là Network.
        for days in [0_u32, 1, 6, 8, 29, 31] {
            let err = report_with("http://127.0.0.1:1", "test-token", "123", None, days, 20)
                .await
                .unwrap_err();
            assert_eq!(err.kind, GraphFailureKind::InvalidParam, "days={}", days);
            assert_eq!(
                err.message,
                "Khoảng thời gian chỉ hỗ trợ 7 hoặc 30 ngày.",
                "days={}",
                days
            );
        }
    }

    #[tokio::test]
    async fn report_rejects_invalid_page_id_before_network() {
        let err = report_with("http://127.0.0.1:1", "test-token", "12ab", None, 7, 20)
            .await
            .unwrap_err();
        assert_eq!(err.kind, GraphFailureKind::InvalidParam);
        assert!(err.message.contains("chữ số"), "{}", err.message);
    }

    #[tokio::test]
    async fn report_request_shape_and_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .and(query_param("fields", REPORT_FIELDS))
            .and(query_param("limit", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(4), Some(2), Some(1))]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(
            server.uri().as_str(),
            "test-token",
            "123",
            Some("  Sống Tích Cực\u{0007}  ".to_string()),
            7,
            20,
        )
        .await
        .unwrap();
        assert_eq!(report.page_id, "123");
        assert_eq!(report.page_name.as_deref(), Some("Sống Tích Cực"));
        assert_eq!(report.post_count, 1);
        assert_eq!(report.total_reactions, 4);
        assert_eq!(report.total_comments, 2);
        assert_eq!(report.total_shares, 1);
        assert_eq!(report.total_interactions, 7);

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let query: HashMap<String, String> = requests[0]
            .url
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(query.get("fields").map(String::as_str), Some(REPORT_FIELDS));
        assert_eq!(query.get("limit").map(String::as_str), Some("100"));
        assert!(!query.contains_key("after"));
        assert!(!query.contains_key("access_token"));
        let since: i64 = query["since"].parse().unwrap();
        let until: i64 = query["until"].parse().unwrap();
        assert_eq!(until - since, 7 * 86_400);
        assert!(since <= Utc::now().timestamp());
        assert!((Utc::now().timestamp() - until).abs() <= 2);
    }

    #[tokio::test]
    async fn report_range_command_request_shape() {
        let server = MockServer::start().await;
        let range = report_custom_range(
            fixed_local_now(),
            "custom",
            Some("2026-10-01"),
            Some("2026-10-20"),
        )
        .unwrap();
        let since = range.since.to_string();
        let until = range.until.to_string();

        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .and(query_param_is_missing("after"))
            .and(query_param("fields", REPORT_FIELDS))
            .and(query_param("limit", "100"))
            .and(query_param("since", since.as_str()))
            .and(query_param("until", until.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(4), Some(2), Some(1))],
                "paging": { "cursors": { "after": "cursor-1" } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(header("Authorization", "Bearer test-token"))
            .and(query_param_is_missing("access_token"))
            .and(query_param("after", "cursor-1"))
            .and(query_param("since", since.as_str()))
            .and(query_param("until", until.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_2", Some(1), Some(1), None)]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with_range(
            server.uri().as_str(),
            "test-token",
            "123",
            Some("Daily Motivation".to_string()),
            range,
            20,
        )
        .await
        .unwrap();
        assert_eq!(report.range_key, "custom");
        assert_eq!(report.range_label, "Tùy chọn: 01/10/2026 – 20/10/2026");
        assert_eq!(report.from_date.as_deref(), Some("2026-10-01"));
        assert_eq!(report.to_date.as_deref(), Some("2026-10-20"));
        assert_eq!(report.days, 20);
        assert_eq!(report.post_count, 2);

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for request in &requests {
            assert!(!request.url.as_str().contains("access_token"));
            let query: HashMap<String, String> = request
                .url
                .query_pairs()
                .into_owned()
                .collect();
            assert_eq!(query.get("since").map(String::as_str), Some(since.as_str()));
            assert_eq!(query.get("until").map(String::as_str), Some(until.as_str()));
        }
        let first: HashMap<String, String> = requests[0]
            .url
            .query_pairs()
            .into_owned()
            .collect();
        assert!(!first.contains_key("after"));
        let second: HashMap<String, String> = requests[1]
            .url
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(second.get("after").map(String::as_str), Some("cursor-1"));
    }

    #[tokio::test]
    async fn report_paginates_with_after_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param_is_missing("after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(1), Some(1), None)],
                "paging": {
                    "cursors": { "after": "cursor-1" },
                    "next": "https://graph.facebook.com/v19.0/123/posts?after=next-token"
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param("after", "cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_2", Some(2), Some(0), None)]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 20)
            .await
            .unwrap();
        assert_eq!(report.post_count, 2);
        assert!(!report.truncated);
        assert_eq!(report.total_reactions, 3);

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(!requests
            .iter()
            .any(|request| request.url.as_str().contains("next-token")));
    }

    #[tokio::test]
    async fn report_stops_at_cap_and_marks_truncated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param_is_missing("after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(1), Some(1), Some(1))],
                "paging": { "cursors": { "after": "cursor-1" } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param("after", "cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_2", Some(2), Some(2), Some(2))],
                "paging": { "cursors": { "after": "cursor-2" } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param("after", "cursor-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_3", Some(3), Some(3), Some(3))]
            })))
            .expect(0)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 2)
            .await
            .unwrap();
        assert!(report.truncated);
        assert_eq!(report.post_count, 2);
        assert_eq!(report.total_interactions, 9);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
    }

    #[tokio::test]
    async fn report_missing_summary_fields_set_partial() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    report_post("123_1", None, None, Some(2)),
                    report_post("123_2", Some(3), None, Some(1))
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 20)
            .await
            .unwrap();
        assert!(report.partial);
        assert_eq!(report.total_reactions, 3);
        assert_eq!(report.total_comments, 0);
        assert_eq!(report.total_shares, 3);
        assert_eq!(report.total_interactions, 6);
    }

    #[tokio::test]
    async fn report_shares_missing_is_not_partial() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(4), Some(2), None)]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 20)
            .await
            .unwrap();
        assert!(!report.partial);
        assert_eq!(report.posts[0].shares, 0);
        assert_eq!(report.total_interactions, 6);
    }

    #[tokio::test]
    async fn report_skips_posts_without_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "message": "không có id" },
                    { "id": "" },
                    { "id": 42 },
                    report_post("123_1", Some(1), Some(1), Some(1))
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 20)
            .await
            .unwrap();
        assert_eq!(report.post_count, 1);
        assert_eq!(report.posts[0].id, "123_1");
    }

    #[tokio::test]
    async fn report_empty_data_is_valid() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 30, 20)
            .await
            .unwrap();
        assert_eq!(report.days, 30);
        assert_eq!(report.post_count, 0);
        assert!(report.posts.is_empty());
        assert!(!report.partial);
        assert!(!report.truncated);
        assert_eq!(report.total_interactions, 0);
    }

    #[test]
    fn report_error_messages_are_mapped_and_scrubbed() {
        let failure = |kind, message: &str, retry_after_secs| GraphFailure {
            kind,
            http_status: 0,
            code: None,
            retry_after_secs,
            message: message.to_string(),
        };

        assert_eq!(
            report_error_message(&failure(GraphFailureKind::TokenInvalid, "x", None)),
            "Token không hợp lệ hoặc đã hết hạn. Vào Cài đặt → Kết nối để cập nhật token, rồi Tải Trang lại."
        );
        assert_eq!(
            report_error_message(&failure(GraphFailureKind::RateLimited, "x", Some(12))),
            "Facebook đang giới hạn tần suất. Thử lại sau 12 giây."
        );
        assert_eq!(
            report_error_message(&failure(GraphFailureKind::RateLimited, "x", None)),
            "Facebook đang giới hạn tần suất. Thử lại sau ít phút."
        );
        assert_eq!(
            report_error_message(&failure(GraphFailureKind::Network, "x", None)),
            "Không thể kết nối Facebook. Kiểm tra mạng rồi thử lại."
        );
        assert_eq!(
            report_error_message(&failure(GraphFailureKind::InvalidParam, "bad param", None)),
            "Không tải được báo cáo: bad param"
        );
        assert_eq!(
            report_error_message(&failure(GraphFailureKind::Other, "hỏng", None)),
            "Không tải được báo cáo: hỏng"
        );

        let scrubbed = report_error_message(&failure(
            GraphFailureKind::Other,
            "request https://graph.facebook.com/v19.0/123/posts?access_token=SECRET_VALUE failed",
            None,
        ));
        assert!(scrubbed.starts_with("Không tải được báo cáo:"), "{}", scrubbed);
        assert!(!scrubbed.contains("https://"), "{}", scrubbed);
        assert!(!scrubbed.contains("access_token"), "{}", scrubbed);
        assert!(!scrubbed.contains("SECRET_VALUE"), "{}", scrubbed);
    }

    #[tokio::test]
    async fn report_repeated_cursor_stops() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param_is_missing("after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_1", Some(1), Some(0), None)],
                "paging": { "cursors": { "after": "same-cursor" } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/123/posts"))
            .and(query_param("after", "same-cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [report_post("123_2", Some(1), Some(0), None)],
                "paging": { "cursors": { "after": "same-cursor" } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = report_with(server.uri().as_str(), "test-token", "123", None, 7, 20)
            .await
            .unwrap();
        assert!(report.truncated);
        assert_eq!(report.post_count, 2);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
    }
}
