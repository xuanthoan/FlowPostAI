use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use std::time::Duration;

const AI_TIMEOUT_SECS: u64 = 120;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePayload {
    pub prompt: String,
    pub style: String,
    #[serde(default)]
    pub custom_style: Option<String>,
    #[serde(default)]
    pub length: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub title: String,
    pub body: String,
    pub prompt: String,
    pub style: String,
    pub is_mock: bool,
}

fn build_system_message(style: &str, custom_style: Option<&str>) -> String {
    let style_label = if style == "Custom" {
        custom_style.unwrap_or("Custom")
    } else {
        style
    };
    format!(
        "Bạn là chuyên gia viết nội dung Facebook với style: {}\n\nQUY TẮC ĐỊNH DẠNG BẮT BUỘC (tuân thủ tuyệt đối):\n1. Chỉ viết văn bản thuần (plain text). KHÔNG dùng markdown.\n2. KHÔNG dùng các ký tự: #, ##, ###, **, *, _, `, >, hoặc gạch đầu dòng markdown.\n3. Dòng đầu tiên là TIÊU ĐỀ, viết IN HOA toàn bộ.\n4. Sau tiêu đề, để trống 2 dòng rồi mới viết phần thân bài.\n5. Không viết tiêu đề và thân bài trên cùng một dòng.\n6. NGÔN NGỮ BẮT BUỘC: Luôn trả lời bằng ĐÚNG ngôn ngữ mà người dùng đã dùng trong câu yêu cầu. Người dùng viết tiếng Việt thì trả về tiếng Việt; người dùng viết tiếng Anh thì trả về tiếng Anh. Không tự dịch hoặc đổi sang ngôn ngữ khác.",
        style_label
    )
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

fn deserialize_null_default<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Default)]
struct MessageResponse {
    #[serde(default, deserialize_with = "deserialize_null_default")]
    content: String,
}

#[derive(Deserialize, Default)]
struct Choice {
    #[serde(default)]
    message: MessageResponse,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

struct RawCompletion {
    content: String,
    finish_reason: Option<String>,
}

// --- openai-responses protocol ---
#[derive(Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    input: &'a str,
    instructions: &'a str,
    max_output_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct ResponsesResponse {
    output: Vec<ResponsesOutput>,
}

#[derive(Deserialize)]
struct ResponsesOutput {
    #[serde(default)]
    content: Vec<ResponsesContent>,
}

#[derive(Deserialize)]
struct ResponsesContent {
    #[serde(rename = "type")]
    ctype: String,
    #[serde(default)]
    text: String,
}

// --- anthropic-messages protocol ---
#[derive(Serialize)]
struct AnthropicMessagesRequest<'a> {
    model: &'a str,
    system: &'a str,
    messages: Vec<Message<'a>>,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicMessagesResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    ctype: String,
    text: String,
}

fn strip_markdown(input: &str) -> String {
    let global = input.replace("**", "").replace("__", "").replace('`', "");
    let mut out: Vec<String> = Vec::new();
    for raw in global.lines() {
        let mut line = raw.trim_start();
        // blockquote
        while let Some(rest) = line.strip_prefix('>') {
            line = rest.trim_start();
        }
        // leading heading hashes
        line = line.trim_start_matches('#');
        let mut trimmed = line.trim_start();
        // bullet markers
        if let Some(rest) = trimmed.strip_prefix("- ") {
            trimmed = rest;
        } else if let Some(rest) = trimmed.strip_prefix("+ ") {
            trimmed = rest;
        }
        // numbered marker: digits then '.' or ')' then space
        let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let after = &trimmed[digits.len()..];
            if let Some(rest) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
                trimmed = rest;
            }
        }
        // remove remaining standalone * or _ emphasis chars
        let cleaned = trimmed.replace('*', "").replace('_', "");
        out.push(cleaned.trim().to_string());
    }
    out.join("\n")
}

/// Reverse lookup: map a Unicode char that could be produced by decoding a
/// Windows-1252/ISO-8859-1 byte back to that byte.
fn cp1252_char_to_byte(c: char) -> Option<u8> {
    match c {
        '\u{0000}'..='\u{007F}' => Some(c as u8),
        '\u{0080}'..='\u{009F}' => Some(c as u8),
        '\u{00A0}'..='\u{00FF}' => Some(c as u8),
        '€' => Some(0x80), '‚' => Some(0x82), 'ƒ' => Some(0x83), '„' => Some(0x84),
        '…' => Some(0x85), '†' => Some(0x86), '‡' => Some(0x87), 'ˆ' => Some(0x88),
        '‰' => Some(0x89), 'Š' => Some(0x8A), '‹' => Some(0x8B), 'Œ' => Some(0x8C),
        'Ž' => Some(0x8E), '‘' => Some(0x91), '’' => Some(0x92), '“' => Some(0x93),
        '”' => Some(0x94), '•' => Some(0x95), '–' => Some(0x96), '—' => Some(0x97),
        '˜' => Some(0x98), '™' => Some(0x99), 'š' => Some(0x9A), '›' => Some(0x9B),
        'œ' => Some(0x9C), 'ž' => Some(0x9E), 'Ÿ' => Some(0x9F),
        _ => None,
    }
}

