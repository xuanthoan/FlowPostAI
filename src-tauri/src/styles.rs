use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub id: String,
    pub name: String,
    pub content: String,
    pub builtin: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StylesFile {
    styles: Vec<Style>,
}

/// Built-in styles with fixed ids so overrides/restores always match.
/// Tuples are `(id, name, content)`.
pub const DEFAULT_STYLES: &[(&str, &str, &str)] = &[
    (
        "viral",
        "Viral",
        "Viết theo phong cách viral mạng xã hội: mở đầu bằng một câu hook gây tò mò hoặc bất ngờ để giữ chân người đọc. Thân bài ngắn gọn, xuống dòng liên tục, mỗi ý một dòng, nhịp điệu nhanh và giàu cảm xúc. Kết bài bằng một câu hỏi hoặc lời kêu gọi bình luận, chia sẻ. Dùng ngôi thứ nhất hoặc thứ hai gần gũi, tránh câu văn dài dòng.",
    ),
    (
        "motivational",
        "Motivational",
        "Viết theo phong cách truyền cảm hứng: mở đầu bằng một nhận định tích cực hoặc câu chuyện rất ngắn tạo đồng cảm. Thân bài đưa ra hai hoặc ba thông điệp động viên, nhấn mạnh nỗ lực, kiên trì và khả năng thay đổi bản thân. Giọng văn ấm áp, chân thành, truyền năng lượng tích cực. Kết bài bằng một lời nhắn nhủ nhẹ nhàng hoặc lời mời người đọc hành động ngay hôm nay.",
    ),
    (
        "story",
        "Story",
        "Viết theo phong cách kể chuyện: mở đầu bằng một tình huống hoặc nhân vật cụ thể để dẫn người đọc vào mạch truyện. Thân bài diễn biến theo trình tự thời gian, có cao trào và cung bậc cảm xúc, dùng chi tiết đời thường. Giọng văn tự nhiên, giàu hình ảnh. Kết bài rút ra một bài học hoặc thông điệp ý nghĩa gắn với sản phẩm, thương hiệu.",
    ),
];

fn styles_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("styles.json"))
}

fn default_styles() -> StylesFile {
    StylesFile {
        styles: DEFAULT_STYLES
            .iter()
            .map(|(id, name, content)| Style {
                id: (*id).to_string(),
                name: (*name).to_string(),
                content: (*content).to_string(),
                builtin: true,
            })
            .collect(),
    }
}

/// Self-heal the on-disk styles list. Returns whether anything was modified.
/// (a) drop duplicate ids keeping the first; (b) force canonical entries `builtin = true`;
/// (c) append missing canonical built-ins; (d) reset a canonical entry whose name/content is blank.
fn normalize_styles(file: &mut StylesFile) -> bool {
    let mut changed = false;

    // (a) drop duplicate ids keeping the first occurrence
    let mut seen: Vec<String> = Vec::new();
    let before = file.styles.len();
    file.styles.retain(|s| {
        if seen.contains(&s.id) {
            false
        } else {
            seen.push(s.id.clone());
            true
        }
    });
    if file.styles.len() != before {
        changed = true;
    }

    // (b) + (d) canonical entries: force builtin, reset blank name/content to defaults
    for (cid, cname, ccontent) in DEFAULT_STYLES.iter().copied() {
        if let Some(s) = file.styles.iter_mut().find(|s| s.id == cid) {
            if !s.builtin {
                s.builtin = true;
                changed = true;
            }
            if s.name.trim().is_empty() || s.content.trim().is_empty() {
                if s.name != cname {
                    s.name = cname.to_string();
                    changed = true;
                }
                if s.content != ccontent {
                    s.content = ccontent.to_string();
                    changed = true;
                }
            }
        }
    }

    // (c) append any missing canonical built-in with its default name+content
    for (cid, cname, ccontent) in DEFAULT_STYLES.iter().copied() {
        if !file.styles.iter().any(|s| s.id == cid) {
            file.styles.push(Style {
                id: cid.to_string(),
                name: cname.to_string(),
                content: ccontent.to_string(),
                builtin: true,
            });
            changed = true;
        }
    }

    changed
}

fn validate_name(name: &str, existing: &[Style], exclude_id: Option<&str>) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tên style không được để trống".into());
    }
    if trimmed.chars().count() > 60 {
        return Err("Tên style tối đa 60 ký tự".into());
    }
    let lower = trimmed.to_lowercase();
    for s in existing {
        if Some(s.id.as_str()) == exclude_id {
            continue;
        }
        if s.name.trim().to_lowercase() == lower {
            return Err(format!("Tên style '{}' đã tồn tại", trimmed));
        }
    }
    Ok(())
}

fn validate_content(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Nội dung style không được để trống".into());
    }
    Ok(())
}

