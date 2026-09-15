use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

// The full endpoint URL. We do NOT append any suffix to this — different servers
// use different paths (e.g. /v1/chat/completions, /v1/responses, /v1).
// Set this to the exact URL your server expects POST requests on.
pub const DEFAULT_BASE_URL: &str = "http://localhost:20128/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "xoay-vong-worker-web-128k";
pub const DEFAULT_PROVIDER_ID: &str = "custom-provider";
pub const DEFAULT_DISPLAY_NAME: &str = "Custom provider";
pub const VALID_PROTOCOLS: &[&str] = &["openai-completions", "openai-responses", "anthropic-messages"];
// Default protocol for new providers
pub const DEFAULT_PROTOCOL: &str = "openai-completions";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub protocol: String,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPublic {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub protocol: String,
    pub models: Vec<String>,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvidersFile {
    providers: Vec<Provider>,
    active_provider_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderPayload {
    pub id: String,
    pub display_name: Option<String>,
    pub base_url: String,
    pub protocol: String,
    pub models: Option<Vec<String>>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderPayload {
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub protocol: Option<String>,
    pub models: Option<Vec<String>>,
    pub api_key: Option<String>,
}

fn providers_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("providers.json"))
}

#[cfg(test)]
fn providers_path_for_test(dir: &Path) -> PathBuf {
    dir.join("providers.json")
}

fn api_key_name(id: &str) -> String {
    format!("ai_provider_{}_key", id)
}

fn is_valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() < 2 || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    for c in id.chars() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
    }
    // no leading/trailing -, no --
    if id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return false;
    }
    true
}

fn validate_protocol(protocol: &str) -> Result<(), String> {
    if !VALID_PROTOCOLS.contains(&protocol) {
        return Err(format!(
            "Protocol không hợp lệ: {}. Chỉ hỗ trợ: {}",
            protocol,
            VALID_PROTOCOLS.join(", ")
        ));
    }
    Ok(())
}

fn validate_base_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Base URL không được để trống".into());
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("Base URL phải bắt đầu bằng http:// hoặc https://".into());
    }
    if trimmed.len() > 2048 {
        return Err("Base URL quá dài".into());
    }
    Ok(())
}

fn normalize_models(models: Option<Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(list) = models {
        for m in list {
            let trimmed = m.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if !out.contains(&trimmed) {
                out.push(trimmed);
            }
        }
    }
    out
}

fn to_public(provider: &Provider, has_api_key: bool) -> ProviderPublic {
    ProviderPublic {
        id: provider.id.clone(),
        display_name: provider.display_name.clone(),
        base_url: provider.base_url.clone(),
        protocol: provider.protocol.clone(),
        models: provider.models.clone(),
        has_api_key,
    }
}

fn has_api_key_for<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    let key_name = api_key_name(id);
    match crate::security::get_secret_hybrid(Some(app), &key_name) {
        Ok(Some(v)) if !v.trim().is_empty() => true,
        _ => false,
    }
}

fn default_provider() -> Provider {
    Provider {
        id: DEFAULT_PROVIDER_ID.to_string(),
        display_name: DEFAULT_DISPLAY_NAME.to_string(),
        base_url: DEFAULT_BASE_URL.to_string(),
        protocol: DEFAULT_PROTOCOL.to_string(),
        models: vec![DEFAULT_MODEL.to_string()],
    }
}

fn default_file() -> ProvidersFile {
    let p = default_provider();
    ProvidersFile {
        active_provider_id: p.id.clone(),
        providers: vec![p],
    }
}

pub fn resolve_active_provider<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(Provider, String), String> {
    let file = read_providers_file(app)?;
    let provider = file
        .providers
        .iter()
        .find(|p| p.id == file.active_provider_id)
        .cloned()
        .ok_or("Không tìm thấy active provider")?;
    let key = crate::security::get_secret_hybrid(Some(app), &api_key_name(&provider.id))
        .ok()
        .flatten()
        .ok_or("Chưa cấu hình API key. Vui lòng vào Cài đặt để thêm key.")?;
    if key.trim().is_empty() {
        return Err("API key trống, vui lòng kiểm tra lại.".into());
    }
    Ok((provider, key))
}

#[cfg(test)]
pub fn resolve_active_provider_at(
    dir: &std::path::Path,
    key_map: &std::collections::HashMap<String, String>,
) -> Result<(Provider, String), String> {
    let path = providers_path_for_test(dir);
    let file = read_providers_file_at(&path)?;
    let provider = file
        .providers
        .iter()
        .find(|p| p.id == file.active_provider_id)
        .cloned()
        .ok_or("Không tìm thấy active provider")?;
    let key_name = api_key_name(&provider.id);
    let key = key_map
        .get(&key_name)
        .cloned()
        .ok_or("Chưa cấu hình API key")?;
    if key.trim().is_empty() {
        return Err("API key trống".into());
    }
    Ok((provider, key))
}

