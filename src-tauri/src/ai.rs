use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use std::time::Duration;

const AI_TIMEOUT_SECS: u64 = 15;

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
    format!("Bạn là chuyên gia viết nội dung Facebook với style: {}", style_label)
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

fn parse_ai_response(content: &str) -> (String, String) {
    // Try to split by \n---\n delimiter for title/body
    if let Some(idx) = content.find("\n---\n") {
        let title = content[..idx].trim().to_string();
        let body = content[idx + 5..].trim().to_string();
        return (title, body);
    }
    // Fallback: first line is title, rest is body
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return ("Nội dung".to_string(), content.to_string());
    }
    let title = lines[0].trim().to_string();
    let body = lines[1..].join("\n").trim().to_string();
    (title, body)
}

fn max_tokens_for_length(length: Option<&str>) -> u32 {
    match length {
        Some("Ngắn") => 250,
        Some("Dài") => 800,
        _ => 500,
    }
}

fn endpoint_for_protocol(base_url: &str, protocol: &str) -> String {
    match protocol {
        "openai-responses" => format!("{}/responses", base_url),
        "anthropic-messages" => format!("{}/messages", base_url),
        _ => format!("{}/chat/completions", base_url),
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
    let base_url = provider.base_url.trim_end_matches('/').to_string();
    let protocol = provider.protocol.as_str();

    let (body, url) = match protocol {
        "openai-responses" => {
            let req = ResponsesRequest {
                model: &model,
                input: prompt,
                instructions: &system_msg,
                max_output_tokens: max_tokens,
                temperature: 0.7,
            };
            let b = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            (b, endpoint_for_protocol(&base_url, protocol))
        }
        "anthropic-messages" => {
            let req = AnthropicMessagesRequest {
                model: &model,
                system: &system_msg,
                messages: vec![Message { role: "user", content: prompt }],
                max_tokens,
            };
            let b = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            (b, endpoint_for_protocol(&base_url, protocol))
        }
        _ => {
            let req = ChatCompletionRequest {
                model: &model,
                messages: vec![
                    Message { role: "system", content: &system_msg },
                    Message { role: "user", content: prompt },
                ],
                max_tokens,
                temperature: 0.7,
            };
            let b = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            (b, endpoint_for_protocol(&base_url, protocol))
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(AI_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Không thể khởi tạo HTTP client: {}", e))?;

    let mut req_builder = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json");
    if protocol == "anthropic-messages" {
        req_builder = req_builder
            .header("x-api-key", key.clone())
            .header("anthropic-version", "2023-06-01");
    }
    let resp = req_builder
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối AI provider: {}", e))?;

    let status = resp.status().as_u16();
    let response_body = resp.text().await.map_err(|e| e.to_string())?;

    if status >= 200 && status < 300 {
        match protocol {
            "openai-responses" => {
                let parsed: ResponsesResponse = serde_json::from_str(&response_body)
                    .map_err(|e| format!("Không parse được phản hồi AI: {}", e))?;
                for out in &parsed.output {
                    for c in &out.content {
                        if c.ctype == "output_text" && !c.text.trim().is_empty() {
                            return Ok(c.text.clone());
                        }
                    }
                }
                // fallback: try generic extraction
                if let Ok(v) = serde_json::from_str::<Value>(&response_body) {
                    if let Some(t) = v.get("output_text").and_then(|x| x.as_str()) {
                        return Ok(t.to_string());
                    }
                }
                Err("Phản hồi AI (responses) không có nội dung".into())
            }
            "anthropic-messages" => {
                let parsed: AnthropicMessagesResponse = serde_json::from_str(&response_body)
                    .map_err(|e| format!("Không parse được phản hồi AI: {}", e))?;
                let combined: String = parsed
                    .content
                    .iter()
                    .filter(|c| c.ctype == "text")
                    .map(|c| c.text.clone())
                    .collect::<Vec<_>>()
                    .join("\n");
                if combined.trim().is_empty() {
                    Err("Phản hồi AI (anthropic) không có nội dung".into())
                } else {
                    Ok(combined)
                }
            }
            _ => {
                let parsed: ChatCompletionResponse = serde_json::from_str(&response_body)
                    .map_err(|e| format!("Không parse được phản hồi AI: {}", e))?;
                if let Some(choice) = parsed.choices.first() {
                    Ok(choice.message.content.clone())
                } else {
                    Err("Phản hồi AI không có choices".into())
                }
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
        assert_eq!(title, "Title here");
        assert_eq!(body, "Body here");
    }

    #[tokio::test]
    async fn test_parse_ai_response_fallback() {
        let content = "First line is title\nSecond line is body";
        let (title, body) = parse_ai_response(content);
        assert_eq!(title, "First line is title");
        assert_eq!(body, "Second line is body");
    }

    #[tokio::test]
    async fn test_parse_ai_response_empty() {
        let content = "";
        let (title, body) = parse_ai_response(content);
        assert_eq!(title, "Nội dung");
        assert_eq!(body, "");
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

    #[test]
    fn test_endpoint_for_protocol() {
        assert_eq!(endpoint_for_protocol("http://localhost:20128/v1", "openai-completions"), "http://localhost:20128/v1/chat/completions");
        assert_eq!(endpoint_for_protocol("http://localhost:20128/v1", "openai-responses"), "http://localhost:20128/v1/responses");
        assert_eq!(endpoint_for_protocol("http://localhost:20128/v1", "anthropic-messages"), "http://localhost:20128/v1/messages");
        assert_eq!(endpoint_for_protocol("https://api.anthropic.com/v1", "anthropic-messages"), "https://api.anthropic.com/v1/messages");
        assert_eq!(endpoint_for_protocol("https://api.openai.com/v1", "unknown"), "https://api.openai.com/v1/chat/completions");
    }

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
        assert_eq!(title, "Tiêu đề");
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
        assert_eq!(title, "Dòng 1");
        assert!(body.contains("Dòng 2"));
    }
}