/// Windows-1252 byte -> char. Inverse of cp1252_char_to_byte.
fn cp1252_byte_to_char(b: u8) -> char {
    match b {
        0x80 => '€', 0x82 => '‚', 0x83 => 'ƒ', 0x84 => '„', 0x85 => '…',
        0x86 => '†', 0x87 => '‡', 0x88 => 'ˆ', 0x89 => '‰', 0x8A => 'Š',
        0x8B => '‹', 0x8C => 'Œ', 0x8E => 'Ž', 0x91 => '‘', 0x92 => '’',
        0x93 => '“', 0x94 => '”', 0x95 => '•', 0x96 => '–', 0x97 => '—',
        0x98 => '˜', 0x99 => '™', 0x9A => 'š', 0x9B => '›', 0x9C => 'œ',
        0x9E => 'ž', 0x9F => 'Ÿ',
        0x81 | 0x8D | 0x8F | 0x90 | 0x9D => b as char,
        _ => b as char,
    }
}

/// Decode HTTP response bytes WITHOUT destroying information and WITHOUT inserting U+FFFD.
/// Prefer strict UTF-8; if invalid, decode as Windows-1252 (byte->char).
fn decode_response_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252_byte_to_char(b)).collect(),
    }
}

const MOJIBAKE_MAX_PASSES: usize = 2;

/// Observable marker proving which repair revision is running.
pub const MOJIBAKE_REPAIR_VERSION: &str = "v3";

/// One repair pass: decode each maximal run of CP1252-mappable chars independently.
fn repair_once(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run: Vec<u8> = Vec::new();
    let mut run_start: usize = 0;
    for (idx, ch) in s.char_indices() {
        match cp1252_char_to_byte(ch) {
            Some(b) => {
                if run.is_empty() {
                    run_start = idx;
                }
                run.push(b);
            }
            None => {
                flush_run(&mut out, s, run_start, idx, &mut run);
                out.push(ch);
            }
        }
    }
    flush_run(&mut out, s, run_start, s.len(), &mut run);
    out
}

fn flush_run(out: &mut String, original: &str, start: usize, end: usize, run: &mut Vec<u8>) {
    if run.is_empty() {
        return;
    }
    let original_run = &original[start..end];
    match std::str::from_utf8(run) {
        Ok(decoded) => {
            if decoded != original_run {
                out.push_str(decoded);
            } else {
                out.push_str(original_run);
            }
        }
        Err(err) => {
            let valid_up_to = err.valid_up_to();
            // `valid_up_to` guarantees the prefix is valid UTF-8.
            let decoded_prefix = std::str::from_utf8(&run[..valid_up_to])
                .expect("valid_up_to guarantees a valid UTF-8 prefix");
            // Only accept partial salvage when it produced real multi-byte content
            // (a char above Latin-1); otherwise keep the run untouched so genuine
            // Latin-1 text (e.g. "café – test") is never altered.
            let improved = decoded_prefix.chars().any(|c| c as u32 > 0x00FF);
            if valid_up_to == 0 || !improved {
                out.push_str(original_run);
            } else {
                out.push_str(decoded_prefix);
                // Each mappable char maps to exactly one byte, so `valid_up_to`
                // (a byte count) is also the number of original run chars consumed.
                let leftover: String = original_run.chars().skip(valid_up_to).collect();
                if err.error_len().is_some() {
                    // Malformed mid-stream: preserve remaining original chars verbatim.
                    out.push_str(&leftover);
                }
                // else: trailing incomplete sequence (unrecoverable partial char) -> drop it.
            }
        }
    }
    run.clear();
}

/// Loss-tolerant, segment-wise mojibake recovery; iterates up to MOJIBAKE_MAX_PASSES; idempotent.
pub(crate) fn repair_mojibake(s: &str) -> String {
    let mut cur = s.to_string();
    for _ in 0..MOJIBAKE_MAX_PASSES {
        let next = repair_once(&cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}

fn parse_ai_response(content: &str) -> (String, String) {
    let repaired = repair_mojibake(content);
    let content: &str = &repaired;
    // Try to split by \n---\n delimiter for title/body
    let (raw_title, raw_body) = if let Some(idx) = content.find("\n---\n") {
        (content[..idx].to_string(), content[idx + 5..].to_string())
    } else if let Some(idx) = content.find("\n\n\n") {
        // System prompt asks for 2 blank lines between title and body.
        (content[..idx].to_string(), content[idx + 3..].to_string())
    } else {
        // Fallback: first line is title, rest is body
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return ("NỘI DUNG".to_string(), String::new());
        }
        (lines[0].to_string(), lines[1..].join("\n"))
    };
    let title = {
        let t = strip_markdown(&raw_title).trim().to_uppercase();
        if t.is_empty() { "NỘI DUNG".to_string() } else { t }
    };
    let body = strip_markdown(&raw_body).trim().to_string();
    (title, body)
}

fn looks_like_mojibake(s: &str) -> bool {
    s.contains('\u{FFFD}') || s.contains('Ã') || s.contains("á»") || s.contains("Ä‘")
}

fn log_mojibake_preview(s: &str) {
    let hex: Vec<String> = s.as_bytes().iter().take(64).map(|b| format!("{:02X}", b)).collect();
    eprintln!(
        "[FlowPost AI] residual mojibake preview (cap 64 bytes): {}",
        hex.join(" ")
    );
}

const AI_MAX_TOKENS_CAP: u32 = 8192;

pub(crate) const EMPTY_BODY_ERROR: &str =
    "AI không trả về nội dung (có thể bị cắt do giới hạn token). Vui lòng thử lại, chọn độ dài khác hoặc đổi model/provider.";

fn ensure_body_present(body: &str) -> Result<(), String> {
    if body.trim().is_empty() {
        Err(EMPTY_BODY_ERROR.into())
    } else {
        Ok(())
    }
}

fn max_tokens_for_length(length: Option<&str>) -> u32 {
    match length {
        Some("Ngắn") => 1024,
        Some("Dài") => 4096,
        _ => 2048,
    }
}

fn build_chat_request<'a>(
    model: &'a str,
    system_msg: &'a str,
    prompt: &'a str,
    max_tokens: u32,
    is_gemini: bool,
) -> ChatCompletionRequest<'a> {
    ChatCompletionRequest {
        model,
        messages: vec![
            Message { role: "system", content: system_msg },
            Message { role: "user", content: prompt },
        ],
        max_tokens,
        temperature: 0.7,
        reasoning_effort: if is_gemini { Some("low") } else { None },
    }
}

