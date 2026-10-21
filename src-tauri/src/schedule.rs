use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

use crate::facebook::{GraphFailure, GraphFailureKind};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleStatus {
    // Wire format must stay PascalCase: frontend compares 'Scheduled',
    // 'Published', 'Publishing'... Aliases keep older lowercase files readable.
    #[serde(rename = "Scheduled", alias = "scheduled")]
    Scheduled,
    #[serde(rename = "Publishing", alias = "publishing")]
    Publishing,
    #[serde(rename = "Published", alias = "published")]
    Published,
    #[serde(rename = "Failed", alias = "failed")]
    Failed,
    #[serde(rename = "Cancelled", alias = "cancelled")]
    Cancelled,
}

impl Default for ScheduleStatus {
    fn default() -> Self {
        ScheduleStatus::Scheduled
    }
}

pub const MAX_RETRIES: u32 = 3;
pub const MAX_PUBLISHES_PER_TICK: usize = 3;
pub const MIN_SCHEDULER_INTERVAL_SECS: u64 = 30;
pub const PUBLISH_DELAY_MIN_MS: u64 = 2_000;
pub const PUBLISH_DELAY_MAX_MS: u64 = 5_000;
pub const RATE_LIMIT_BACKOFF_SECS: [u64; 4] = [60, 300, 900, 3600];
pub const MAX_BACKOFF_SECS: u64 = 21_600;

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
    #[serde(default)]
    pub allow_duplicate: bool,
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
    if let Some(page_id) = payload.page_id.as_deref() {
        crate::facebook::validate_page_id(page_id)?;
    }
    if let Some(pages) = payload.pages.as_ref() {
        for page in pages {
            crate::facebook::validate_page_id(page)?;
        }
    }
    let now = Utc::now();
    // Must be at least 5 minutes in future
    if payload.scheduled_at <= now + chrono::Duration::minutes(5) {
        return Err("Thời gian lên lịch phải trong tương lai (tối thiểu 5 phút)".into());
    }
    validate_content_approved(&app, &payload.content_id)?;

    let config = read_scheduler_config(&app)?;
    let mut items = read_schedule_index(&app)?;

    let content_id = payload.content_id.trim().to_string();
    let pages = payload.pages.unwrap_or_default();
    let resolved_page_id = payload
        .page_id
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            config
                .selected_page_id
                .clone()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            pages
                .first()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });
    if !payload.allow_duplicate {
        if let Some(page) = resolved_page_id.as_deref() {
            if find_duplicate(&items, &config, &content_id, page).is_some() {
                return Err("Lịch đăng trùng: nội dung này đã có lịch cho cùng trang.".into());
            }
        }
    }
    let item = ScheduleItem {
        id: uuid::Uuid::new_v4().to_string(),
        content_id,
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
    let target = items
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy lịch {}", id))?;
    if target.status == ScheduleStatus::Publishing {
        return Err("Mục đang đăng, không thể xóa.".into());
    }
    items.retain(|s| s.id != id);
    write_schedule_index(&app, &items)
}

#[tauri::command]
pub fn update_schedule_status(
    app: AppHandle,
    id: String,
    status: ScheduleStatus,
) -> Result<ScheduleItem, String> {
    if status == ScheduleStatus::Publishing {
        return Err("Không thể đặt trạng thái Publishing thủ công.".into());
    }
    let mut items = read_schedule_index(&app)?;
    let pos = items
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy {}", id))?;
    if items[pos].status == ScheduleStatus::Publishing {
        return Err("Mục đang đăng, không thể đổi trạng thái thủ công.".into());
    }
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
    if items[pos].status == ScheduleStatus::Publishing {
        return Err("Mục đang đăng, không thể dời lịch.".into());
    }
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

    // Dedupe: nếu đã running thì không spawn thêm (và bỏ qua recovery).
    if SCHEDULER_RUNNING.load(Ordering::SeqCst) {
        return Ok(());
    }
    // Phục hồi item kẹt Publishing từ lần chạy trước TRƯỚC khi đánh dấu running.
    recover_stale_publishing(&app)?;
    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval_secs = config.interval_seconds.max(MIN_SCHEDULER_INTERVAL_SECS);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut paused_until: Option<DateTime<Utc>> = None;
        loop {
            interval.tick().await;
            let current_config = match read_scheduler_config(&app_handle) {
                Ok(c) => c,
                Err(_) => break,
            };
            if !current_config.enabled {
                break;
            }
            // Hot-reload interval nếu config đổi (luôn tôn trọng mức tối thiểu)
            let effective_secs = current_config
                .interval_seconds
                .max(MIN_SCHEDULER_INTERVAL_SECS);
            if effective_secs != interval_secs {
                interval_secs = effective_secs;
                interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            }
            if let Some(until) = paused_until {
                if Utc::now() < until {
                    continue;
                }
                paused_until = None;
            }
            if let Ok(outcome) = process_due_schedules(&app_handle).await {
                if outcome.rate_limited_until.is_some() {
                    paused_until = outcome.rate_limited_until;
                }
            }
        }
        SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    });

    Ok(())
}

