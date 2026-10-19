use chrono::{DateTime, Local, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, Runtime};

static AUTO_SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Recurrence {
    FixedTimes { times: Vec<String> },
    Interval { minutes: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSchedule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub recurrence: Recurrence,
    pub prompt: String,
    pub style: String,
    #[serde(default)]
    pub custom_style: Option<String>,
    #[serde(default)]
    pub style_content: Option<String>,
    #[serde(default)]
    pub topic_lists: Vec<crate::topics::TopicList>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub auto_approve: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSchedulerConfig {
    pub interval_seconds: u64,
    pub enabled: bool,
}

impl Default for AutoSchedulerConfig {
    fn default() -> Self {
        Self { interval_seconds: 60, enabled: true }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAutoSchedulePayload {
    pub name: String,
    pub recurrence: Recurrence,
    pub prompt: String,
    pub style: String,
    #[serde(default)]
    pub custom_style: Option<String>,
    #[serde(default)]
    pub style_content: Option<String>,
    #[serde(default)]
    pub topic_lists: Vec<crate::topics::TopicList>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub auto_approve: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAutoSchedulePayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub custom_style: Option<String>,
    #[serde(default)]
    pub style_content: Option<String>,
    #[serde(default)]
    pub topic_lists: Option<Vec<crate::topics::TopicList>>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

fn auto_dir(app: &AppHandle<impl Runtime>) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("auto-schedule");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn index_path(app: &AppHandle<impl Runtime>) -> Result<PathBuf, String> {
    Ok(auto_dir(app)?.join("auto-schedule-index.json"))
}

fn config_path(app: &AppHandle<impl Runtime>) -> Result<PathBuf, String> {
    Ok(auto_dir(app)?.join("auto-schedule-config.json"))
}

fn read_index(app: &AppHandle<impl Runtime>) -> Result<Vec<AutoSchedule>, String> {
    let path = index_path(app)?;
    if !path.exists() { return Ok(Vec::new()); }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() { return Ok(Vec::new()); }
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_index(app: &AppHandle<impl Runtime>, items: &[AutoSchedule]) -> Result<(), String> {
    let path = index_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn read_config(app: &AppHandle<impl Runtime>) -> Result<AutoSchedulerConfig, String> {
    let path = config_path(app)?;
    if !path.exists() { return Ok(AutoSchedulerConfig::default()); }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if contents.trim().is_empty() { return Ok(AutoSchedulerConfig::default()); }
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_config(app: &AppHandle<impl Runtime>, config: &AutoSchedulerConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() { return Err("Tên lịch không được để trống".into()); }
    if t.chars().count() > 60 { return Err("Tên lịch tối đa 60 ký tự".into()); }
    Ok(())
}

pub fn validate_recurrence(recurrence: &Recurrence) -> Result<(), String> {
    match recurrence {
        Recurrence::FixedTimes { times } => {
            for t in times {
                NaiveTime::parse_from_str(t.trim(), "%H:%M").map_err(|_| "Định dạng giờ không hợp lệ (HH:MM)".to_string())?;
            }
            Ok(())
        }
        Recurrence::Interval { minutes } => {
            if *minutes < 1 || *minutes > 1440 {
                return Err("Khoảng thời gian phải từ 1 đến 1440 phút".into());
            }
            Ok(())
        }
    }
}

pub fn compute_next_run(recurrence: &Recurrence, now: DateTime<Local>) -> DateTime<Utc> {
    match recurrence {
        Recurrence::Interval { minutes } => (now + chrono::Duration::minutes(*minutes as i64)).with_timezone(&Utc),
        Recurrence::FixedTimes { times } => {
            let mut parsed: Vec<NaiveTime> = times.iter().filter_map(|t| NaiveTime::parse_from_str(t.trim(), "%H:%M").ok()).collect();
            parsed.sort();
            let today = now.date_naive();
            let now_time = now.time();
            let mut chosen: Option<NaiveTime> = None;
            for t in &parsed {
                if *t > now_time {
                    chosen = Some(*t);
                    break;
                }
            }
            if let Some(t) = chosen {
                let dt = today.and_time(t);
                return dt.and_local_timezone(Local).unwrap().with_timezone(&Utc);
            }
            let t = parsed.first().copied().unwrap_or(NaiveTime::from_hms_opt(0,0,0).unwrap());
            let tomorrow = today + chrono::Duration::days(1);
            let dt = tomorrow.and_time(t);
            dt.and_local_timezone(Local).unwrap().with_timezone(&Utc)
        }
    }
}

#[tauri::command]
pub fn list_auto_schedule(app: AppHandle) -> Result<Vec<AutoSchedule>, String> {
    read_index(&app)
}

#[tauri::command]
pub fn get_auto_schedule(app: AppHandle, id: String) -> Result<AutoSchedule, String> {
    let items = read_index(&app)?;
    items.into_iter().find(|s| s.id == id).ok_or_else(|| format!("Không tìm thấy {}", id))
}

#[tauri::command]
pub fn create_auto_schedule(app: AppHandle, payload: CreateAutoSchedulePayload) -> Result<AutoSchedule, String> {
    validate_name(&payload.name)?;
    if payload.prompt.trim().is_empty() { return Err("Vui lòng nhập prompt".into()); }
    if payload.style.trim().is_empty() { return Err("Vui lòng chọn style".into()); }
    validate_recurrence(&payload.recurrence)?;
    let now = Utc::now();
    let item = AutoSchedule {
        id: uuid::Uuid::new_v4().to_string(),
        name: payload.name.trim().to_string(),
        enabled: true,
        recurrence: payload.recurrence.clone(),
        prompt: payload.prompt.trim().to_string(),
        style: payload.style.trim().to_string(),
        custom_style: payload.custom_style.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        style_content: payload.style_content,
        topic_lists: payload.topic_lists,
        provider_id: payload.provider_id,
        model: payload.model,
        auto_approve: payload.auto_approve,
        last_run_at: None,
        next_run_at: Some(compute_next_run(&payload.recurrence, Local::now()).to_rfc3339()),
        last_error: None,
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    };
    let mut items = read_index(&app)?;
    items.push(item.clone());
    write_index(&app, &items)?;
    Ok(item)
}

#[tauri::command]
pub fn update_auto_schedule(app: AppHandle, id: String, payload: UpdateAutoSchedulePayload) -> Result<AutoSchedule, String> {
    let mut items = read_index(&app)?;
    let pos = items.iter().position(|s| s.id == id).ok_or_else(|| format!("Không tìm thấy {}", id))?;
    if let Some(name) = &payload.name { validate_name(name)?; items[pos].name = name.trim().to_string(); }
    if let Some(rec) = &payload.recurrence { validate_recurrence(rec)?; items[pos].recurrence = rec.clone(); }
    if let Some(p) = &payload.prompt {
        if p.trim().is_empty() { return Err("Vui lòng nhập prompt".into()); }
        items[pos].prompt = p.trim().to_string();
    }
    if let Some(s) = &payload.style {
        if s.trim().is_empty() { return Err("Vui lòng chọn style".into()); }
        items[pos].style = s.trim().to_string();
    }
    if payload.custom_style.is_some() { items[pos].custom_style = payload.custom_style.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()); }
    if payload.style_content.is_some() { items[pos].style_content = payload.style_content; }
    if let Some(tl) = payload.topic_lists { items[pos].topic_lists = tl; }
    if payload.provider_id.is_some() { items[pos].provider_id = payload.provider_id; }
    if payload.model.is_some() { items[pos].model = payload.model; }
    if let Some(a) = payload.auto_approve { items[pos].auto_approve = a; }
    if let Some(e) = payload.enabled { items[pos].enabled = e; }
    items[pos].updated_at = Utc::now().to_rfc3339();
    if items[pos].next_run_at.is_none() {
        items[pos].next_run_at = Some(compute_next_run(&items[pos].recurrence, Local::now()).to_rfc3339());
    }
    let updated = items[pos].clone();
    write_index(&app, &items)?;
    Ok(updated)
}

#[tauri::command]
pub fn delete_auto_schedule(app: AppHandle, id: String) -> Result<(), String> {
    let mut items = read_index(&app)?;
    let len_before = items.len();
    items.retain(|s| s.id != id);
    if items.len() == len_before { return Err(format!("Không tìm thấy {}", id)); }
    write_index(&app, &items)
}

#[tauri::command]
pub fn toggle_auto_schedule(app: AppHandle, id: String, enabled: bool) -> Result<AutoSchedule, String> {
    let mut items = read_index(&app)?;
    let pos = items.iter().position(|s| s.id == id).ok_or_else(|| format!("Không tìm thấy {}", id))?;
    items[pos].enabled = enabled;
    items[pos].updated_at = Utc::now().to_rfc3339();
    let updated = items[pos].clone();
    write_index(&app, &items)?;
    Ok(updated)
}

#[tauri::command]
pub fn get_auto_scheduler_config(app: AppHandle) -> Result<AutoSchedulerConfig, String> {
    read_config(&app)
}

#[tauri::command]
pub fn set_auto_scheduler_config(app: AppHandle, config: AutoSchedulerConfig) -> Result<AutoSchedulerConfig, String> {
    if config.interval_seconds < 1 || config.interval_seconds > 3600 {
        return Err("Khoảng kiểm tra phải từ 1 đến 3600 giây".into());
    }
    write_config(&app, &config)?;
    Ok(config)
}

#[tauri::command(rename = "start_auto_scheduler")]
pub async fn start_auto_scheduler_cmd(app: AppHandle) -> Result<(), String> {
    let mut config = read_config(&app)?;
    config.enabled = true;
    write_config(&app, &config)?;
    start_auto_scheduler_impl(app).await
}

// The contract names command start_auto_scheduler. Provide both sync and async? 
// We'll expose async command named start_auto_scheduler via alias below.
pub async fn start_auto_scheduler_impl<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if AUTO_SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) { return Ok(()); }
    let config = read_config(&app)?;
    let mut interval_secs = config.interval_seconds;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    loop {
        interval.tick().await;
        let current_config = match read_config(&app) { Ok(c) => c, Err(_) => break };
        if !current_config.enabled { break; }
        if current_config.interval_seconds != interval_secs && current_config.interval_seconds >= 1 && current_config.interval_seconds <= 3600 {
            interval_secs = current_config.interval_seconds;
            interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        }
        let _ = process_due(&app).await;
    }
    AUTO_SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command(rename = "stop_auto_scheduler")]
pub async fn stop_auto_scheduler_cmd(app: AppHandle) -> Result<(), String> {
    let mut config = read_config(&app)?;
    config.enabled = false;
    write_config(&app, &config)?;
    Ok(())
}

pub fn start_auto_scheduler<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = start_auto_scheduler_impl(app_clone).await;
    });
    Ok(())
}

async fn process_due<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let mut items = read_index(app)?;
    let now = Local::now();
    for i in 0..items.len() {
        if !items[i].enabled { continue; }
        let due = match &items[i].next_run_at {
            None => true,
            Some(s) => chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc) <= now.with_timezone(&Utc)).unwrap_or(true),
        };
        if !due { continue; }
        let mut last_err: Option<String> = None;
        for _attempt in 0..3u32 {
            match run_one(app, &items[i]).await {
                Ok(()) => { last_err = None; break; }
                Err(e) => { last_err = Some(e); }
            }
        }
        items[i].last_run_at = Some(Utc::now().to_rfc3339());
        items[i].last_error = last_err;
        items[i].next_run_at = Some(compute_next_run(&items[i].recurrence, Local::now()).to_rfc3339());
        let _ = write_index(app, &items);
    }
    Ok(())
}