/// Detect Google Gemini hosts so we can lower the reasoning budget.
/// Gemini 2.5/3.x are thinking models: on the OpenAI-compat endpoint the
/// reasoning tokens are charged against `max_tokens`, truncating the visible
/// output unless `reasoning_effort` is lowered.
fn is_gemini_base_url(base_url: &str) -> bool {
    match reqwest::Url::parse(base_url) {
        Ok(url) => match url.host_str() {
            Some(host) => {
                let host = host.to_lowercase();
                host == "generativelanguage.googleapis.com"
                    || host.ends_with(".generativelanguage.googleapis.com")
            }
            None => false,
        },
        Err(_) => false,
    }
}

fn decide_retry(
    finish_reason: Option<&str>,
    content: &str,
    current_max_tokens: u32,
    retries_done: u32,
) -> Option<u32> {
    if retries_done >= 1 {
        return None;
    }
    let bad = finish_reason == Some("length") || content.trim().is_empty();
    if !bad {
        return None;
    }
    let next = current_max_tokens.saturating_mul(2).min(AI_MAX_TOKENS_CAP);
    if next > current_max_tokens {
        Some(next)
    } else {
        None
    }
}

async fn send_chat_once(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    body: &str,
) -> Result<RawCompletion, String> {
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json; charset=utf-8")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| {
            eprintln!("[FlowPost AI] AI request failed: url={} error={:#}", url, e);
            format!("Không thể kết nối AI provider: {} (url: {})", e, url)
        })?;

    let status = resp.status().as_u16();
    let response_bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let response_body = decode_response_bytes(&response_bytes);

    if status >= 200 && status < 300 {
        if response_body.trim_start().starts_with("data:") || response_body.contains("\ndata:") {
            parse_sse_response(&response_body)
        } else {
            parse_json_completion(&response_body)
        }
    } else {
        let error_msg = match status {
            401 | 403 => "API key không hợp lệ hoặc hết hạn.".to_string(),
            429 => "Quá nhiều yêu cầu, vui lòng thử lại sau.".to_string(),
            500 => "AI server đang gặp sự cố, vui lòng thử lại.".to_string(),
            _ => {
                if let Ok(json) = serde_json::from_str::<Value>(&response_body) {
                    if let Some(msg) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                        format!("Lỗi AI ({}): {}", status, msg)
                    } else {
                        format!("Lỗi AI ({}): {}", status, &response_body.chars().take(300).collect::<String>())
                    }
                } else {
                    format!("Lỗi HTTP {}: {}", status, &response_body.chars().take(300).collect::<String>())
                }
            }
        };
        Err(error_msg)
    }
}

fn parse_json_completion(body: &str) -> Result<RawCompletion, String> {
    let parsed: ChatCompletionResponse = serde_json::from_str(body).map_err(|e| {
        format!(
            "Không parse được phản hồi AI: {} body={}",
            e,
            &body.chars().take(500).collect::<String>()
        )
    })?;
    let choice = parsed
        .choices
        .first()
        .ok_or_else(|| "Phản hồi AI không có choices".to_string())?;
    Ok(RawCompletion {
        content: choice.message.content.clone(),
        finish_reason: choice.finish_reason.clone(),
    })
}

fn parse_sse_response(body: &str) -> Result<RawCompletion, String> {
    // SSE streaming: collect delta.content (reusing the historical loop logic).
    let mut combined = String::new();
    let mut finish_reason: Option<String> = None;
    let mut saw_data = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") { continue; }
        saw_data = true;
        let json_str = trimmed.trim_start_matches("data:").trim();
        if json_str == "[DONE]" || json_str.is_empty() { continue; }
        if let Ok(val) = serde_json::from_str::<Value>(json_str) {
            if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                for choice in choices {
                    if let Some(content) = choice.get("delta").and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                        combined.push_str(content);
                    } else if let Some(content) = choice.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                        combined.push_str(content);
                    }
                    if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                        finish_reason = Some(fr.to_string());
                    }
                }
            } else if let Some(content) = val.get("choices").and_then(|c| c.as_array()).and_then(|arr| arr.first()).and_then(|ch| ch.get("delta")).and_then(|d| d.get("content")).and_then(|c| c.as_str()) { combined.push_str(content); }
        }
    }
    if !saw_data {
        return Err("Phản hồi AI không phải SSE hợp lệ".into());
    }
    Ok(RawCompletion {
        content: combined,
        finish_reason,
    })
}