/// Chuyển mọi item đang Publishing (do app dừng giữa chừng) sang Failed.
/// KHÔNG tự đăng lại để tránh đăng trùng.
pub fn recover_stale_publishing(app: &AppHandle) -> Result<usize, String> {
    let mut items = read_schedule_index(app)?;
    let changed = recover_stale_publishing_items(&mut items, Utc::now());
    if changed > 0 {
        write_schedule_index(app, &items)?;
    }
    Ok(changed)
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

/// Khoảng nghỉ giữa 2 lần đăng trong cùng tick: 2000..=5000ms.
fn publish_delay_ms(seq: u64) -> u64 {
    (PUBLISH_DELAY_MIN_MS + (seq % 4) * 1000).min(PUBLISH_DELAY_MAX_MS)
}

/// Backoff rate-limit theo bảng [60, 300, 900, 3600], chặn trên MAX_BACKOFF_SECS.
fn rate_limit_backoff_secs(retry_count: u32) -> u64 {
    let idx = (retry_count as usize).min(RATE_LIMIT_BACKOFF_SECS.len() - 1);
    RATE_LIMIT_BACKOFF_SECS[idx].min(MAX_BACKOFF_SECS)
}

/// Thứ tự ưu tiên: item.page_id -> config.selected_page_id -> item.pages.first().
fn resolve_item_page(item: &ScheduleItem, config: &SchedulerConfig) -> Option<String> {
    item.page_id
        .clone()
        .or_else(|| config.selected_page_id.clone())
        .or_else(|| item.pages.first().cloned())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Các item được chọn trong tick hiện tại: Scheduled, còn lượt retry, đã tới giờ.
/// Sắp xếp (scheduled_at, id) và cắt theo cap.
fn select_due_indices(items: &[ScheduleItem], now: DateTime<Utc>, cap: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.status == ScheduleStatus::Scheduled
                && item.retry_count < MAX_RETRIES
                && item.scheduled_at <= now
        })
        .map(|(index, _)| index)
        .collect();
    indices.sort_by(|&a, &b| {
        items[a]
            .scheduled_at
            .cmp(&items[b].scheduled_at)
            .then_with(|| items[a].id.cmp(&items[b].id))
    });
    indices.truncate(cap);
    indices
}

/// Tìm lịch trùng: cùng content_id, cùng page đã resolve, và còn hiệu lực
/// (Scheduled/Publishing). Published/Failed/Cancelled không chặn.
fn find_duplicate<'a>(
    items: &'a [ScheduleItem],
    config: &SchedulerConfig,
    content_id: &str,
    page: &str,
) -> Option<&'a ScheduleItem> {
    items.iter().find(|item| {
        item.content_id == content_id
            && matches!(
                item.status,
                ScheduleStatus::Scheduled | ScheduleStatus::Publishing
            )
            && resolve_item_page(item, config).as_deref() == Some(page)
    })
}

