use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, Runtime};

mod ai;
mod auto_schedule;
mod content;
mod facebook;
mod image_gen;
mod image_styles;
mod providers;
mod schedule;
mod security;
mod styles;
mod topics;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) media_type: String,
    pub(crate) extension: String,
    pub(crate) size: u64,
    pub(crate) imported_at: DateTime<Utc>,
}

fn media_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("media");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory)
}

fn index_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(media_dir(app)?.join("media-index.json"))
}

fn read_index<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<MediaItem>, String> {
    let path = index_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_index<R: Runtime>(app: &AppHandle<R>, items: &[MediaItem]) -> Result<(), String> {
    let path = index_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    // Atomic rename within same directory/filesystem keeps previous index intact on failure
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
fn write_index_at(path: &Path, items: &[MediaItem]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn import_from_paths(
    directory: &Path,
    index: &mut Vec<MediaItem>,
    sources: Vec<PathBuf>,
) -> Result<Vec<MediaItem>, String> {
    let mut imported = Vec::new();
    let mut copied: Vec<PathBuf> = Vec::new();
    let original_len = index.len();
    for source in sources {
        let extension = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let original_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("media")
            .to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let target = directory.join(format!("{}.{}", id, extension));
        if let Err(e) = fs::copy(&source, &target) {
            for p in &copied {
                let _ = fs::remove_file(p);
            }
            index.truncate(original_len);
            return Err(format!("Không thể sao chép {}: {}", original_name, e));
        }
        copied.push(target.clone());
        let size = match fs::metadata(&target) {
            Ok(m) => m.len(),
            Err(e) => {
                for p in &copied {
                    let _ = fs::remove_file(p);
                }
                index.truncate(original_len);
                return Err(e.to_string());
            }
        };
        let item = MediaItem {
            id,
            name: original_name,
            path: target.to_string_lossy().to_string(),
            media_type: kind_for(&target).into(),
            extension,
            size,
            imported_at: Utc::now(),
        };
        imported.push(item.clone());
        index.push(item);
    }
    Ok(imported)
}

fn kind_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mp4" | "mov" | "webm" | "mkv" => "video",
        _ => "image",
    }
}

fn register_bytes_at(
    directory: &Path,
    index: &mut Vec<MediaItem>,
    bytes: &[u8],
    name: &str,
    extension: &str,
    media_type: &str,
) -> Result<MediaItem, String> {
    let ext = extension.trim().trim_start_matches('.').to_lowercase();
    if ext.is_empty() {
        return Err("Phần mở rộng ảnh không hợp lệ".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let target = directory.join(format!("{}.{}", id, ext));
    if let Err(e) = fs::write(&target, bytes) {
        return Err(format!("Không thể ghi ảnh: {}", e));
    }
    let size = match fs::metadata(&target) {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            let _ = fs::remove_file(&target);
            return Err(e.to_string());
        }
    };
    let item = MediaItem {
        id,
        name: name.to_string(),
        path: target.to_string_lossy().to_string(),
        media_type: media_type.to_string(),
        extension: ext,
        size,
        imported_at: Utc::now(),
    };
    index.push(item.clone());
    Ok(item)
}

pub(crate) fn register_media_bytes<R: Runtime>(
    app: &AppHandle<R>,
    bytes: &[u8],
    name: &str,
    extension: &str,
    media_type: &str,
) -> Result<MediaItem, String> {
    let directory = media_dir(app)?;
    let mut index = read_index(app)?;
    let item = register_bytes_at(&directory, &mut index, bytes, name, extension, media_type)?;
    if let Err(e) = write_index(app, &index) {
        let _ = fs::remove_file(&item.path);
        return Err(e);
    }
    Ok(item)
}

#[tauri::command]
fn list_media(app: AppHandle) -> Result<Vec<MediaItem>, String> {
    let mut items = read_index(&app)?;
    items.retain(|item| Path::new(&item.path).exists());
    items.sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
    Ok(items)
}

#[tauri::command]
fn import_media(app: AppHandle) -> Result<Vec<MediaItem>, String> {
    let selected = rfd::FileDialog::new()
        .set_title("Chọn ảnh hoặc video cho FlowPost AI")
        .add_filter("Ảnh", &["jpg", "jpeg", "png", "webp", "gif"])
        .add_filter("Video", &["mp4", "mov", "webm", "mkv"])
        .pick_files()
        .unwrap_or_default();
    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let directory = media_dir(&app)?;
    let mut index = read_index(&app)?;
    let imported = import_from_paths(&directory, &mut index, selected).map_err(|e| {
        // Ensure no partial index is persisted if batch fails — rollback is handled inside import_from_paths;
        // we just propagate the error without writing.
        e
    })?;
    write_index(&app, &index)?;
    Ok(imported)
}

#[tauri::command]
fn delete_media(app: AppHandle, id: String) -> Result<(), String> {
    let mut items = read_index(&app)?;
    if let Some(item) = items.iter().find(|item| item.id == id) {
        let path = PathBuf::from(&item.path);
        if path.starts_with(media_dir(&app)?) && path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    items.retain(|item| item.id != id);
    write_index(&app, &items)
}

#[tauri::command]
fn media_location(app: AppHandle) -> Result<String, String> {
    Ok(media_dir(&app)?.to_string_lossy().to_string())
}

pub(crate) const REPORT_MAX_CSV_BYTES: usize = 32 * 1024 * 1024;

/// Tên tệp CSV an toàn cho Windows: bỏ ký tự cấm/điều khiển, ép đuôi `.csv`,
/// giới hạn 120 ký tự và tránh tên thiết bị dành riêng (CON, PRN, AUX, ...).
fn sanitize_report_csv_name(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches(|c| c == '.' || c == ' ');
    let base = if trimmed.to_ascii_lowercase().ends_with(".csv") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    let mut base = base.trim().trim_matches(|c| c == '.' || c == ' ').to_string();
    if base.is_empty() {
        base = "bao-cao".to_string();
    }
    let stem = base
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        base = format!("bao-cao-{}", base);
    }
    // 120 ký tự bao gồm cả đuôi `.csv`.
    let base: String = base.chars().take(116).collect();
    let base = base.trim_end_matches(|c| c == '.' || c == ' ');
    if base.is_empty() {
        "bao-cao.csv".to_string()
    } else {
        format!("{}.csv", base)
    }
}

fn validate_report_csv_contents(contents: &str) -> Result<(), String> {
    if contents.is_empty() {
        return Err("Nội dung CSV rỗng.".to_string());
    }
    if contents.len() > REPORT_MAX_CSV_BYTES {
        return Err("Tệp CSV quá lớn (tối đa 32 MB).".to_string());
    }
    Ok(())
}

fn write_csv_file(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents.as_bytes())
        .map_err(|e| format!("Không thể lưu tệp CSV: {}", e))
}

#[tauri::command]
fn save_report_csv(default_name: String, contents: String) -> Result<Option<String>, String> {
    validate_report_csv_contents(&contents)?;
    let name = sanitize_report_csv_name(&default_name);
    let Some(path) = rfd::FileDialog::new()
        .set_title("Lưu báo cáo CSV")
        .set_file_name(&name)
        .add_filter("CSV", &["csv"])
        .save_file()
    else {
        return Ok(None);
    };
    write_csv_file(&path, &contents)?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_write_index_atomic_creates_and_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_write_atomic");
        let index_path = dir.join("media-index.json");
        let tmp_path = index_path.with_extension("json.tmp");

        // First write
        let item1 = MediaItem {
            id: "id1".into(),
            name: "a.jpg".into(),
            path: dir.join("a.jpg").to_string_lossy().to_string(),
            media_type: "image".into(),
            extension: "jpg".into(),
            size: 123,
            imported_at: Utc::now(),
        };
        write_index_at(&index_path, &[item1.clone()]).unwrap();
        assert!(index_path.exists());
        assert!(!tmp_path.exists(), "tmp should be cleaned after rename");
        let contents = fs::read_to_string(&index_path).unwrap();
        let parsed: Vec<MediaItem> = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "id1");

        // Second write overwrites atomically
        let item2 = MediaItem {
            id: "id2".into(),
            name: "b.mp4".into(),
            path: dir.join("b.mp4").to_string_lossy().to_string(),
            media_type: "video".into(),
            extension: "mp4".into(),
            size: 456,
            imported_at: Utc::now(),
        };
        write_index_at(&index_path, &[item1, item2]).unwrap();
        assert!(!tmp_path.exists());
        let contents2 = fs::read_to_string(&index_path).unwrap();
        let parsed2: Vec<MediaItem> = serde_json::from_str(&contents2).unwrap();
        assert_eq!(parsed2.len(), 2);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_index_atomic_preserves_previous_on_tmp_failure() {
        // Simulate that previous index is preserved if we fail before rename.
        // We do this by manually creating tmp and verifying index not overwritten until rename.
        let dir = unique_temp_dir("flowpost_write_preserve");
        let index_path = dir.join("media-index.json");
        let item1 = MediaItem {
            id: "id1".into(),
            name: "a.jpg".into(),
            path: "a.jpg".into(),
            media_type: "image".into(),
            extension: "jpg".into(),
            size: 1,
            imported_at: Utc::now(),
        };
        write_index_at(&index_path, &[item1.clone()]).unwrap();
        let original = fs::read_to_string(&index_path).unwrap();

        // Simulate failure: write invalid tmp but don't rename
        let tmp = index_path.with_extension("json.tmp");
        fs::write(&tmp, "invalid json that will not be renamed").unwrap();
        // Index should still be original
        let still = fs::read_to_string(&index_path).unwrap();
        assert_eq!(original, still);
        // Cleanup and successful rewrite should still work
        fs::remove_file(&tmp).unwrap();
        let item2 = MediaItem {
            id: "id2".into(),
            name: "b.png".into(),
            path: "b.png".into(),
            media_type: "image".into(),
            extension: "png".into(),
            size: 2,
            imported_at: Utc::now(),
        };
        write_index_at(&index_path, &[item1, item2]).unwrap();
        let parsed: Vec<MediaItem> = serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
        assert_eq!(parsed.len(), 2);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_import_from_paths_rollback_on_failure() {
        let dir = unique_temp_dir("flowpost_import_rollback");
        let src_dir = dir.join("sources");
        fs::create_dir_all(&src_dir).unwrap();
        let target_dir = dir.join("media");
        fs::create_dir_all(&target_dir).unwrap();

        // Create 2 real source files
        let src1 = src_dir.join("photo.jpg");
        let src2 = src_dir.join("video.mp4");
        fs::write(&src1, b"fake jpg content").unwrap();
        fs::write(&src2, b"fake mp4 content").unwrap();
        // Third source does not exist -> will trigger copy error
        let src3 = src_dir.join("missing.png");

        let mut index: Vec<MediaItem> = Vec::new();
        let result = import_from_paths(&target_dir, &mut index, vec![src1.clone(), src2.clone(), src3]);

        assert!(result.is_err(), "should fail on missing source");
        assert!(
            result.unwrap_err().contains("Không thể sao chép"),
            "error should be Vietnamese copy message"
        );
        // Rollback: no files should remain in target_dir
        let remaining: Vec<_> = fs::read_dir(&target_dir).unwrap().collect();
        assert!(
            remaining.is_empty(),
            "target dir should be empty after rollback, found {:?}",
            remaining
        );
        // Index should remain empty (no partial push without write)
        assert!(index.is_empty(), "index should remain empty after rollback");

        // Happy path after rollback should still work
        let src3_ok = src_dir.join("ok.png");
        fs::write(&src3_ok, b"ok content").unwrap();
        let mut index2: Vec<MediaItem> = Vec::new();
        let ok = import_from_paths(&target_dir, &mut index2, vec![src1, src2, src3_ok]).unwrap();
        assert_eq!(ok.len(), 3);
        assert_eq!(index2.len(), 3);
        let files: Vec<_> = fs::read_dir(&target_dir).unwrap().collect();
        assert_eq!(files.len(), 3);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_import_from_paths_success_and_kind() {
        let dir = unique_temp_dir("flowpost_import_success");
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let target_dir = dir.join("media");
        fs::create_dir_all(&target_dir).unwrap();
        let src = src_dir.join("clip.Mp4");
        fs::write(&src, b"video data").unwrap();

        let mut index = Vec::new();
        let imported = import_from_paths(&target_dir, &mut index, vec![src]).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].media_type, "video");
        assert_eq!(imported[0].extension, "mp4");
        assert!(Path::new(&imported[0].path).exists());

        assert_eq!(kind_for(Path::new("photo.JPG")), "image");
        assert_eq!(kind_for(Path::new("movie.MKV")), "video");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn register_bytes_at_writes_file_and_index_row() {
        let dir = unique_temp_dir("flowpost_register_bytes");
        let bytes = b"\x89PNG\r\n\x1a\nfake-image-bytes";
        let mut index: Vec<MediaItem> = Vec::new();
        let item =
            register_bytes_at(&dir, &mut index, bytes, "Ảnh AI.png", "png", "image").unwrap();

        assert_eq!(index.len(), 1);
        assert_eq!(item.name, "Ảnh AI.png");
        assert_eq!(item.extension, "png");
        assert_eq!(item.media_type, "image");
        let path = PathBuf::from(&item.path);
        assert!(path.exists(), "file must be written");
        assert!(path.starts_with(&dir), "path must live inside the media dir");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(item.size, bytes.len() as u64);
        assert_eq!(index[0].id, item.id);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn register_bytes_at_rejects_empty_extension() {
        let dir = unique_temp_dir("flowpost_register_empty_ext");
        let mut index: Vec<MediaItem> = Vec::new();
        let err = register_bytes_at(&dir, &mut index, b"x", "n", "  .  ", "image").unwrap_err();
        assert_eq!(err, "Phần mở rộng ảnh không hợp lệ");
        assert!(index.is_empty(), "index must stay untouched on error");
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert!(entries.is_empty(), "no file should be written");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn register_bytes_at_normalizes_dot_and_case() {
        let dir = unique_temp_dir("flowpost_register_ext_case");
        let mut index: Vec<MediaItem> = Vec::new();
        let item = register_bytes_at(&dir, &mut index, b"x", "n", ".PNG", "image").unwrap();
        assert_eq!(item.extension, "png");
        assert!(item.path.ends_with(".png"), "path={}", item.path);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sanitize_report_csv_name_strips_paths_and_reserved_names() {
        let path_name = sanitize_report_csv_name("..\\..\\CON.txt");
        assert!(!path_name.contains('\\'), "{}", path_name);
        assert!(!path_name.contains('/'), "{}", path_name);
        assert!(path_name.ends_with(".csv"), "{}", path_name);
        assert!(path_name.starts_with("bao-cao-"), "{}", path_name);

        assert_eq!(
            sanitize_report_csv_name("C:\\temp\\bao-cao.csv"),
            "Ctempbao-cao.csv"
        );
        assert_eq!(
            sanitize_report_csv_name("bao-cao-song-tich-cuc-7ngay-2026-10-23.csv"),
            "bao-cao-song-tich-cuc-7ngay-2026-10-23.csv"
        );
        assert_eq!(
            sanitize_report_csv_name("Báo cáo tháng 10"),
            "Báo cáo tháng 10.csv"
        );
        assert_eq!(sanitize_report_csv_name("   "), "bao-cao.csv");

        for reserved in ["CON", "PRN", "AUX", "NUL", "COM1", "LPT9", "con.csv"] {
            let name = sanitize_report_csv_name(reserved);
            assert!(name.starts_with("bao-cao-"), "{} -> {}", reserved, name);
            assert!(name.ends_with(".csv"), "{} -> {}", reserved, name);
        }

        let long = sanitize_report_csv_name(&"a".repeat(500));
        assert!(long.chars().count() <= 120, "{} ký tự", long.chars().count());
        assert!(long.ends_with(".csv"), "{}", long);
    }

    #[test]
    fn write_csv_file_preserves_bom_and_utf8() {
        let dir = unique_temp_dir("flowpost_csv_write");
        let path = dir.join("bao-cao.csv");
        let contents = "\u{FEFF}Trang,Tổng số bài\r\nSống Tích Cực,2\r\nNội dung \"trích dẫn\"\r\n";

        write_csv_file(&path, contents).unwrap();

        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes, contents.as_bytes());
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "thiếu UTF-8 BOM");
        assert_eq!(String::from_utf8(bytes).unwrap(), contents);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_report_csv_rejects_empty_and_oversize_contents() {
        assert_eq!(
            validate_report_csv_contents("").unwrap_err(),
            "Nội dung CSV rỗng."
        );
        let oversize = "a".repeat(REPORT_MAX_CSV_BYTES + 1);
        assert_eq!(
            validate_report_csv_contents(&oversize).unwrap_err(),
            "Tệp CSV quá lớn (tối đa 32 MB)."
        );
    }
}

pub fn run() {
    eprintln!("[FlowPost AI] ai.rs mojibake-repair {} active", ai::MOJIBAKE_REPAIR_VERSION);
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = schedule::start_scheduler(handle.clone()).await;
                let _ = auto_schedule::start_auto_scheduler(handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_media,
            import_media,
            save_report_csv,
            delete_media,
            media_location,
            security::set_facebook_token,
            security::delete_facebook_token,
            security::credential_status,
            providers::list_providers,
            providers::get_active_provider,
            providers::create_provider,
            providers::update_provider,
            providers::delete_provider,
            providers::fetch_provider_models,
            providers::set_active_provider,
            content::list_content,
            content::get_content,
            content::save_content,
            content::delete_content,
            content::update_content_status,
            content::update_content,
            ai::generate_content,
            styles::list_styles,
            styles::create_style,
            styles::update_style,
            styles::duplicate_style,
            styles::delete_style,
            styles::reset_style,
            image_styles::list_image_styles,
            image_styles::create_image_style,
            image_styles::update_image_style,
            image_styles::duplicate_image_style,
            image_styles::delete_image_style,
            image_styles::reset_image_style,
            image_gen::get_image_config,
            image_gen::set_image_config,
            image_gen::pick_image_folder,
            image_gen::generate_image,
            topics::list_topics,
            topics::create_topic,
            topics::update_topic,
            topics::delete_topic,
            schedule::list_schedule,
            schedule::get_schedule,
            schedule::create_schedule,
            schedule::delete_schedule,
            schedule::update_schedule_status,
            schedule::reschedule,
            schedule::retry_schedule,
            facebook::validate_facebook_token,
            facebook::list_facebook_pages,
            facebook::publish_content,
            facebook::list_page_posts,
            facebook::get_post_insights,
            facebook::get_page_report,
            schedule::get_scheduler_config,
            schedule::set_scheduler_config,
            schedule::select_scheduler_page,
            schedule::start_scheduler,
            schedule::stop_scheduler,
            auto_schedule::list_auto_schedule,
            auto_schedule::get_auto_schedule,
            auto_schedule::create_auto_schedule,
            auto_schedule::update_auto_schedule,
            auto_schedule::delete_auto_schedule,
            auto_schedule::toggle_auto_schedule,
            auto_schedule::get_auto_scheduler_config,
            auto_schedule::set_auto_scheduler_config,
            auto_schedule::start_auto_scheduler_cmd,
            auto_schedule::stop_auto_scheduler_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlowPost AI");
}