async fn complete_with_retry(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    model: &str,
    system_msg: &str,
    prompt: &str,
    initial_max_tokens: u32,
    is_gemini: bool,
) -> Result<RawCompletion, String> {
    let mut max_tokens = initial_max_tokens;
    let mut retries_done: u32 = 0;
    loop {
        let req = build_chat_request(model, system_msg, prompt, max_tokens, is_gemini);
        let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        let raw = send_chat_once(client, url, key, &body).await?;
        match decide_retry(raw.finish_reason.as_deref(), &raw.content, max_tokens, retries_done) {
            Some(next) => {
                eprintln!(
                    "[FlowPost AI] completion truncated/empty (finish_reason={:?}, len={}) retrying with max_tokens={}",
                    raw.finish_reason, raw.content.chars().count(), next
                );
                max_tokens = next;
                retries_done += 1;
            }
            None => {
                eprintln!("[FlowPost AI] completion accepted (finish_reason={:?}, len={})", raw.finish_reason, raw.content.chars().count());
                return Ok(raw);
            }
        }
    }
}

async fn call_ai_provider(
    app: &AppHandle<impl Runtime>,
    prompt: &str,
    style: &str,
    custom_style: Option<&str>,
    length: Option<&str>,
) -> Result<RawCompletion, String> {
    let (provider, key) = crate::providers::resolve_active_provider(app)?;
    let max_tokens = max_tokens_for_length(length);
    let model = provider
        .models
        .first()
        .cloned()
        .unwrap_or_else(|| crate::providers::DEFAULT_MODEL.to_string());
    let url = crate::providers::chat_completions_url(&provider.base_url);
    // Suffix normalisation is shared via providers::chat_completions_url to avoid double suffix.
    // Protocol is hardcoded to openai-completions — the server handles all formats at the same endpoint
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(AI_TIMEOUT_SECS))
        .http1_only()
        .build()
        .map_err(|e| format!("Không thể khởi tạo HTTP client: {}", e))?;

    let is_gemini = is_gemini_base_url(&provider.base_url);
    let system_msg = build_system_message(style, custom_style);
    complete_with_retry(&client, &url, &key, &model, &system_msg, prompt, max_tokens, is_gemini).await
}