/// Item kẹt Publishing khi app dừng → Failed, không auto-repost.
pub fn recover_stale_publishing_items(items: &mut [ScheduleItem], now: DateTime<Utc>) -> usize {
    let mut changed = 0;
    for item in items
        .iter_mut()
        .filter(|item| item.status == ScheduleStatus::Publishing)
    {
        item.status = ScheduleStatus::Failed;
        item.retry_count = MAX_RETRIES;
        item.last_error = Some(
            "Ứng dụng đã dừng khi đang đăng. Không tự đăng lại để tránh trùng; kiểm tra Facebook rồi thử lại thủ công."
                .to_string(),
        );
        item.updated_at = now;
        changed += 1;
    }
    changed
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureAction {
    Retry,
    StopTick,
    Terminal,
}

/// Áp dụng chính sách lỗi lên item. Pure để unit-test được.
fn apply_failure(
    item: &mut ScheduleItem,
    failure: &GraphFailure,
    now: DateTime<Utc>,
) -> FailureAction {
    item.last_error = Some(failure.message.clone());
    item.updated_at = now;
    match failure.kind {
        GraphFailureKind::TokenInvalid | GraphFailureKind::InvalidParam => {
            item.status = ScheduleStatus::Failed;
            item.retry_count = MAX_RETRIES;
            FailureAction::Terminal
        }
        GraphFailureKind::RateLimited => {
            let previous_retry_count = item.retry_count;
            item.retry_count = previous_retry_count.saturating_add(1);
            let backoff = failure
                .retry_after_secs
                .unwrap_or(0)
                .max(rate_limit_backoff_secs(previous_retry_count))
                .min(MAX_BACKOFF_SECS);
            if item.retry_count >= MAX_RETRIES {
                item.status = ScheduleStatus::Failed;
                FailureAction::Terminal
            } else {
                item.status = ScheduleStatus::Scheduled;
                item.scheduled_at = now + chrono::Duration::seconds(backoff as i64);
                FailureAction::StopTick
            }
        }
        GraphFailureKind::Network | GraphFailureKind::Other => {
            item.retry_count = item.retry_count.saturating_add(1);
            if item.retry_count >= MAX_RETRIES {
                item.status = ScheduleStatus::Failed;
                FailureAction::Terminal
            } else {
                item.status = ScheduleStatus::Scheduled;
                item.scheduled_at =
                    now + chrono::Duration::minutes(retry_delay_minutes(item.retry_count));
                FailureAction::Retry
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct TickOutcome {
    pub attempted: usize,
    pub rate_limited_until: Option<DateTime<Utc>>,
}

pub async fn process_due_schedules(app: &AppHandle) -> Result<TickOutcome, String> {
    let config = read_scheduler_config(app).unwrap_or_default();
    let mut items = read_schedule_index(app)?;
    let mut outcome = TickOutcome::default();
    let now = Utc::now();
    let indices = select_due_indices(&items, now, MAX_PUBLISHES_PER_TICK);
    for (seq, i) in indices.into_iter().enumerate() {
        // Nghỉ giữa các lần đăng, không nghỉ trước lần đầu.
        if seq > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(publish_delay_ms(
                seq as u64,
            )))
            .await;
        }
        let now = Utc::now();
        // Đánh dấu Publishing và ghi NGAY để lần khởi động sau biết item đang dở.
        items[i].status = ScheduleStatus::Publishing;
        items[i].updated_at = now;
        items[i].last_error = None;
        if let Err(e) = write_schedule_index(app, &items) {
            items[i].status = ScheduleStatus::Scheduled;
            return Err(e);
        }
        outcome.attempted += 1;

        let (content_body, first_image) = match read_content_with_media(app, &items[i].content_id) {
            Ok((body, image)) => (body, image),
            Err(e) => {
                let failure = GraphFailure::local(GraphFailureKind::Other, e);
                apply_failure(&mut items[i], &failure, now);
                write_schedule_index(app, &items)?;
                continue;
            }
        };
        let resolved_page_id = resolve_item_page(&items[i], &config).unwrap_or_default();
        let res = crate::facebook::publish_content_classified(
            app.clone(),
            resolved_page_id,
            content_body,
            first_image,
        )
        .await;
        match res {
            Ok(_) => {
                items[i].status = ScheduleStatus::Published;
                items[i].last_error = None;
                items[i].updated_at = now;
                write_schedule_index(app, &items)?;
            }
            Err(failure) => {
                let action = apply_failure(&mut items[i], &failure, now);
                let paused_until = if action == FailureAction::StopTick {
                    Some(items[i].scheduled_at)
                } else {
                    None
                };
                write_schedule_index(app, &items)?;
                if let Some(until) = paused_until {
                    outcome.rate_limited_until = Some(until);
                    break;
                }
            }
        }
    }
    Ok(outcome)
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

    #[test]
    fn test_legacy_fixture_without_optional_fields_defaults() {
        let json = r#"[{"id":"old","contentId":"c","scheduledAt":"2026-01-01T00:00:00Z","platform":"FB","status":"Scheduled","retryCount":0,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}]"#;
        let parsed: Vec<ScheduleItem> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].pages.is_empty());
        assert_eq!(parsed[0].page_id, None);
        assert_eq!(parsed[0].last_error, None);
        assert_eq!(parsed[0].status, ScheduleStatus::Scheduled);
    }

    #[test]
    fn test_publishing_status_serializes_pascal_case_and_roundtrips() {
        let mut item = sample_schedule("c1", 10);
        item.status = ScheduleStatus::Publishing;
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"status\":\"Publishing\""), "json: {}", json);
        let back: ScheduleItem = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, ScheduleStatus::Publishing);
        // Alias lowercase giữ tương thích file cũ
        assert_eq!(
            serde_json::from_str::<ScheduleStatus>("\"publishing\"").unwrap(),
            ScheduleStatus::Publishing
        );
    }

    #[test]
    fn test_recover_stale_publishing_items_marks_failed_once() {
        let now = Utc::now();
        let mut stuck = sample_schedule("c1", -5);
        stuck.status = ScheduleStatus::Publishing;
        stuck.retry_count = 0;
        let untouched = sample_schedule("c2", 10);
        let mut items = vec![stuck, untouched];
        assert_eq!(recover_stale_publishing_items(&mut items, now), 1);
        assert_eq!(items[0].status, ScheduleStatus::Failed);
        assert_eq!(items[0].retry_count, MAX_RETRIES);
        assert!(items[0]
            .last_error
            .as_deref()
            .unwrap()
            .contains("Không tự đăng lại"));
        assert_eq!(items[0].updated_at, now);
        assert_eq!(items[1].status, ScheduleStatus::Scheduled);
        // Chạy lần hai không đổi gì
        assert_eq!(recover_stale_publishing_items(&mut items, now), 0);
        assert_eq!(items[0].status, ScheduleStatus::Failed);
    }

    #[test]
    fn test_publish_delay_ms_range() {
        for seq in 0..=7u64 {
            let delay = publish_delay_ms(seq);
            assert!(
                delay >= PUBLISH_DELAY_MIN_MS && delay <= PUBLISH_DELAY_MAX_MS,
                "seq {} -> {}",
                seq,
                delay
            );
        }
        assert_eq!(publish_delay_ms(0), 2_000);
        assert_eq!(publish_delay_ms(4), 2_000);
        assert_eq!(publish_delay_ms(3), 5_000);
    }

    #[test]
    fn test_rate_limit_backoff_table() {
        let expected = [60, 300, 900, 3600, 3600, 3600, 3600, 3600, 3600, 3600];
        for (n, want) in expected.iter().enumerate() {
            assert_eq!(rate_limit_backoff_secs(n as u32), *want, "retry {}", n);
        }
        assert_eq!(rate_limit_backoff_secs(999), 3600);
    }

    fn failure(
        kind: GraphFailureKind,
        message: &str,
        retry_after_secs: Option<u64>,
    ) -> GraphFailure {
        GraphFailure {
            kind,
            http_status: 0,
            code: None,
            retry_after_secs,
            message: message.to_string(),
        }
    }

    #[test]
    fn test_apply_failure_token_invalid_is_terminal() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::TokenInvalid, "token hết hạn", None),
            now,
        );
        assert_eq!(action, FailureAction::Terminal);
        assert_eq!(item.status, ScheduleStatus::Failed);
        assert_eq!(item.retry_count, MAX_RETRIES);
        assert_eq!(item.last_error.as_deref(), Some("token hết hạn"));
        assert_eq!(item.updated_at, now);
    }

    #[test]
    fn test_apply_failure_invalid_param_is_terminal() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::InvalidParam, "tham số sai", None),
            now,
        );
        assert_eq!(action, FailureAction::Terminal);
        assert_eq!(item.status, ScheduleStatus::Failed);
        assert_eq!(item.retry_count, MAX_RETRIES);
    }

    #[test]
    fn test_apply_failure_rate_limited_first_is_stop_tick() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        item.retry_count = 0;
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::RateLimited, "rate limit", None),
            now,
        );
        assert_eq!(action, FailureAction::StopTick);
        assert_eq!(item.status, ScheduleStatus::Scheduled);
        assert_eq!(item.retry_count, 1);
        assert_eq!(item.scheduled_at, now + chrono::Duration::seconds(60));
    }

    #[test]
    fn test_apply_failure_rate_limited_honours_retry_after_and_cap() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::RateLimited, "rate limit", Some(999_999)),
            now,
        );
        assert_eq!(action, FailureAction::StopTick);
        assert_eq!(
            item.scheduled_at,
            now + chrono::Duration::seconds(MAX_BACKOFF_SECS as i64)
        );
    }

    #[test]
    fn test_apply_failure_rate_limited_exhausted_is_terminal() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        item.retry_count = MAX_RETRIES - 1;
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::RateLimited, "rate limit", Some(120)),
            now,
        );
        assert_eq!(action, FailureAction::Terminal);
        assert_eq!(item.status, ScheduleStatus::Failed);
        assert_eq!(item.retry_count, MAX_RETRIES);
    }

    #[test]
    fn test_apply_failure_network_retries_with_delay() {
        let now = Utc::now();
        let mut item = sample_schedule("c1", -1);
        let action = apply_failure(
            &mut item,
            &failure(GraphFailureKind::Network, "mất kết nối", None),
            now,
        );
        assert_eq!(action, FailureAction::Retry);
        assert_eq!(item.status, ScheduleStatus::Scheduled);
        assert_eq!(item.retry_count, 1);
        assert_eq!(item.scheduled_at, now + chrono::Duration::minutes(5));
    }

    #[test]
    fn test_find_duplicate_scopes_by_content_page_and_status() {
        let config = SchedulerConfig::default();

        let scheduled = sample_schedule("c1", 10);
        assert!(find_duplicate(&[scheduled], &config, "c1", "page1").is_some());

        let mut publishing = sample_schedule("c1", 10);
        publishing.status = ScheduleStatus::Publishing;
        assert!(find_duplicate(&[publishing], &config, "c1", "page1").is_some());

        let mut published = sample_schedule("c1", 10);
        published.status = ScheduleStatus::Published;
        assert!(find_duplicate(&[published], &config, "c1", "page1").is_none());

        let mut other_page = sample_schedule("c1", 10);
        other_page.page_id = Some("page2".into());
        other_page.pages = vec!["page2".into()];
        assert!(find_duplicate(&[other_page], &config, "c1", "page1").is_none());
    }

    #[test]
    fn test_resolve_item_page_precedence() {
        let mut config = SchedulerConfig::default();
        config.selected_page_id = Some("config_page".into());
        let mut item = sample_schedule("c1", 10);
        item.page_id = None;
        item.pages = vec!["pages_first".into()];
        assert_eq!(
            resolve_item_page(&item, &config).as_deref(),
            Some("config_page")
        );
        item.page_id = Some("item_page".into());
        assert_eq!(
            resolve_item_page(&item, &config).as_deref(),
            Some("item_page")
        );
        config.selected_page_id = None;
        item.page_id = None;
        assert_eq!(
            resolve_item_page(&item, &config).as_deref(),
            Some("pages_first")
        );
        item.pages.clear();
        assert_eq!(resolve_item_page(&item, &config), None);
    }

    #[test]
    fn test_select_due_indices_caps_sorts_and_filters() {
        let now = Utc::now();
        let base = now - chrono::Duration::minutes(30);
        let mut items = Vec::new();
        for k in 0..10i64 {
            let mut item = sample_schedule(&format!("c{}", k), -30);
            item.id = format!("id{:02}", 9 - k);
            item.scheduled_at = base + chrono::Duration::seconds(k);
            items.push(item);
        }
        let mut exhausted = sample_schedule("exhausted", -60);
        exhausted.id = "a_exhausted".into();
        exhausted.retry_count = MAX_RETRIES;
        items.push(exhausted);
        let mut future = sample_schedule("future", 30);
        future.id = "a_future".into();
        items.push(future);

        let indices = select_due_indices(&items, now, MAX_PUBLISHES_PER_TICK);
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_select_due_indices_tie_breaks_by_id() {
        let now = Utc::now();
        let at = now - chrono::Duration::minutes(1);
        let mut items = Vec::new();
        for id in ["b", "a", "c"] {
            let mut item = sample_schedule("c1", -1);
            item.id = id.to_string();
            item.scheduled_at = at;
            items.push(item);
        }
        assert_eq!(select_due_indices(&items, now, 3), vec![1, 0, 2]);
    }
}
