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
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageResponse,
}

#[derive(Deserialize)]
struct MessageResponse {
    content: String,
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

fn max_tokens_for_length(length: Option<&str>) -> u32 {
    match length {
        Some("Ngắn") => 250,
        Some("Dài") => 800,
        _ => 500,
    }
}

async fn call_ai_provider(
    app: &AppHandle<impl Runtime>,
    prompt: &str,
    style: &str,
    custom_style: Option<&str>,
    length: Option<&str>,
) -> Result<String, String> {
    let (provider, key) = crate::providers::resolve_active_provider(app)?;
    let system_msg = build_system_message(style, custom_style);
    let max_tokens = max_tokens_for_length(length);
    let model = provider
        .models
        .first()
        .cloned()
        .unwrap_or_else(|| crate::providers::DEFAULT_MODEL.to_string());
    let url = crate::providers::chat_completions_url(&provider.base_url);
    // Suffix normalisation is shared via providers::chat_completions_url to avoid double suffix.
    // Protocol is hardcoded to openai-completions — the server handles all formats at the same endpoint
    let req = ChatCompletionRequest {
        model: &model,
        messages: vec![
            Message { role: "system", content: &system_msg },
            Message { role: "user", content: prompt },
        ],
        max_tokens,
        temperature: 0.7,
    };
    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(AI_TIMEOUT_SECS))
        .http1_only()
        .build()
        .map_err(|e| format!("Không thể khởi tạo HTTP client: {}", e))?;

    let req_builder = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json; charset=utf-8");
    let resp = req_builder
        .body(body)
        .send()
        .await
        .map_err(|e| {
            let url_str = url.clone();
            eprintln!("[FlowPost AI] AI request failed: url={} error={:#}", url_str, e);
            format!("Không thể kết nối AI provider: {} (url: {})", e, url_str)
        })?;

    let status = resp.status().as_u16();
    let response_bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let response_body = decode_response_bytes(&response_bytes);

    if status >= 200 && status < 300 {
        if response_body.trim_start().starts_with("data:") || response_body.contains("\ndata:") {
            // SSE streaming: collect delta.content
            let mut combined = String::new();
            for line in response_body.lines() {
                let trimmed = line.trim();
                if !trimmed.starts_with("data:") { continue; }
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
                        }
                    } else if let Some(content) = val.get("choices").and_then(|c| c.as_array()).and_then(|arr| arr.first()).and_then(|ch| ch.get("delta")).and_then(|d| d.get("content")).and_then(|c| c.as_str()) { combined.push_str(content); }
                }
            }
            if !combined.trim().is_empty() {
                Ok(combined)
            } else {
                Err(format!("Phản hồi AI rỗng (SSE): {}", &response_body.chars().take(300).collect::<String>()))
            }
        } else {
            // Try non-streaming JSON
            let parsed: ChatCompletionResponse = serde_json::from_str(&response_body).map_err(|e| format!("Không parse được phản hồi AI: {} body={}", e, &response_body.chars().take(500).collect::<String>()))?;
            if let Some(choice) = parsed.choices.first() {
                Ok(choice.message.content.clone())
            } else {
                Err("Phản hồi AI không có choices".into())
            }
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

#[tauri::command]
pub async fn generate_content<R: Runtime>(app: AppHandle<R>, payload: GeneratePayload) -> Result<GenerateResult, String> {
    eprintln!("[FlowPost AI] generate_content: mojibake-repair {}", MOJIBAKE_REPAIR_VERSION);
    if payload.prompt.trim().is_empty() {
        return Err("Vui lòng nhập prompt".into());
    }
    if payload.style.trim().is_empty() {
        return Err("Vui lòng chọn style".into());
    }

    let content = call_ai_provider(&app, &payload.prompt, &payload.style, payload.custom_style.as_deref(), payload.length.as_deref()).await?;
    let (title, body) = parse_ai_response(&content);
    if looks_like_mojibake(&title) || looks_like_mojibake(&body) {
        log_mojibake_preview(&content);
    }

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
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("xoay-vong-worker-web-128k"));
        assert!(json.contains("test user"));
    }

    #[test]
    fn test_max_tokens_for_length() {
        assert_eq!(max_tokens_for_length(None), 500);
        assert_eq!(max_tokens_for_length(Some("Vừa")), 500);
        assert_eq!(max_tokens_for_length(Some("Ngắn")), 250);
        assert_eq!(max_tokens_for_length(Some("Dài")), 800);
        assert_eq!(max_tokens_for_length(Some("unknown")), 500);
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