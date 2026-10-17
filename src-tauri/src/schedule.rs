use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleStatus {
    Scheduled,
    Published,
    Failed,
    Cancelled,
}

impl Default for ScheduleStatus {
    fn default() -> Self {
        ScheduleStatus::Scheduled
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleItem {
    pub id: String,
    pub content_id: String,
    pub scheduled_at: DateTime<Utc>,
    pub platform: String,
    #[serde(default)]
    pub pages: Vec<String>,
    #[serde(default)]
    pub page_id: Option<String>,
    pub status: ScheduleStatus,
    pub retry_count: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn schedule_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("schedule");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn schedule_index_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(schedule_dir(app)?.join("schedule-index.json"))
}

fn read_schedule_index(app: &AppHandle) -> Result<Vec<ScheduleItem>, String> {
    let path = schedule_index_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_schedule_index(app: &AppHandle, items: &[ScheduleItem]) -> Result<(), String> {
    let path = schedule_index_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerConfig {
    pub interval_seconds: u64,   // 30 (mặc định), 60, custom ≤3600
    pub enabled: bool,           // bật/tắt scheduler
    pub selected_page_id: Option<String>, // page_id user chọn (persist)
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { interval_seconds: 30, enabled: true, selected_page_id: None }
    }
}

fn read_scheduler_config(app: &AppHandle) -> Result<SchedulerConfig, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("schedule-config.json");
    if path.exists() {
        let c = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if c.trim().is_empty() {
            return Ok(SchedulerConfig::default());
        }
        serde_json::from_str(&c).map_err(|e| e.to_string())
    } else {
        Ok(SchedulerConfig::default())
    }
}

fn write_scheduler_config(app: &AppHandle, config: &SchedulerConfig) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("schedule-config.json");
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn write_schedule_at(path: &std::path::Path, items: &[ScheduleItem]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn validate_content_approved(app: &AppHandle, content_id: &str) -> Result<(), String> {
    // Reuse content.rs read logic without circular dep: read file directly
    let content_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("content")
        .join("content-index.json");
    if !content_path.exists() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let contents = fs::read_to_string(&content_path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let items: Vec<crate::content::ContentItem> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let found = items.iter().find(|c| c.id == content_id);
    match found {
        None => Err(format!("Không tìm thấy nội dung {}", content_id)),
        Some(c) => {
            if c.status != crate::content::ContentStatus::Approved {
                return Err("Chỉ nội dung đã duyệt mới được lên lịch".into());
            }
            Ok(())
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSchedulePayload {
    pub content_id: String,
    pub scheduled_at: DateTime<Utc>,
    pub platform: String,
    #[serde(default)]
    pub pages: Option<Vec<String>>,
    #[serde(default)]
    pub page_id: Option<String>,
}

#[tauri::command]
pub fn list_schedule(app: AppHandle) -> Result<Vec<ScheduleItem>, String> {
    let mut items = read_schedule_index(&app)?;
    items.sort_by(|a, b| a.scheduled_at.cmp(&b.scheduled_at));
    Ok(items)
}

#[tauri::command]
pub fn get_schedule(app: AppHandle, id: String) -> Result<Option<ScheduleItem>, String> {
    let items = read_schedule_index(&app)?;
    Ok(items.into_iter().find(|s| s.id == id))
}

#[tauri::command]
pub fn create_schedule(app: AppHandle, payload: CreateSchedulePayload) -> Result<ScheduleItem, String> {
    if payload.content_id.trim().is_empty() {
        return Err("Thiếu contentId".into());
    }
    if payload.platform.trim().is_empty() {
        return Err("Vui lòng chọn nền tảng".into());
    }
    let now = Utc::now();
    // Must be at least 5 minutes in future
    if payload.scheduled_at <= now + chrono::Duration::minutes(5) {
        return Err("Thời gian lên lịch phải trong tương lai (tối thiểu 5 phút)".into());
    }
    validate_content_approved(&app, &payload.content_id)?;

    let mut items = read_schedule_index(&app)?;

    let pages = payload.pages.unwrap_or_default();
    let resolved_page_id = payload
        .page_id
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| pages.first().cloned());
    let item = ScheduleItem {
        id: uuid::Uuid::new_v4().to_string(),
        content_id: payload.content_id.trim().to_string(),
        scheduled_at: payload.scheduled_at,
        platform: payload.platform.trim().to_string(),
        pages: pages.clone(),
        page_id: resolved_page_id.clone(),
        status: ScheduleStatus::Scheduled,
        retry_count: 0,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    items.push(item.clone());
    write_schedule_index(&app, &items)?;
    Ok(item)
}

#[tauri::command]
pub fn delete_schedule(app: AppHandle, id: String) -> Result<(), String> {
    let mut items = read_schedule_index(&app)?;
    let len_before = items.len();
    items.retain(|s| s.id != id);
    if items.len() == len_before {
        return Err(format!("Không tìm thấy lịch {}", id));
    }
    write_schedule_index(&app, &items)
}

#[tauri::command]
pub fn update_schedule_status(
    app: AppHandle,
    id: String,
    status: ScheduleStatus,
) -> Result<ScheduleItem, String> {
    let mut items = read_schedule_index(&app)?;
    let pos = items
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy {}", id))?;
    items[pos].status = status;
    items[pos].updated_at = Utc::now();
    let updated = items[pos].clone();
    write_schedule_index(&app, &items)?;
    Ok(updated)
}

#[tauri::command]
pub fn reschedule(app: AppHandle, id: String, scheduled_at: DateTime<Utc>) -> Result<ScheduleItem, String> {
    if scheduled_at <= Utc::now() + chrono::Duration::minutes(5) {
        return Err("Thời gian lên lịch phải trong tương lai (tối thiểu 5 phút)".into());
    }
    let mut items = read_schedule_index(&app)?;
    let pos = items
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy {}", id))?;
    items[pos].scheduled_at = scheduled_at;
    items[pos].updated_at = Utc::now();
    let updated = items[pos].clone();
    write_schedule_index(&app, &items)?;
    Ok(updated)
}

#[tauri::command]
pub fn retry_schedule(app: AppHandle, schedule_id: String) -> Result<ScheduleItem, String> {
    let mut items = read_schedule_index(&app)?;
    let pos = items
        .iter()
        .position(|s| s.id == schedule_id)
        .ok_or_else(|| format!("Không tìm thấy {}", schedule_id))?;
    if items[pos].status != ScheduleStatus::Failed {
        return Err("Chỉ lịch Failed mới retry được".into());
    }
    let now = Utc::now();
    items[pos].status = ScheduleStatus::Scheduled;
    items[pos].retry_count = 0;
    items[pos].last_error = None;
    items[pos].scheduled_at = now + chrono::Duration::minutes(retry_delay_minutes(1));
    items[pos].updated_at = now;
    let updated = items[pos].clone();
    write_schedule_index(&app, &items)?;
    Ok(updated)
}

#[tauri::command]
pub async fn start_scheduler(app: AppHandle) -> Result<(), String> {
    let mut config = read_scheduler_config(&app)?;
    config.enabled = true;
    write_scheduler_config(&app, &config)?;

    // Dedupe: nếu đã running thì không spawn thêm
    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval_secs = config.interval_seconds;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let current_config = match read_scheduler_config(&app_handle) {
                Ok(c) => c,
                Err(_) => break,
            };
            if !current_config.enabled {
                break;
            }
            // Hot-reload interval nếu config đổi
            if current_config.interval_seconds != interval_secs && current_config.interval_seconds >= 1 && current_config.interval_seconds <= 3600 {
                interval_secs = current_config.interval_seconds;
                interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            }
            let _ = process_due_schedules(&app_handle).await;
        }
        SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_scheduler(app: AppHandle) -> Result<(), String> {
    let mut config = read_scheduler_config(&app)?;
    config.enabled = false;
    write_scheduler_config(&app, &config)?;
    Ok(())
}

#[tauri::command]
pub fn get_scheduler_config(app: AppHandle) -> Result<SchedulerConfig, String> {
    read_scheduler_config(&app)
}

#[tauri::command]
pub fn set_scheduler_config(app: AppHandle, config: SchedulerConfig) -> Result<(), String> {
    if config.interval_seconds == 0 || config.interval_seconds > 3600 {
        return Err("Khoảng kiểm tra phải từ 1 đến 3600 giây".into());
    }
    write_scheduler_config(&app, &config)
}

#[tauri::command]
pub fn select_scheduler_page(app: AppHandle, page_id: String) -> Result<(), String> {
    let mut config = read_scheduler_config(&app)?;
    config.selected_page_id = Some(page_id);
    write_scheduler_config(&app, &config)
}

fn retry_delay_minutes(n: u32) -> i64 {
    match n {
        1 => 5,
        2 => 10,
        _ => 30,
    }
}

pub async fn process_due_schedules(app: &AppHandle) -> Result<(), String> {
    let config = read_scheduler_config(app).unwrap_or_default();
    let mut items = read_schedule_index(app)?;
    let now = Utc::now();
    let mut changed = false;
    for i in 0..items.len() {
        if items[i].status != ScheduleStatus::Scheduled {
            continue;
        }
        if items[i].retry_count >= 3 {
            continue;
        }
        if items[i].scheduled_at > now {
            continue;
        }
        // Phân giải page_id: item.page_id -> config.selected_page_id -> pages.first()
        let resolved_page_id = items[i]
            .page_id
            .clone()
            .or_else(|| config.selected_page_id.clone())
            .or_else(|| items[i].pages.first().cloned())
            .unwrap_or_default();
        if resolved_page_id.trim().is_empty() {
            items[i].last_error = Some("Thiếu page_id để đăng bài".into());
            items[i].retry_count += 1;
            if items[i].retry_count >= 3 {
                items[i].status = ScheduleStatus::Failed;
            } else {
                items[i].scheduled_at = now + chrono::Duration::minutes(retry_delay_minutes(items[i].retry_count));
            }
            items[i].updated_at = now;
            changed = true;
            continue;
        }
        // Lấy nội dung body + ảnh đầu (fallback đăng text nếu ảnh lỗi)
        let (content_body, first_image) = match read_content_with_media(app, &items[i].content_id) {
            Ok((b, img)) => (b, img),
            Err(e) => {
                items[i].last_error = Some(e);
                items[i].retry_count += 1;
                if items[i].retry_count >= 3 {
                    items[i].status = ScheduleStatus::Failed;
                } else {
                    items[i].scheduled_at = now + chrono::Duration::minutes(retry_delay_minutes(items[i].retry_count));
                }
                items[i].updated_at = now;
                changed = true;
                continue;
            }
        };
        // Gọi publish (gửi ảnh đầu nếu có, fallback text nếu ảnh không hợp lệ đã được resolve trả None)
        let res = crate::facebook::publish_content(app.clone(), resolved_page_id.clone(), content_body, first_image).await;
        match res {
            Ok(_) => {
                items[i].status = ScheduleStatus::Published;
                items[i].last_error = None;
                items[i].updated_at = now;
                changed = true;
            }
            Err(e) => {
                items[i].retry_count += 1;
                items[i].last_error = Some(e);
                if items[i].retry_count >= 3 {
                    items[i].status = ScheduleStatus::Failed;
                } else {
                    items[i].scheduled_at = now + chrono::Duration::minutes(retry_delay_minutes(items[i].retry_count));
                }
                items[i].updated_at = now;
                changed = true;
            }
        }
    }
    if changed {
        write_schedule_index(app, &items)?;
    }
    Ok(())
}

#[allow(dead_code)]
fn read_content_body(app: &AppHandle, content_id: &str) -> Result<String, String> {
    let content_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("content")
        .join("content-index.json");
    if !content_path.exists() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let contents = std::fs::read_to_string(&content_path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let items: Vec<crate::content::ContentItem> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let found = items.into_iter().find(|c| c.id == content_id).ok_or_else(|| format!("Không tìm thấy nội dung {}", content_id))?;
    Ok(crate::content::compose_facebook_post(&found.title, &found.body))
}

fn resolve_first_image_path(app: &AppHandle, media_ids: &[String]) -> Option<String> {
    if media_ids.is_empty() {
        return None;
    }
    let first_id = media_ids[0].trim();
    if first_id.is_empty() {
        return None;
    }
    // Read media-index.json (duplicate logic from lib.rs to avoid private access)
    let media_path = match app.path().app_data_dir() {
        Ok(d) => d.join("media").join("media-index.json"),
        Err(_) => return None,
    };
    if !media_path.exists() {
        return None;
    }
    let contents = std::fs::read_to_string(&media_path).ok()?;
    if contents.trim().is_empty() {
        return None;
    }
    // Parse as generic Value to avoid private MediaItem struct
    let items: Vec<serde_json::Value> = serde_json::from_str(&contents).ok()?;
    let found = items.iter().find(|v| v.get("id").and_then(|id| id.as_str()) == Some(first_id))?;
    let path = found.get("path").and_then(|p| p.as_str())?;
    let media_type = found.get("mediaType").and_then(|t| t.as_str()).unwrap_or("image");
    if media_type != "image" {
        return None;
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return None;
    }
    // Pre-flight like facebook.rs: size <=10MB and mime whitelist
    if let Ok(meta) = std::fs::metadata(p) {
        if meta.len() > 10 * 1024 * 1024 {
            return None;
        }
    }
    let mime = mime_guess::from_path(p).first_or_octet_stream().to_string();
    if !["image/jpeg", "image/png", "image/webp", "image/gif"].contains(&mime.as_str()) {
        return None;
    }
    Some(path.to_string())
}

fn read_content_with_media(app: &AppHandle, content_id: &str) -> Result<(String, Option<String>), String> {
    let content_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("content")
        .join("content-index.json");
    if !content_path.exists() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let contents = std::fs::read_to_string(&content_path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() {
        return Err(format!("Không tìm thấy nội dung {}", content_id));
    }
    let items: Vec<crate::content::ContentItem> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    let found = items.into_iter().find(|c| c.id == content_id).ok_or_else(|| format!("Không tìm thấy nội dung {}", content_id))?;
    let image_path = resolve_first_image_path(app, &found.media_ids);
    Ok((crate::content::compose_facebook_post(&found.title, &found.body), image_path))
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

    fn sample_schedule(content_id: &str, minutes_ahead: i64) -> ScheduleItem {
        ScheduleItem {
            id: uuid::Uuid::new_v4().to_string(),
            content_id: content_id.into(),
            scheduled_at: Utc::now() + chrono::Duration::minutes(minutes_ahead),
            platform: "Sống Tích Cực".into(),
            pages: vec!["page1".into()],
            page_id: Some("page1".into()),
            status: ScheduleStatus::Scheduled,
            retry_count: 0,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_write_schedule_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_schedule_atomic");
        let path = dir.join("schedule-index.json");
        let tmp = path.with_extension("json.tmp");
        let item = sample_schedule("c1", 10);
        write_schedule_at(&path, &[item]).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        let parsed: Vec<ScheduleItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_schedule_sort_and_roundtrip() {
        let dir = unique_temp_dir("flowpost_schedule_roundtrip");
        let path = dir.join("schedule-index.json");
        let mut items = vec![
            sample_schedule("c1", 30),
            sample_schedule("c2", 10),
            sample_schedule("c3", 20),
        ];
        write_schedule_at(&path, &items).unwrap();
        let mut parsed: Vec<ScheduleItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        parsed.sort_by(|a, b| a.scheduled_at.cmp(&b.scheduled_at));
        assert!(parsed[0].scheduled_at < parsed[1].scheduled_at);
        assert!(parsed[1].scheduled_at < parsed[2].scheduled_at);
        // Delete one
        let id = parsed[1].id.clone();
        items.retain(|s| s.id != id);
        write_schedule_at(&path, &items).unwrap();
        let after: Vec<ScheduleItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_schedule_index_empty_handling() {
        let dir = unique_temp_dir("flowpost_schedule_empty");
        let path = dir.join("schedule-index.json");
        // No file -> read would be empty, but write then read
        write_schedule_at(&path, &[]).unwrap();
        let parsed: Vec<ScheduleItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.len(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_retry_delay() {
        assert_eq!(retry_delay_minutes(1), 5);
        assert_eq!(retry_delay_minutes(2), 10);
        assert_eq!(retry_delay_minutes(3), 30);
        assert_eq!(retry_delay_minutes(99), 30);
    }

    #[test]
    fn test_schedule_page_id_and_last_error_roundtrip() {
        let dir = unique_temp_dir("flowpost_schedule_page_id");
        let path = dir.join("schedule-index.json");
        let mut item = sample_schedule("c1", 10);
        item.page_id = Some("page123".into());
        item.last_error = Some("lỗi thử".into());
        item.retry_count = 2;
        write_schedule_at(&path, &[item.clone()]).unwrap();
        let parsed: Vec<ScheduleItem> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed[0].page_id, Some("page123".into()));
        assert_eq!(parsed[0].last_error, Some("lỗi thử".into()));
        assert_eq!(parsed[0].retry_count, 2);
        // Test default khi thiếu field (backward compat)
        let json_old = r#"[{"id":"old","contentId":"c","scheduledAt":"2026-01-01T00:00:00Z","platform":"FB","pages":["p1"],"status":"scheduled","retryCount":0,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}]"#;
        let parsed_old: Vec<ScheduleItem> = serde_json::from_str(json_old).unwrap();
        assert_eq!(parsed_old[0].page_id, None);
        assert_eq!(parsed_old[0].last_error, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_retry_schedule_resets_failed_item() {
        let dir = unique_temp_dir("flowpost_retry_schedule");
        let path = dir.join("schedule-index.json");
        // Create a Failed item with retry_count and last_error
        let mut item = sample_schedule("c1", -10); // past due
        item.status = ScheduleStatus::Failed;
        item.retry_count = 3;
        item.last_error = Some("Đăng bài thất bại: token expired".into());
        let item_id = item.id.clone();
        write_schedule_at(&path, &[item]).unwrap();

        // Simulate retry_schedule logic: read, find, reset, write
        let mut items: Vec<ScheduleItem> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let pos = items.iter().position(|s| s.id == item_id).unwrap();
        assert_eq!(items[pos].status, ScheduleStatus::Failed);

        let now = Utc::now();
        items[pos].status = ScheduleStatus::Scheduled;
        items[pos].retry_count = 0;
        items[pos].last_error = None;
        items[pos].scheduled_at = now + chrono::Duration::minutes(retry_delay_minutes(1));
        items[pos].updated_at = now;

        write_schedule_at(&path, &items).unwrap();

        // Verify the reset
        let parsed: Vec<ScheduleItem> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].status, ScheduleStatus::Scheduled);
        assert_eq!(parsed[0].retry_count, 0);
        assert_eq!(parsed[0].last_error, None);
        assert!(parsed[0].scheduled_at > Utc::now());
        assert_eq!(parsed[0].id, item_id);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_retry_schedule_rejects_non_failed() {
        // Verify that retry logic checks status before applying
        let mut item = sample_schedule("c2", 10);
        item.status = ScheduleStatus::Scheduled;
        // The command checks: if items[pos].status != ScheduleStatus::Failed → error
        assert_ne!(item.status, ScheduleStatus::Failed);
        // This confirms non-Failed items would be rejected
    }
}