fn create_style_in(file: &mut StylesFile, name: &str, content: &str) -> Result<Style, String> {
    validate_name(name, &file.styles, None)?;
    validate_content(content)?;
    let style = Style {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        content: content.trim().to_string(),
        builtin: false,
    };
    file.styles.push(style.clone());
    Ok(style)
}

fn update_style_in(
    file: &mut StylesFile,
    id: &str,
    name: &str,
    content: &str,
) -> Result<Style, String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style '{}'", id))?;
    validate_name(name, &file.styles, Some(id))?;
    validate_content(content)?;
    file.styles[idx].name = name.trim().to_string();
    file.styles[idx].content = content.trim().to_string();
    Ok(file.styles[idx].clone())
}

fn duplicate_style_in(file: &mut StylesFile, id: &str) -> Result<Style, String> {
    let source = file
        .styles
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("Không tìm thấy style '{}'", id))?;

    let base = source.name.trim().to_string();
    let mut n: u32 = 1;
    let new_name = loop {
        let suffix = if n == 1 {
            " (bản sao)".to_string()
        } else {
            format!(" (bản sao {})", n)
        };
        let max_base = 60usize.saturating_sub(suffix.chars().count());
        let truncated: String = base.chars().take(max_base).collect();
        let candidate = format!("{}{}", truncated, suffix);
        if validate_name(&candidate, &file.styles, None).is_ok() {
            break candidate;
        }
        n += 1;
    };

    let style = Style {
        id: uuid::Uuid::new_v4().to_string(),
        name: new_name,
        content: source.content.clone(),
        builtin: false,
    };
    file.styles.push(style.clone());
    Ok(style)
}

fn delete_style_in(file: &mut StylesFile, id: &str) -> Result<(), String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style '{}'", id))?;
    if file.styles[idx].builtin {
        return Err("Không thể xóa style mặc định. Dùng \"Khôi phục mặc định\" để đặt lại.".into());
    }
    file.styles.remove(idx);
    Ok(())
}

fn reset_style_in(file: &mut StylesFile, id: &str) -> Result<Style, String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style '{}'", id))?;
    let (_, cname, ccontent) = DEFAULT_STYLES
        .iter()
        .copied()
        .find(|(cid, _, _)| *cid == id)
        .ok_or("Chỉ có thể khôi phục mặc định cho style mặc định")?;
    file.styles[idx].name = cname.to_string();
    file.styles[idx].content = ccontent.to_string();
    file.styles[idx].builtin = true;
    Ok(file.styles[idx].clone())
}