fn read_providers_file<R: Runtime>(app: &AppHandle<R>) -> Result<ProvidersFile, String> {
    let path = providers_path(app)?;
    if !path.exists() {
        let file = default_file();
        // Migrate legacy ai_provider_key if exists (giữ 1 release, xóa key cũ sau copy)
        if let Ok(Some(legacy)) = crate::security::get_secret_hybrid(Some(app), "ai_provider_key") {
            if !legacy.trim().is_empty() {
                let new_key = api_key_name(DEFAULT_PROVIDER_ID);
                // copy to new key, best effort, rồi xóa key cũ
                let _ = crate::security::set_secret_hybrid(Some(app), &new_key, &legacy);
                let _ = crate::security::delete_secret_hybrid(Some(app), "ai_provider_key");
            }
        }
        write_providers_file(app, &file)?;
        return Ok(file);
    }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut file: ProvidersFile = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    // Ensure at least one provider, and protocol/models sanity
    if file.providers.is_empty() {
        file = default_file();
        write_providers_file(app, &file)?;
    }
    // Ensure active id exists
    if !file.providers.iter().any(|p| p.id == file.active_provider_id) {
        file.active_provider_id = file.providers[0].id.clone();
        write_providers_file(app, &file)?;
    }
    Ok(file)
}

fn write_providers_file<R: Runtime>(app: &AppHandle<R>, file: &ProvidersFile) -> Result<(), String> {
    let path = providers_path(app)?;
    write_providers_file_at(&path, file)
}