#[tauri::command]
pub async fn generate_content<R: Runtime>(app: AppHandle<R>, payload: GeneratePayload) -> Result<GenerateResult, String> {
    eprintln!("[FlowPost AI] generate_content: mojibake-repair {}", MOJIBAKE_REPAIR_VERSION);
    if payload.prompt.trim().is_empty() {
        return Err("Vui lòng nhập prompt".into());
    }
    if payload.style.trim().is_empty() {
        return Err("Vui lòng chọn style".into());
    }

    let raw = call_ai_provider(&app, &payload.prompt, &payload.style, payload.custom_style.as_deref(), payload.length.as_deref()).await?;
    let (title, body) = parse_ai_response(&raw.content);
    if looks_like_mojibake(&title) || looks_like_mojibake(&body) {
        log_mojibake_preview(&raw.content);
    }
    ensure_body_present(&body)?;

    Ok(GenerateResult {
        title,
        body,
        prompt: payload.prompt.clone(),
        style: payload.style.clone(),
        is_mock: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};
    use serde_json::json;

    fn sample_content() -> String {
        "Tiêu đề bài viết\n---\nNội dung bài viết ở đây.".to_string()
    }

    fn build_mock_response(content: &str) -> ResponseTemplate {
        let body = json!({
            "choices": [{
                "message": {
                    "content": content
                }
            }]
        });
        ResponseTemplate::new(200)
            .set_body_json(body)
            .insert_header("content-type", "application/json")
    }

    #[tokio::test]
    async fn test_parse_ai_response_with_delimiter() {
        let content = "Title here\n---\nBody here";
        let (title, body) = parse_ai_response(content);
        assert_eq!(title, "TITLE HERE");
        assert_eq!(body, "Body here");
    }

    #[tokio::test]
    async fn test_parse_ai_response_fallback() {
        let content = "First line is title\nSecond line is body";
        let (title, body) = parse_ai_response(content);
        assert_eq!(title, "FIRST LINE IS TITLE");
        assert_eq!(body, "Second line is body");
    }

    #[tokio::test]
    async fn test_parse_ai_response_empty() {
        let content = "";
        let (title, body) = parse_ai_response(content);
        assert_eq!(title, "NỘI DUNG");
        assert_eq!(body, "");
    }

    #[tokio::test]
    async fn test_parse_ai_response_bullets_and_numbered() {
        let (title, body) = parse_ai_response("- Mục 1\n* Mục 2\n1. Mục 3");
        assert_eq!(title, "MỤC 1");
        assert_eq!(body, "Mục 2\nMục 3");
    }

    #[test]
    fn test_strip_markdown_removes_emphasis() {
        assert_eq!(strip_markdown("**a** và `b` và _c_"), "a và b và c");
    }

    #[test]
    fn test_strip_markdown_removes_leading_heading_and_bullets() {
        assert_eq!(
            strip_markdown("### T\n- a\n* b\n+ c\n1. d\n> e"),
            "T\na\nb\nc\nd\ne"
        );
    }

    #[test]
    fn test_parse_ai_response_strips_markdown() {
        let (title, body) = parse_ai_response("### Tiêu đề\n---\n**Đậm** và `code`");
        assert_eq!(title, "TIÊU ĐỀ");
        assert_eq!(body, "Đậm và code");
    }

    #[test]
    fn test_parse_ai_response_uppercase_vietnamese() {
        let (title, _body) = parse_ai_response("điều kỳ diệu\n---\nbody");
        assert_eq!(title, "ĐIỀU KỲ DIỆU");
    }

    #[test]
    fn test_parse_ai_response_uppercase_fallback() {
        let (title, body) = parse_ai_response("First line is title\nSecond");
        assert_eq!(title, "FIRST LINE IS TITLE");
        assert_eq!(body, "Second");
    }

    #[test]
    fn test_build_system_message_contains_format_rules() {
        let msg = build_system_message("Viral", None);
        assert!(msg.contains("chuyên gia viết nội dung Facebook"));
        assert!(msg.contains("KHÔNG dùng markdown"));
        assert!(msg.contains("IN HOA"));
        assert!(msg.contains("2 dòng"));
    }

    #[tokio::test]
    async fn test_call_ai_provider_success() {
        let mock_server = MockServer::start().await;
        let content = sample_content();
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test_key_123"))
            .respond_with(build_mock_response(&content))
            .mount(&mock_server)
            .await;

        // We can't test the full call_ai_provider without an AppHandle,
        // so we test the response parsing logic here via json macro.
        let body = json!({
            "choices": [{
                "message": { "content": content }
            }]
        });
        let parsed: ChatCompletionResponse = serde_json::from_value(body).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].message.content, content);
    }

    #[tokio::test]
    async fn test_call_ai_provider_error_401() {
        let mock_server = MockServer::start().await;
        let error_body = json!({
            "error": {
                "message": "Invalid API key provided",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(error_body.clone()))
            .mount(&mock_server)
            .await;

        // Parse error response to verify format
        let resp_str = serde_json::to_string(&error_body).unwrap();
        let parsed: Value = serde_json::from_str(&resp_str).unwrap();
        assert!(parsed.get("error").is_some());
        let msg = parsed.get("error").unwrap().get("message").unwrap().as_str().unwrap();
        assert_eq!(msg, "Invalid API key provided");
    }

    #[tokio::test]
    async fn test_build_system_message() {
        let msg = build_system_message("Viral", None);
        assert!(msg.contains("Viral"));
        assert!(msg.contains("chuyên gia viết nội dung Facebook"));
    }

    #[tokio::test]
    async fn test_build_system_message_custom() {
        let msg = build_system_message("Custom", Some("Hài hước GenZ"));
        assert!(msg.contains("Hài hước GenZ"));
    }

    #[tokio::test]
    async fn test_chat_completion_request_serialization() {
        let request = ChatCompletionRequest {
            model: crate::providers::DEFAULT_MODEL,
            messages: vec![
                Message { role: "system", content: "test system" },
                Message { role: "user", content: "test user" },
            ],
            max_tokens: 500,
            temperature: 0.7,
            reasoning_effort: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("xoay-vong-worker-web-128k"));
        assert!(json.contains("test user"));
    }

    #[test]
    fn test_max_tokens_for_length() {
        assert_eq!(max_tokens_for_length(None), 2048);
        assert_eq!(max_tokens_for_length(Some("Vừa")), 2048);
        assert_eq!(max_tokens_for_length(Some("Ngắn")), 1024);
        assert_eq!(max_tokens_for_length(Some("Dài")), 4096);
        assert_eq!(max_tokens_for_length(Some("unknown")), 2048);
    }

    #[test]
    fn test_build_chat_request_gemini_includes_reasoning_effort_low() {
        let req = build_chat_request("m", "sys", "user", 2048, true);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"reasoning_effort\":\"low\""), "json={}", json);
    }

    #[test]
    fn test_build_chat_request_non_gemini_omits_reasoning_effort() {
        let req = build_chat_request("m", "sys", "user", 2048, false);
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("reasoning_effort"), "json={}", json);
    }

    #[test]
    fn test_is_gemini_base_url() {
        assert!(is_gemini_base_url("https://generativelanguage.googleapis.com/v1beta/openai"));
        assert!(!is_gemini_base_url("https://api.openai.com/v1"));
        assert!(!is_gemini_base_url("http://127.0.0.1:20128/v1"));
        assert!(!is_gemini_base_url("https://api.anthropic.com/v1"));
        assert!(!is_gemini_base_url("not a url"));
    }

    #[test]
    fn test_decide_retry_length_doubles_once() {
        assert_eq!(decide_retry(Some("length"), "", 2048, 0), Some(4096));
        assert_eq!(decide_retry(Some("length"), "", 2048, 1), None);
        assert_eq!(decide_retry(Some("length"), "x", 8192, 0), None);
    }

    #[test]
    fn test_decide_retry_clean_completion_is_accepted() {
        assert_eq!(decide_retry(Some("stop"), "nội dung", 2048, 0), None);
    }

    #[test]
    fn test_decide_retry_empty_content_retries() {
        assert_eq!(decide_retry(None, "   ", 1024, 0), Some(2048));
    }

    #[test]
    fn test_parse_ai_response_blank_line_separator() {
        let (title, body) = parse_ai_response("TIÊU ĐỀ\n\n\nThân bài ở đây");
        assert_eq!(title, "TIÊU ĐỀ");
        assert_eq!(body, "Thân bài ở đây");
        assert!(!body.is_empty());
    }

    #[test]
    fn test_parse_ai_response_single_line_still_empty_body() {
        let (title, body) = parse_ai_response("Chỉ một dòng");
        assert_eq!(title, "CHỈ MỘT DÒNG");
        assert_eq!(body, "");
    }

    #[test]
    fn test_parse_json_completion_null_content_is_empty() {
        let raw = parse_json_completion(
            r#"{"choices":[{"message":{"content":null},"finish_reason":"length"}]}"#,
        )
        .unwrap();
        assert_eq!(raw.content, "");
        assert_eq!(raw.finish_reason.as_deref(), Some("length"));

        let raw2 = parse_json_completion(r#"{"choices":[{},{"message":{}}]}"#).unwrap();
        assert_eq!(raw2.content, "");
        assert_eq!(raw2.finish_reason, None);
    }

    #[test]
    fn test_parse_json_completion_reads_finish_reason() {
        let raw = parse_json_completion(
            r#"{"choices":[{"message":{"content":"hello"},"finish_reason":"stop"}]}"#,
        )
        .unwrap();
        assert_eq!(raw.content, "hello");
        assert_eq!(raw.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_parse_sse_response_detects_finish_reason_length() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"SÁT THỦ\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n\
                    data: [DONE]\n\n";
        let raw = parse_sse_response(body).unwrap();
        assert_eq!(raw.content, "SÁT THỦ");
        assert_eq!(raw.finish_reason.as_deref(), Some("length"));
    }

    #[test]
    fn test_parse_sse_response_accumulates_content() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Thân \"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"bài\"}}]}\n\n\
                    data: [DONE]\n\n";
        let raw = parse_sse_response(body).unwrap();
        assert_eq!(raw.content, "Thân bài");
        assert_eq!(raw.finish_reason, None);
    }

    #[test]
    fn test_ensure_body_present_rejects_blank() {
        assert_eq!(ensure_body_present(""), Err(EMPTY_BODY_ERROR.to_string()));
        assert_eq!(ensure_body_present("   \n "), Err(EMPTY_BODY_ERROR.to_string()));
        assert!(ensure_body_present("ok").is_ok());
    }

    #[tokio::test]
    async fn test_complete_with_retry_retries_on_length_and_returns_second_response() {
        let mock_server = MockServer::start().await;

        // First response: truncated title, finish_reason "length". Served at most once.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "SÁT THỦ HOÀN HẢO" },
                    "finish_reason": "length"
                }]
            })))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&mock_server)
            .await;

        // Second response: full body, finish_reason "stop".
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "TIÊU ĐỀ\n\n\nThân bài đầy đủ" },
                    "finish_reason": "stop"
                }]
            })))
            .with_priority(2)
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let url = format!("{}/v1/chat/completions", mock_server.uri());
        let raw =
            complete_with_retry(&client, &url, "test_key_123", "m", "sys", "user", 2048, false)
                .await
                .unwrap();
        assert_eq!(raw.content, "TIÊU ĐỀ\n\n\nThân bài đầy đủ");
        assert_eq!(raw.finish_reason.as_deref(), Some("stop"));

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "expected exactly 2 POSTs");
        let first: Value = requests[0].body_json().unwrap();
        let second: Value = requests[1].body_json().unwrap();
        assert_eq!(first["max_tokens"], 2048);
        assert_eq!(second["max_tokens"], 4096);
    }

    #[tokio::test]
    async fn test_complete_with_retry_no_retry_on_stop() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "Tiêu đề\n\n\nThân bài" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let url = format!("{}/v1/chat/completions", mock_server.uri());
        let raw =
            complete_with_retry(&client, &url, "test_key_123", "m", "sys", "user", 2048, false)
                .await
                .unwrap();
        assert_eq!(raw.content, "Tiêu đề\n\n\nThân bài");

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "expected exactly 1 POST");
    }

    #[tokio::test]
    async fn test_complete_with_retry_sends_reasoning_effort_for_gemini() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(build_mock_response("Tiêu đề\n\n\nThân bài"))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let url = format!("{}/v1/chat/completions", mock_server.uri());
        let _ = complete_with_retry(&client, &url, "test_key_123", "m", "sys", "user", 2048, true)
            .await
            .unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: Value = requests[0].body_json().unwrap();
        assert_eq!(body["reasoning_effort"], "low");
    }

    // endpoint_for_protocol removed — base_url is now used directly as the full endpoint URL.

    #[test]
    fn test_responses_request_serialization() {
        let req = ResponsesRequest {
            model: crate::providers::DEFAULT_MODEL,
            input: "prompt test",
            instructions: "Bạn là chuyên gia",
            max_output_tokens: 500,
            temperature: 0.7,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("prompt test"));
        assert!(json.contains("max_output_tokens"));
        assert!(json.contains("instructions"));
    }

    #[test]
    fn test_anthropic_request_serialization() {
        let req = AnthropicMessagesRequest {
            model: crate::providers::DEFAULT_MODEL,
            system: "Bạn là chuyên gia",
            messages: vec![Message { role: "user", content: "hello" }],
            max_tokens: 500,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("system"));
        assert!(json.contains("max_tokens"));
    }

    #[tokio::test]
    async fn test_parse_responses_response() {
        let content = "Tiêu đề\n---\nBody responses";
        let body = json!({
            "output": [{
                "type": "message",
                "content": [{ "type": "output_text", "text": content }]
            }]
        });
        let parsed: ResponsesResponse = serde_json::from_value(body).unwrap();
        let text = &parsed.output[0].content[0].text;
        assert_eq!(text, content);
        let (title, body) = parse_ai_response(text);
        assert_eq!(title, "TIÊU ĐỀ");
        assert_eq!(body, "Body responses");
    }

    #[tokio::test]
    async fn test_parse_anthropic_response() {
        let body = json!({
            "content": [{ "type": "text", "text": "Dòng 1\nDòng 2 nội dung" }]
        });
        let parsed: AnthropicMessagesResponse = serde_json::from_value(body).unwrap();
        assert_eq!(parsed.content.len(), 1);
        assert_eq!(parsed.content[0].ctype, "text");
        let combined: String = parsed.content.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join("\n");
        let (title, body) = parse_ai_response(&combined);
        assert_eq!(title, "DÒNG 1");
        assert!(body.contains("Dòng 2"));
    }

    fn corrupt_to_cp1252(s: &str) -> String {
        s.bytes().map(super::cp1252_byte_to_char).collect()
    }

    #[test]
    fn test_repair_mojibake_known_vietnamese() {
        assert_eq!(repair_mojibake("THIáº¾T Káº¾"), "THIẾT KẾ");
    }

    #[test]
    fn test_repair_mojibake_correct_vietnamese_unchanged() {
        assert_eq!(repair_mojibake("ĐIỀU KỲ DIỆU"), "ĐIỀU KỲ DIỆU");
    }

    #[test]
    fn test_repair_mojibake_ascii_unchanged() {
        assert_eq!(repair_mojibake("Hello World"), "Hello World");
    }

    #[test]
    fn test_repair_mojibake_idempotent() {
        let once = repair_mojibake("THIáº¾T Káº¾");
        assert_eq!(once, "THIẾT KẾ");
        assert_eq!(repair_mojibake(&once), once);
    }

    #[test]
    fn test_repair_mojibake_replacement_char_unchanged() {
        let s = "abc\u{FFFD}def";
        assert_eq!(repair_mojibake(s), s);
    }

    #[test]
    fn test_repair_mojibake_round_trip() {
        let samples = [
            "THIẾT KẾ NG",
            "ĐIỀU KỲ DIỆU",
            "NUTRIFRESH - THIẾT KẾ NG",
            "Thương hiệu đồ ăn thực dưỡng, an toàn, sạch",
        ];
        for s in samples {
            let corrupted = corrupt_to_cp1252(s);
            assert_ne!(corrupted, s, "sample must differ when corrupted: {}", s);
            assert_eq!(repair_mojibake(&corrupted), s, "round-trip failed for {}", s);
        }
    }

    #[test]
    fn test_parse_ai_response_repairs_mojibake() {
        let (title, body) = parse_ai_response("NUTRIFRESH - THIáº¾T Káº¾ NG\n---\nNá»™i dung");
        assert_eq!(title, "NUTRIFRESH - THIẾT KẾ NG");
        assert_eq!(body, "Nội dung");
    }

    #[test]
    fn test_build_system_message_contains_language_rule() {
        let msg = build_system_message("Viral", None);
        assert!(msg.contains("NGÔN NGỮ BẮT BUỘC"));
        assert!(msg.contains("ĐÚNG ngôn ngữ"));
        assert!(msg.contains("tiếng Việt"));
        assert!(msg.contains("tiếng Anh"));
        assert!(msg.contains("chuyên gia viết nội dung Facebook"));
        assert!(msg.contains("KHÔNG dùng markdown"));
    }

    #[test]
    fn test_repair_mojibake_round_trip_vietnamese_samples() {
        for s in ["Bạn đã bao giờ", "CHĂM SÓC TỰ NHIÊN", "ĐIỀU KỲ DIỆU", "Thương hiệu thực dưỡng"] {
            let c = corrupt_to_cp1252(s);
            assert_ne!(c, s, "must differ: {}", s);
            assert_eq!(repair_mojibake(&c), s, "round-trip failed: {}", s);
        }
    }

    #[test]
    fn test_repair_mojibake_segment_recovers_around_replacement_char() {
        let input = format!("{} \u{FFFD} {}", corrupt_to_cp1252("Bạn"), corrupt_to_cp1252("đã"));
        let out = repair_mojibake(&input);
        assert!(out.starts_with("Bạn"), "left run not recovered: {}", out);
        assert!(out.contains('\u{FFFD}'), "U+FFFD must be preserved: {}", out);
        assert!(out.ends_with("đã"), "right run not recovered: {}", out);
    }

    #[test]
    fn test_repair_mojibake_correct_text_never_altered() {
        for s in ["ĐIỀU KỲ DIỆU", "Nội dung", "ÂM NHẠC", "Hello World 123"] {
            assert_eq!(repair_mojibake(s), s);
        }
    }

    #[test]
    fn test_repair_mojibake_idempotent_general() {
        let c = corrupt_to_cp1252("Bạn đã bao giờ");
        let once = repair_mojibake(&c);
        assert_eq!(repair_mojibake(&once), once);
    }

    #[test]
    fn test_repair_mojibake_double_layer() {
        let once = corrupt_to_cp1252("Bạn");
        let twice = corrupt_to_cp1252(&once);
        assert_ne!(twice, once);
        assert_eq!(repair_mojibake(&twice), "Bạn");
    }

    #[test]
    fn test_parse_ai_response_title_and_body_recovered_with_fffd() {
        let content = format!(
            "{}\n---\n{} \u{FFFD} {}",
            corrupt_to_cp1252("NUTRIFRESH - CHĂM SÓC"),
            corrupt_to_cp1252("Bạn đã"),
            corrupt_to_cp1252("bao giờ")
        );
        let (title, body) = parse_ai_response(&content);
        assert_eq!(title, "NUTRIFRESH - CHĂM SÓC");
        assert!(body.contains("Bạn đã"));
        assert!(body.contains("bao giờ"));
        assert!(body.contains('\u{FFFD}'));
    }

    #[test]
    fn test_decode_response_bytes_strict_utf8_unchanged() {
        let b = "Bạn đã".as_bytes();
        let s = decode_response_bytes(b);
        assert_eq!(s, "Bạn đã");
        assert!(!s.contains('\u{FFFD}'));
    }

    #[test]
    fn test_decode_response_bytes_invalid_utf8_no_replacement_char() {
        let s = decode_response_bytes(&[0x41, 0xE9, 0x42]); // invalid UTF-8; CP1252 => "AéB"
        assert_eq!(s, "AéB");
        assert!(!s.contains('\u{FFFD}'));
    }

    #[test]
    fn test_decode_then_repair_pipeline() {
        // Server returns valid UTF-8 bytes of the MOJIBAKE string "Báº¡n".
        let raw = "Báº¡n".as_bytes();
        let decoded = decode_response_bytes(raw);
        assert_eq!(decoded, "Báº¡n");
        assert_eq!(repair_mojibake(&decoded), "Bạn");
    }

    // Ground-truth 64-byte preview captured from the app's own diagnostic log.
    fn real_captured_bytes() -> [u8; 64] {
        [
            0x4E, 0x55, 0x54, 0x52, 0x49, 0x46, 0x52, 0x45, 0x53, 0x48, 0x20, 0x2D, 0x20, 0x54, 0x48, 0xC3,
            0x86, 0xC2, 0xAF, 0xC3, 0x86, 0xC2, 0xA0, 0x4E, 0x47, 0x20, 0x48, 0x49, 0xC3, 0xA1, 0xC2, 0xBB,
            0xE2, 0x80, 0xA0, 0x55, 0x20, 0xC3, 0x84, 0xEF, 0xBF, 0xBD, 0xC3, 0xA1, 0xC2, 0xBB, 0xE2, 0x80,
            0x99, 0x20, 0xC3, 0x84, 0xE2, 0x80, 0x9A, 0x4E, 0x20, 0x54, 0x48, 0xC3, 0xA1, 0xC2, 0xBB, 0xC2,
        ]
    }

    #[test]
    fn test_repair_mojibake_real_captured_bytes_partial_salvage() {
        let raw = real_captured_bytes();
        let input = String::from_utf8_lossy(&raw).to_string();
        let out = repair_mojibake(&input);

        // "Đ" was genuinely lost upstream (byte 0x90 -> U+FFFD), so only "Ồ ĂN" is recoverable here.
        assert!(out.contains("THƯƠNG HIỆU"), "expected 'THƯƠNG HIỆU' in: {}", out);
        assert!(out.contains("Ồ ĂN"), "expected 'Ồ ĂN' in: {}", out);

        for marker in ['Æ', '†', '’', '‚'] {
            assert!(!out.contains(marker), "marker {:?} survived: {}", marker, out);
        }
        assert!(!out.contains("á»"), "mojibake 'á»' survived: {}", out);
        assert!(!out.contains("Ä‚"), "mojibake 'Ä‚' survived: {}", out);
        assert!(!out.contains("Ä‘"), "mojibake 'Ä‘' survived: {}", out);

        assert_eq!(out.matches('\u{FFFD}').count(), input.matches('\u{FFFD}').count());
        assert!(out.matches('\u{FFFD}').count() <= 2, "too many U+FFFD: {}", out);

        assert_eq!(repair_mojibake(&out), out, "repair must be idempotent: {}", out);
    }

    #[test]
    fn test_repair_mojibake_lossless_full_title_round_trip() {
        let expected = "NUTRIFRESH - THƯƠNG HIỆU ĐỒ ĂN THỰC DƯỠNG";
        let corrupted = corrupt_to_cp1252(expected);
        assert_ne!(corrupted, expected);
        assert_eq!(repair_mojibake(&corrupted), expected);
    }

    #[test]
    fn test_repair_mojibake_survives_injected_fffd_mid_string() {
        let full = corrupt_to_cp1252("Bạn đã bao giờ");
        let corrupted_d = corrupt_to_cp1252("đ");
        let damaged = full.replacen(&corrupted_d, "\u{FFFD}", 1);
        assert!(damaged.contains('\u{FFFD}'), "construction must inject U+FFFD");

        let out = repair_mojibake(&damaged);
        assert_eq!(out, "Bạn \u{FFFD}ã bao giờ");
        assert_eq!(out.matches('\u{FFFD}').count(), 1);
    }

    #[test]
    fn test_repair_mojibake_latin1_text_unchanged() {
        for s in ["café – test", "Résumé – déjà vu", "naïve façade"] {
            assert_eq!(repair_mojibake(s), s, "legit Latin-1 text altered: {}", s);
        }
    }

    #[test]
    fn test_repair_mojibake_dangling_lead_byte_not_reemitted() {
        let raw = real_captured_bytes();
        let out = repair_mojibake(&String::from_utf8_lossy(&raw));
        assert!(!out.contains('Ä'), "stray 'Ä' leaked: {}", out);
    }

    #[test]
    fn test_mojibake_repair_version_is_v3() {
        assert_eq!(MOJIBAKE_REPAIR_VERSION, "v3");
    }
}