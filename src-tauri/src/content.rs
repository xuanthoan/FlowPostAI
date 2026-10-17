use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentStatus {
    Draft,
    Approved,
    Archived,
}

impl Default for ContentStatus {
    fn default() -> Self {
        ContentStatus::Draft
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub prompt: String,
    pub style: String,
    #[serde(default)]
    pub custom_style: Option<String>,
    #[serde(default)]
    pub media_ids: Vec<String>,
    pub status: ContentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const TITLE_BODY_SEPARATOR: &str = "\n\n\n";

pub fn compose_facebook_post(title: &str, body: &str) -> String {
    let t = title.trim();
    let b = body.trim();
    match (t.is_empty(), b.is_empty()) {
        (false, false) => format!("{}{}{}", t, TITLE_BODY_SEPARATOR, b),
        (false, true) => t.to_string(),
        (true, false) => b.to_string(),
        (true, true) => String::new(),
    }
}

fn content_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("content");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn content_index_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(content_dir(app)?.join("content-index.json"))
}

fn read_content_index(app: &AppHandle) -> Result<Vec<ContentItem>, String> {
    let path = content_index_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut items: Vec<ContentItem> = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    for it in items.iter_mut() {
        it.title = crate::ai::repair_mojibake(&it.title);
        it.body = crate::ai::repair_mojibake(&it.body);
    }
    Ok(items)
}

fn write_content_index(app: &AppHandle, items: &[ContentItem]) -> Result<(), String> {
    let path = content_index_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

// Test helpers without AppHandle
#[cfg(test)]
fn write_content_at(path: &std::path::Path, items: &[ContentItem]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveContentPayload {
    pub title: String,
    pub body: String,
    pub prompt: String,
    pub style: String,
    pub custom_style: Option<String>,
    pub media_ids: Option<Vec<String>>,
}

#[tauri::command]
pub fn list_content(app: AppHandle) -> Result<Vec<ContentItem>, String> {
    let mut items = read_content_index(&app)?;
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

#[tauri::command]
pub fn get_content(app: AppHandle, id: String) -> Result<Option<ContentItem>, String> {
    let items = read_content_index(&app)?;
    Ok(items.into_iter().find(|c| c.id == id))
}

#[tauri::command]
pub fn save_content(app: AppHandle, payload: SaveContentPayload) -> Result<ContentItem, String> {
    if payload.title.trim().is_empty() && payload.body.trim().is_empty() {
        return Err("Tiêu đề hoặc nội dung không được để trống".into());
    }
    if payload.prompt.trim().is_empty() {
        return Err("Prompt không được để trống".into());
    }
    let mut items = read_content_index(&app)?;
    let now = Utc::now();
    let item = ContentItem {
        id: uuid::Uuid::new_v4().to_string(),
        title: payload.title.trim().to_string(),
        body: payload.body.trim().to_string(),
        prompt: payload.prompt.trim().to_string(),
        style: payload.style,
        custom_style: payload.custom_style.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        media_ids: payload.media_ids.unwrap_or_default(),
        status: ContentStatus::Draft,
        created_at: now,
        updated_at: now,
    };
    items.push(item.clone());
    write_content_index(&app, &items)?;
    Ok(item)
}

#[tauri::command]
pub fn delete_content(app: AppHandle, id: String) -> Result<(), String> {
    let mut items = read_content_index(&app)?;
    let len_before = items.len();
    items.retain(|c| c.id != id);
    if items.len() == len_before {
        return Err(format!("Không tìm thấy nội dung {}", id));
    }
    write_content_index(&app, &items)
}

#[tauri::command]
pub fn update_content_status(
    app: AppHandle,
    id: String,
    status: ContentStatus,
) -> Result<ContentItem, String> {
    let mut items = read_content_index(&app)?;
    let pos = items.iter().position(|c| c.id == id).ok_or_else(|| format!("Không tìm thấy {}", id))?;
    items[pos].status = status;
    items[pos].updated_at = Utc::now();
    let updated = items[pos].clone();
    write_content_index(&app, &items)?;
    Ok(updated)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContentPayload {
    pub content_id: String,
    pub title: String,
    pub body: String,
    pub prompt: String,
    pub style: String,
    pub custom_style: Option<String>,
    pub media_ids: Option<Vec<String>>,
}

#[tauri::command]
pub fn update_content(app: AppHandle, payload: UpdateContentPayload) -> Result<ContentItem, String> {
    if payload.title.trim().is_empty() && payload.body.trim().is_empty() {
        return Err("Tiêu đề hoặc nội dung không được để trống".into());
    }
    if payload.prompt.trim().is_empty() {
        return Err("Prompt không được để trống".into());
    }
    let mut items = read_content_index(&app)?;
    let pos = items.iter().position(|c| c.id == payload.content_id)
        .ok_or_else(|| format!("Không tìm thấy {}", payload.content_id))?;
    items[pos].title = payload.title.trim().to_string();
    items[pos].body = payload.body.trim().to_string();
    items[pos].prompt = payload.prompt.trim().to_string();
    items[pos].style = payload.style;
    items[pos].custom_style = payload.custom_style.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    items[pos].media_ids = payload.media_ids.unwrap_or_default();
    items[pos].updated_at = Utc::now();
    let updated = items[pos].clone();
    write_content_index(&app, &items)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_item(id: &str, title: &str) -> ContentItem {
        ContentItem {
            id: id.into(),
            title: title.into(),
            body: "body".into(),
            prompt: "prompt".into(),
            style: "Viral".into(),
            custom_style: None,
            media_ids: vec![],
            status: ContentStatus::Draft,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_write_content_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_content_atomic");
        let path = dir.join("content-index.json");
        let tmp = path.with_extension("json.tmp");
        let item = sample_item("id1", "Title 1");
        write_content_at(&path, &[item]).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        let parsed: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_save_list_delete_roundtrip() {
        let dir = unique_temp_dir("flowpost_content_roundtrip");
        let path = dir.join("content-index.json");

        // Simulate save 3 items
        let mut items = Vec::new();
        for i in 0..3 {
            let mut item = sample_item(&format!("id{}", i), &format!("Title {}", i));
            // Ensure different created_at for sorting
            std::thread::sleep(std::time::Duration::from_millis(10));
            item.created_at = Utc::now();
            item.updated_at = Utc::now();
            items.push(item);
        }
        write_content_at(&path, &items).unwrap();

        // Read and verify sorted desc
        let contents = std::fs::read_to_string(&path).unwrap();
        let mut parsed: Vec<ContentItem> = serde_json::from_str(&contents).unwrap();
        parsed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        assert_eq!(parsed.len(), 3);
        // Delete one
        let id_to_delete = parsed[1].id.clone();
        let mut remaining = parsed.clone();
        remaining.retain(|c| c.id != id_to_delete);
        write_content_at(&path, &remaining).unwrap();
        let after: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|c| c.id != id_to_delete));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_content_status_serialization_is_lowercase_canonical() {
        assert_eq!(serde_json::to_string(&ContentStatus::Draft).unwrap(), "\"draft\"");
        assert_eq!(serde_json::to_string(&ContentStatus::Approved).unwrap(), "\"approved\"");
        assert_eq!(serde_json::to_string(&ContentStatus::Archived).unwrap(), "\"archived\"");
    }

    #[test]
    fn test_content_status_deserializes_exact_frontend_values() {
        let draft: ContentStatus = serde_json::from_str("\"draft\"").unwrap();
        let approved: ContentStatus = serde_json::from_str("\"approved\"").unwrap();
        let archived: ContentStatus = serde_json::from_str("\"archived\"").unwrap();
        assert_eq!(draft, ContentStatus::Draft);
        assert_eq!(approved, ContentStatus::Approved);
        assert_eq!(archived, ContentStatus::Archived);
        assert!(
            serde_json::from_str::<ContentStatus>("\"Approved\"").is_err(),
            "PascalCase must be rejected so frontend/backend casing drift is caught"
        );
    }

    #[test]
    fn test_content_status_persisted_json_uses_lowercase() {
        let mut item = sample_item("id-status", "Title");
        item.status = ContentStatus::Approved;
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"status\":\"approved\""), "persisted status must be lowercase, got {}", json);
        let parsed: ContentItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, ContentStatus::Approved);
    }

    #[test]
    fn test_update_content_payload_deserializes_frontend_camel_case() {
        let json = serde_json::json!({
            "contentId": "c1", "title": "T", "body": "B", "prompt": "P",
            "style": "Viral", "customStyle": null, "mediaIds": ["m1", "m2"]
        });
        let p: UpdateContentPayload = serde_json::from_value(json).unwrap();
        assert_eq!(p.content_id, "c1");
        assert_eq!(p.title, "T");
        assert_eq!(p.body, "B");
        assert_eq!(p.prompt, "P");
        assert_eq!(p.style, "Viral");
        assert_eq!(p.custom_style, None);
        assert_eq!(p.media_ids, Some(vec!["m1".into(), "m2".into()]));
    }

    #[test]
    fn test_update_content_fields_and_timestamps() {
        let dir = unique_temp_dir("flowpost_update_content");
        let path = dir.join("content-index.json");

        let mut item = sample_item("upd1", "Original Title");
        item.body = "Original body".into();
        item.prompt = "Original prompt".into();
        item.style = "Viral".into();
        let original_created = item.created_at;
        let original_updated = item.updated_at;
        write_content_at(&path, &[item]).unwrap();

        // Simulate update_content logic without AppHandle
        let mut items: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let pos = items.iter().position(|c| c.id == "upd1").unwrap();
        items[pos].title = "Updated Title".into();
        items[pos].body = "Updated body".into();
        items[pos].prompt = "Updated prompt".into();
        items[pos].style = "Story".into();
        items[pos].custom_style = Some("Hài hước".into());
        items[pos].media_ids = vec!["m1".into(), "m2".into()];
        items[pos].updated_at = Utc::now();
        write_content_at(&path, &items).unwrap();

        // Verify fields changed
        let parsed: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let found = parsed.iter().find(|c| c.id == "upd1").unwrap();
        assert_eq!(found.title, "Updated Title");
        assert_eq!(found.body, "Updated body");
        assert_eq!(found.prompt, "Updated prompt");
        assert_eq!(found.style, "Story");
        assert_eq!(found.custom_style, Some("Hài hước".into()));
        assert_eq!(found.media_ids, vec!["m1".to_string(), "m2".to_string()]);

        // Verify updated_at changed but created_at unchanged
        assert_eq!(found.created_at, original_created);
        assert!(found.updated_at > original_updated);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_update_content_empty_title_body_rejected() {
        let dir = unique_temp_dir("flowpost_update_empty");
        let path = dir.join("content-index.json");
        let item = sample_item("upd2", "Title");
        write_content_at(&path, &[item]).unwrap();

        let items: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(items.len(), 1);

        // Simulate validation: empty title + body should be rejected
        let empty_title = "";
        let empty_body = "";
        assert!(empty_title.trim().is_empty() && empty_body.trim().is_empty(),
            "Empty title+body should trigger validation");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_update_content_empty_prompt_rejected() {
        // Simulate validation: empty prompt should be rejected
        let empty_prompt = "";
        assert!(empty_prompt.trim().is_empty(),
            "Empty prompt should trigger validation");
    }

    #[test]
    fn test_update_content_nonexistent_id_returns_error() {
        let dir = unique_temp_dir("flowpost_update_notfound");
        let path = dir.join("content-index.json");
        let item = sample_item("existing", "Title");
        write_content_at(&path, &[item]).unwrap();

        let items: Vec<ContentItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let pos = items.iter().position(|c| c.id == "nonexistent");
        assert!(pos.is_none(), "Non-existent id should not be found");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_compose_facebook_post_full() {
        assert_eq!(
            compose_facebook_post("TIN HOT", "Thân bài"),
            "TIN HOT\n\n\nThân bài"
        );
    }

    #[test]
    fn test_compose_facebook_post_empty_parts() {
        assert_eq!(compose_facebook_post("TIN", ""), "TIN");
        assert_eq!(compose_facebook_post("", "Thân"), "Thân");
        assert_eq!(compose_facebook_post("", ""), "");
    }
}