fn write_providers_file_at(path: &Path, file: &ProvidersFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
fn read_providers_file_at(path: &Path) -> Result<ProvidersFile, String> {
    if !path.exists() {
        return Ok(default_file());
    }
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

#[cfg(test)]
fn write_providers_at(path: &Path, file: &ProvidersFile) -> Result<(), String> {
    write_providers_file_at(path, file)
}

// --- Commands ---

#[tauri::command]
pub fn list_providers(app: AppHandle) -> Result<Vec<ProviderPublic>, String> {
    let file = read_providers_file(&app)?;
    Ok(file
        .providers
        .iter()
        .map(|p| to_public(p, has_api_key_for(&app, &p.id)))
        .collect())
}

#[tauri::command]
pub fn get_active_provider(app: AppHandle) -> Result<ProviderPublic, String> {
    let file = read_providers_file(&app)?;
    let provider = file
        .providers
        .iter()
        .find(|p| p.id == file.active_provider_id)
        .ok_or("Không tìm thấy active provider")?;
    Ok(to_public(provider, has_api_key_for(&app, &provider.id)))
}

#[tauri::command]
pub fn create_provider(app: AppHandle, payload: CreateProviderPayload) -> Result<ProviderPublic, String> {
    let id = payload.id.trim().to_lowercase();
    if !is_valid_id(&id) {
        return Err("Provider ID không hợp lệ: phải lowercase, bắt đầu bằng chữ cái, chỉ chứa a-z, 0-9, '-', không --, 1-64 ký tự".into());
    }
    validate_base_url(&payload.base_url)?;
    validate_protocol(&payload.protocol)?;
    let mut file = read_providers_file(&app)?;
    if file.providers.iter().any(|p| p.id == id) {
        return Err(format!("Provider ID '{}' đã tồn tại", id));
    }
    let display_name = payload
        .display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.clone());
    let models = normalize_models(payload.models);
    let provider = Provider {
        id: id.clone(),
        display_name,
        base_url: payload.base_url.trim().to_string(),
        protocol: payload.protocol.trim().to_string(),
        models: if models.is_empty() {
            vec![DEFAULT_MODEL.to_string()]
        } else {
            models
        },
    };
    if let Some(key) = payload.api_key {
        if !key.trim().is_empty() {
            crate::security::set_secret_hybrid(Some(&app), &api_key_name(&id), &key)?;
        }
    }
    file.providers.push(provider.clone());
    file.active_provider_id = provider.id.clone();
    write_providers_file(&app, &file)?;
    Ok(to_public(&provider, has_api_key_for(&app, &id)))
}

#[tauri::command]
pub fn update_provider(
    app: AppHandle,
    id: String,
    payload: UpdateProviderPayload,
) -> Result<ProviderPublic, String> {
    let id = id.trim().to_lowercase();
    let mut file = read_providers_file(&app)?;
    let idx = file
        .providers
        .iter()
        .position(|p| p.id == id)
        .ok_or(format!("Không tìm thấy provider '{}'", id))?;

    if let Some(display_name) = payload.display_name {
        let trimmed = display_name.trim().to_string();
        if !trimmed.is_empty() {
            file.providers[idx].display_name = trimmed;
        }
    }
    if let Some(base_url) = payload.base_url {
        validate_base_url(&base_url)?;
        file.providers[idx].base_url = base_url.trim().to_string();
    }
    if let Some(protocol) = payload.protocol {
        validate_protocol(&protocol)?;
        file.providers[idx].protocol = protocol.trim().to_string();
    }
    if let Some(models) = payload.models {
        let normalized = normalize_models(Some(models));
        if !normalized.is_empty() {
            file.providers[idx].models = normalized;
        }
    }
    if let Some(api_key) = payload.api_key {
        if api_key.trim().is_empty() {
            crate::security::delete_secret_hybrid(Some(&app), &api_key_name(&id))?;
        } else {
            crate::security::set_secret_hybrid(Some(&app), &api_key_name(&id), &api_key)?;
        }
    }
    let provider = file.providers[idx].clone();
    write_providers_file(&app, &file)?;
    Ok(to_public(&provider, has_api_key_for(&app, &id)))
}

#[tauri::command]
pub fn delete_provider(app: AppHandle, id: String) -> Result<(), String> {
    let id = id.trim().to_lowercase();
    let mut file = read_providers_file(&app)?;
    if file.providers.len() <= 1 {
        return Err("Không thể xóa provider cuối cùng. Hãy update thay vì xóa.".into());
    }
    let pos = file
        .providers
        .iter()
        .position(|p| p.id == id)
        .ok_or(format!("Không tìm thấy provider '{}'", id))?;
    file.providers.remove(pos);
    crate::security::delete_secret_hybrid(Some(&app), &api_key_name(&id))?;
    if file.active_provider_id == id {
        file.active_provider_id = file.providers[0].id.clone();
    }
    write_providers_file(&app, &file)?;
    Ok(())
}

#[tauri::command]
pub async fn fetch_provider_models(app: AppHandle, id: String) -> Result<Vec<String>, String> {
    let file = read_providers_file(&app)?;
    let provider = file
        .providers
        .iter()
        .find(|p| p.id == id.trim().to_lowercase())
        .cloned()
        .ok_or(format!("Không tìm thấy provider '{}'", id))?;
    let key = crate::security::get_secret_hybrid(Some(&app), &api_key_name(&provider.id))
        .ok()
        .flatten()
        .ok_or("Chưa cấu hình API key cho provider này".to_string())?;
    if key.trim().is_empty() {
        return Err("API key trống".into());
    }
    let base = provider.base_url.trim_end_matches('/').to_string();
    let url = format!("{}/models", base);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Không thể khởi tạo HTTP client: {}", e))?;
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối provider để fetch models: {}", e))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("Lỗi fetch models ({}): {}", status, &body.chars().take(300).collect::<String>()));
    }
    // Try OpenAI format {"data":[{"id":"model-x"},...]} or {"object":"list","data":[...]}
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
            let mut models: Vec<String> = data
                .iter()
                .filter_map(|v| {
                    v.get("id")
                        .and_then(|id| id.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                })
                .collect();
            if !models.is_empty() {
                models.sort();
                models.dedup();
                return Ok(models);
            }
        }
        // fallback: if body is array directly
        if let Some(arr) = json.as_array() {
            let models: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !models.is_empty() {
                return Ok(models);
            }
        }
    }
    Err(format!("Không parse được danh sách models: {}", &body.chars().take(300).collect::<String>()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn test_provider_id_validation() {
        assert!(is_valid_id("acme-gateway"));
        assert!(is_valid_id("custom-provider"));
        assert!(is_valid_id("a1"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("Acme")); // uppercase
        assert!(!is_valid_id("1acme")); // starts with digit
        assert!(!is_valid_id("-acme"));
        assert!(!is_valid_id("acme-"));
        assert!(!is_valid_id("acme--gateway"));
        assert!(!is_valid_id("acme_gateway"));
        assert!(!is_valid_id("a"));
        // boundary length
        let long = "a".repeat(64);
        assert!(is_valid_id(&long));
        let too_long = "a".repeat(65);
        assert!(!is_valid_id(&too_long));
    }

    #[test]
    fn test_validate_protocol() {
        assert!(validate_protocol("openai-completions").is_ok());
        assert!(validate_protocol("openai-responses").is_ok());
        assert!(validate_protocol("anthropic-messages").is_ok());
        assert!(validate_protocol("invalid").is_err());
        assert!(validate_protocol("invalid-protocol").is_err());
    }

    #[test]
    fn test_validate_base_url() {
        assert!(validate_base_url("http://localhost:20128/v1").is_ok());
        assert!(validate_base_url("https://gateway.example/v1").is_ok());
        assert!(validate_base_url("").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
        assert!(validate_base_url("gateway.example/v1").is_err());
    }

    #[test]
    fn test_normalize_models() {
        let input = Some(vec!["  model-a  ".into(), "".into(), "model-a".into(), "model-b".into()]);
        let out = normalize_models(input);
        assert_eq!(out, vec!["model-a", "model-b"]);
    }

    #[test]
    fn test_providers_file_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_providers_atomic");
        let path = providers_path_for_test(&dir);
        let tmp = path.with_extension("json.tmp");
        let file = default_file();
        write_providers_at(&path, &file).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        let file2 = ProvidersFile {
            providers: vec![Provider {
                id: "acme-gateway".into(),
                display_name: "Acme".into(),
                base_url: "https://example.com/v1".into(),
                protocol: "openai-completions".into(),
                models: vec!["model-x".into()],
            }],
            active_provider_id: "acme-gateway".into(),
        };
        write_providers_at(&path, &file2).unwrap();
        assert!(!tmp.exists());
        let read = read_providers_file_at(&path).unwrap();
        assert_eq!(read.providers[0].id, "acme-gateway");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_providers_file_preserves_on_tmp_failure() {
        let dir = unique_temp_dir("flowpost_providers_preserve");
        let path = providers_path_for_test(&dir);
        let file = default_file();
        write_providers_at(&path, &file).unwrap();
        let original = fs::read_to_string(&path).unwrap();
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "invalid json that will not be renamed").unwrap();
        let still = fs::read_to_string(&path).unwrap();
        assert_eq!(original, still);
        fs::remove_file(&tmp).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_default_provider() {
        let p = default_provider();
        assert_eq!(p.id, "custom-provider");
        assert_eq!(p.base_url, DEFAULT_BASE_URL);
        assert_eq!(p.protocol, "openai-completions");
        assert!(p.models.contains(&DEFAULT_MODEL.to_string()));
    }

    #[test]
    fn test_api_key_name() {
        assert_eq!(api_key_name("custom-provider"), "ai_provider_custom-provider_key");
        assert_eq!(api_key_name("acme-gateway"), "ai_provider_acme-gateway_key");
    }

    #[test]
    fn test_resolve_active_provider_at_default() {
        let dir = unique_temp_dir("flowpost_resolve_default");
        let path = providers_path_for_test(&dir);
        let file = default_file();
        write_providers_at(&path, &file).unwrap();
        let mut key_map = std::collections::HashMap::new();
        key_map.insert(
            api_key_name("custom-provider"),
            "test_key_123".to_string(),
        );
        let (provider, key) = resolve_active_provider_at(&dir, &key_map).unwrap();
        assert_eq!(provider.id, "custom-provider");
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(key, "test_key_123");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_resolve_active_provider_at_custom() {
        let dir = unique_temp_dir("flowpost_resolve_custom");
        let path = providers_path_for_test(&dir);
        let file = ProvidersFile {
            providers: vec![Provider {
                id: "acme-gateway".into(),
                display_name: "Acme Gateway".into(),
                base_url: "https://gateway.example/v1".into(),
                protocol: "openai-completions".into(),
                models: vec!["model-a".into(), "model-b".into()],
            }],
            active_provider_id: "acme-gateway".into(),
        };
        write_providers_at(&path, &file).unwrap();
        let mut key_map = std::collections::HashMap::new();
        key_map.insert(api_key_name("acme-gateway"), "secret_acme".to_string());
        let (provider, key) = resolve_active_provider_at(&dir, &key_map).unwrap();
        assert_eq!(provider.id, "acme-gateway");
        assert_eq!(provider.base_url, "https://gateway.example/v1");
        assert_eq!(provider.models, vec!["model-a", "model-b"]);
        assert_eq!(key, "secret_acme");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_resolve_missing_key_fails() {
        let dir = unique_temp_dir("flowpost_resolve_missing");
        let path = providers_path_for_test(&dir);
        let file = default_file();
        write_providers_at(&path, &file).unwrap();
        let key_map = std::collections::HashMap::new();
        let err = resolve_active_provider_at(&dir, &key_map).unwrap_err();
        assert!(err.contains("API key"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
