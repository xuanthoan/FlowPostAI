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
        "Bạn là chuyên gia viết nội dung Facebook với style: {}\n\nQUY TẮC ĐỊNH DẠNG BẮT BUỘC (tuân thủ tuyệt đối):\n1. Chỉ viết văn bản thuần (plain text). KHÔNG dùng markdown.\n2. KHÔNG dùng các ký tự: #, ##, ###, **, *, _, `, >, hoặc gạch đầu dòng markdown.\n3. Dòng đầu tiên là TIÊU ĐỀ, viết IN HOA toàn bộ.\n4. Sau tiêu đề, để trống 2 dòng rồi mới viết phần thân bài.\n5. Không viết tiêu đề và thân bài trên cùng một dòng.",
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

fn parse_ai_response(content: &str) -> (String, String) {
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
    let raw_base = provider.base_url.trim_end_matches('/').replace("localhost", "127.0.0.1").to_string();
    let base_url = raw_base.trim_end_matches("/chat/completions").trim_end_matches("/responses").trim_end_matches("/messages").trim_end_matches('/').to_string();
    let url = format!("{}/chat/completions", base_url);
    // Appends /chat/completions but handles case where base already contains suffix to avoid double
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
        .header("Content-Type", "application/json");
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
    let response_body = resp.text().await.map_err(|e| e.to_string())?;

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
    if payload.prompt.trim().is_empty() {
        return Err("Vui lòng nhập prompt".into());
    }
    if payload.style.trim().is_empty() {
        return Err("Vui lòng chọn style".into());
    }

    let content = call_ai_provider(&app, &payload.prompt, &payload.style, payload.custom_style.as_deref(), payload.length.as_deref()).await?;
    let (title, body) = parse_ai_response(&content);

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
}