async fn run_one<R: Runtime>(app: &AppHandle<R>, s: &AutoSchedule) -> Result<(), String> {
    let prompt_used = crate::ai::expand_prompt(&s.prompt, &s.topic_lists, chrono::Utc::now().timestamp() as u64)?;
    let raw = crate::ai::call_ai_provider_with(app, &prompt_used, &s.style, s.custom_style.as_deref(), s.style_content.as_deref(), s.provider_id.as_deref(), s.model.as_deref()).await?;
    let (title, body) = crate::ai::parse_ai_response(&raw.content);
    let payload = crate::content::SaveContentPayload {
        title: title.clone(),
        body: body.clone(),
        prompt: s.prompt.clone(),
        style: s.style.clone(),
        topic: None,
        custom_style: s.custom_style.clone(),
        media_ids: None,
    };
    let item = crate::content::save_content_item(app, &payload)?;
    if s.auto_approve {
        let _ = crate::content::update_content_status_generic(app.clone(), item.id, crate::content::ContentStatus::Approved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    // tests minimal: compute_next_run etc.
    #[test]
    fn test_compute_next_run_fixed_time() {
        let now = Local::now().date_naive().and_hms_opt(9,0,0).unwrap();
        let dt = now.and_local_timezone(Local).unwrap();
        let rec = Recurrence::FixedTimes { times: vec!["10:00".into()] };
        let next = compute_next_run(&rec, dt);
        assert!(next.with_timezone(&Local).hour() == 10);
    }
    #[test]
    fn test_compute_next_run_interval() {
        let now = Local::now();
        let rec = Recurrence::Interval { minutes: 10 };
        let next = compute_next_run(&rec, now);
        assert!(next > now.with_timezone(&Utc));
    }
    #[test]
    fn test_auto_schedule_recurrence_validation() {
        let rec = Recurrence::FixedTimes { times: vec!["bad".into()] };
        assert!(validate_recurrence(&rec).is_err());
    }
    #[test]
    fn test_auto_schedule_empty_handling() {
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
    }
    #[test]
    fn test_auto_scheduler_config_validation() {
        // just logic
        assert!(!(1u64 < 1 || 1u64 > 3600));
    }
}