fn write_styles_at(path: &Path, file: &StylesFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_styles_at(path: &Path) -> Result<StylesFile, String> {
    let contents = if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    if contents.trim().is_empty() {
        let file = default_styles();
        write_styles_at(path, &file)?;
        return Ok(file);
    }
    let mut file: StylesFile = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    if normalize_styles(&mut file) {
        write_styles_at(path, &file)?;
    }
    Ok(file)
}

// --- Commands ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleInput {
    pub name: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStylePayload {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[tauri::command]
pub fn list_styles<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Style>, String> {
    let path = styles_path(&app)?;
    let file = read_styles_at(&path)?;
    Ok(file.styles)
}

#[tauri::command]
pub fn create_style<R: Runtime>(app: AppHandle<R>, payload: StyleInput) -> Result<Style, String> {
    let path = styles_path(&app)?;
    let mut file = read_styles_at(&path)?;
    let style = create_style_in(&mut file, &payload.name, &payload.content)?;
    write_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn update_style<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateStylePayload,
) -> Result<Style, String> {
    let path = styles_path(&app)?;
    let mut file = read_styles_at(&path)?;
    let style = update_style_in(&mut file, &payload.id, &payload.name, &payload.content)?;
    write_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn duplicate_style<R: Runtime>(app: AppHandle<R>, id: String) -> Result<Style, String> {
    let path = styles_path(&app)?;
    let mut file = read_styles_at(&path)?;
    let style = duplicate_style_in(&mut file, &id)?;
    write_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn delete_style<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    let path = styles_path(&app)?;
    let mut file = read_styles_at(&path)?;
    delete_style_in(&mut file, &id)?;
    write_styles_at(&path, &file)?;
    Ok(())
}

#[tauri::command]
pub fn reset_style<R: Runtime>(app: AppHandle<R>, id: String) -> Result<Style, String> {
    let path = styles_path(&app)?;
    let mut file = read_styles_at(&path)?;
    let style = reset_style_in(&mut file, &id)?;
    write_styles_at(&path, &file)?;
    Ok(style)
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

    fn styles_path_for_test(dir: &Path) -> PathBuf {
        dir.join("styles.json")
    }

    #[test]
    fn test_default_styles_three_builtins() {
        let file = default_styles();
        assert_eq!(file.styles.len(), 3);
        let ids: Vec<&str> = file.styles.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["viral", "motivational", "story"]);
        for s in &file.styles {
            assert!(s.builtin, "built-in {} must be builtin", s.id);
            assert!(!s.name.trim().is_empty());
            assert!(!s.content.trim().is_empty());
        }
    }

    #[test]
    fn test_normalize_seeds_missing_builtin() {
        let mut file = default_styles();
        file.styles.retain(|s| s.id != "story");
        assert_eq!(file.styles.len(), 2);
        let changed = normalize_styles(&mut file);
        assert!(changed);
        let story = file.styles.iter().find(|s| s.id == "story").unwrap();
        assert!(story.builtin);
        let (_, name, content) = DEFAULT_STYLES
            .iter()
            .copied()
            .find(|(id, _, _)| *id == "story")
            .unwrap();
        assert_eq!(story.name, name);
        assert_eq!(story.content, content);
    }

    #[test]
    fn test_normalize_dedupes_ids() {
        let mut file = default_styles();
        file.styles.push(Style {
            id: "viral".into(),
            name: "Viral 2".into(),
            content: "nội dung khác".into(),
            builtin: false,
        });
        let changed = normalize_styles(&mut file);
        assert!(changed);
        assert_eq!(file.styles.iter().filter(|s| s.id == "viral").count(), 1);
    }

    #[test]
    fn test_normalize_resets_empty_canonical_name_or_content() {
        let mut file = default_styles();
        let idx = file.styles.iter().position(|s| s.id == "viral").unwrap();
        file.styles[idx].name = "   ".into();
        file.styles[idx].content = String::new();
        let changed = normalize_styles(&mut file);
        assert!(changed);
        let viral = file.styles.iter().find(|s| s.id == "viral").unwrap();
        assert_eq!(viral.name, "Viral");
        assert!(!viral.content.trim().is_empty());
    }

    #[test]
    fn test_empty_file_treated_as_default() {
        let dir = unique_temp_dir("flowpost_styles_empty");
        let path = styles_path_for_test(&dir);
        fs::write(&path, "   \n  ").unwrap();
        let file = read_styles_at(&path).unwrap();
        assert_eq!(file.styles.len(), 3);
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.trim().is_empty(), "file must be rewritten");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_corrupt_json_returns_error() {
        let dir = unique_temp_dir("flowpost_styles_corrupt");
        let path = styles_path_for_test(&dir);
        fs::write(&path, "{not json").unwrap();
        assert!(read_styles_at(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_styles_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_styles_atomic");
        let path = styles_path_for_test(&dir);
        let tmp = path.with_extension("json.tmp");
        write_styles_at(&path, &default_styles()).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_validate_name_empty() {
        let err = validate_name("   ", &[], None).unwrap_err();
        assert_eq!(err, "Tên style không được để trống");
    }

    #[test]
    fn test_validate_name_too_long() {
        let long = "a".repeat(61);
        let err = validate_name(&long, &[], None).unwrap_err();
        assert_eq!(err, "Tên style tối đa 60 ký tự");
        assert!(validate_name(&"a".repeat(60), &[], None).is_ok());
    }

    #[test]
    fn test_validate_name_duplicate_case_insensitive() {
        let existing = vec![Style {
            id: "viral".into(),
            name: "Viral".into(),
            content: "c".into(),
            builtin: true,
        }];
        let err = validate_name("viral", &existing, None).unwrap_err();
        assert!(err.contains("đã tồn tại"));
        // excluding self must not error
        assert!(validate_name("viral", &existing, Some("viral")).is_ok());
    }

    #[test]
    fn test_validate_content_empty() {
        let err = validate_content("  \n ").unwrap_err();
        assert_eq!(err, "Nội dung style không được để trống");
    }

    #[test]
    fn test_validate_content_accepts_long_input() {
        let long = "a".repeat(20_000);
        assert!(validate_content(&long).is_ok());
        let mut file = default_styles();
        let style = create_style_in(&mut file, "Style dài", &long).unwrap();
        assert_eq!(style.content.chars().count(), 20_000);
        assert_eq!(file.styles.last().unwrap().content.chars().count(), 20_000);
    }

    #[test]
    fn test_create_style_valid() {
        let mut file = default_styles();
        let style = create_style_in(&mut file, "  Mới  ", "  nội dung  ").unwrap();
        assert!(!style.id.trim().is_empty());
        assert_ne!(style.id, "viral");
        assert!(!style.builtin);
        assert_eq!(style.name, "Mới");
        assert_eq!(style.content, "nội dung");
        assert_eq!(file.styles.len(), 4);
        assert_eq!(file.styles.last().unwrap().id, style.id);
    }

    #[test]
    fn test_update_style_excludes_self_in_duplicate_check() {
        let mut file = default_styles();
        let updated = update_style_in(&mut file, "viral", "Viral", "nội dung mới").unwrap();
        assert_eq!(updated.name, "Viral");
        assert_eq!(updated.content, "nội dung mới");
        assert!(updated.builtin);
    }

    #[test]
    fn test_duplicate_style_new_id_and_suffix() {
        let mut file = default_styles();
        let source = file.styles.iter().find(|s| s.id == "viral").unwrap().clone();
        let dup = duplicate_style_in(&mut file, "viral").unwrap();
        assert_ne!(dup.id, source.id);
        assert!(!dup.builtin);
        assert_eq!(dup.name, "Viral (bản sao)");
        assert_eq!(dup.content, source.content);
    }

    #[test]
    fn test_duplicate_style_increments_suffix() {
        let mut file = default_styles();
        let d1 = duplicate_style_in(&mut file, "viral").unwrap();
        let d2 = duplicate_style_in(&mut file, "viral").unwrap();
        assert_eq!(d1.name, "Viral (bản sao)");
        assert_eq!(d2.name, "Viral (bản sao 2)");
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_delete_builtin_rejected() {
        let mut file = default_styles();
        let err = delete_style_in(&mut file, "viral").unwrap_err();
        assert_eq!(
            err,
            "Không thể xóa style mặc định. Dùng \"Khôi phục mặc định\" để đặt lại."
        );
        assert!(file.styles.iter().any(|s| s.id == "viral"));
    }

    #[test]
    fn test_delete_custom_ok() {
        let mut file = default_styles();
        let custom = create_style_in(&mut file, "Riêng", "nội dung").unwrap();
        assert_eq!(file.styles.len(), 4);
        delete_style_in(&mut file, &custom.id).unwrap();
        assert_eq!(file.styles.len(), 3);
        assert!(!file.styles.iter().any(|s| s.id == custom.id));
    }

    #[test]
    fn test_reset_builtin_restores_defaults() {
        let mut file = default_styles();
        let idx = file.styles.iter().position(|s| s.id == "viral").unwrap();
        file.styles[idx].name = "Đổi tên".into();
        file.styles[idx].content = "Đổi nội dung".into();
        let reset = reset_style_in(&mut file, "viral").unwrap();
        let (_, name, content) = DEFAULT_STYLES
            .iter()
            .copied()
            .find(|(id, _, _)| *id == "viral")
            .unwrap();
        assert_eq!(reset.name, name);
        assert_eq!(reset.content, content);
        assert!(reset.builtin);
    }

    #[test]
    fn test_reset_custom_rejected() {
        let mut file = default_styles();
        let custom = create_style_in(&mut file, "Riêng", "nội dung").unwrap();
        let err = reset_style_in(&mut file, &custom.id).unwrap_err();
        assert_eq!(err, "Chỉ có thể khôi phục mặc định cho style mặc định");
    }

    #[test]
    fn test_vietnamese_literals_are_not_mojibaked() {
        // Expected string built from ASCII escape sequences only, so this assertion
        // cannot be corrupted together with the literals it guards.
        let expected = "T\u{00EA}n style kh\u{00F4}ng \u{0111}\u{01B0}\u{1EE3}c \u{0111}\u{1EC3} tr\u{1ED1}ng";
        assert_eq!(validate_name("", &[], None).unwrap_err(), expected);

        // Mojibake always introduces U+00C3 ('A-tilde') where correct Vietnamese has none.
        let mojibake_marker = '\u{00C3}';
        assert!(!validate_name("", &[], None).unwrap_err().contains(mojibake_marker));
        assert!(!validate_content("   ").unwrap_err().contains(mojibake_marker));
        assert!(!delete_style_in(&mut default_styles(), "viral").unwrap_err().contains(mojibake_marker));

        // Build the custom entry inline instead of adding a new helper.
        let mut with_custom = default_styles();
        with_custom.styles.push(Style {
            id: "custom-1".into(),
            name: "Custom".into(),
            content: "body".into(),
            builtin: false,
        });
        assert!(!reset_style_in(&mut with_custom, "custom-1").unwrap_err().contains(mojibake_marker));

        // Built-in bodies must be genuine Vietnamese (non-Latin-1 characters present) and not mojibaked.
        for s in default_styles().styles {
            assert!(s.content.chars().any(|c| c as u32 > 0x00FF), "style {} has no Vietnamese characters", s.id);
            assert!(!s.content.contains(mojibake_marker), "style {} content looks mojibaked", s.id);
            assert!(!s.name.contains(mojibake_marker), "style {} name looks mojibaked", s.id);
        }
    }
}
