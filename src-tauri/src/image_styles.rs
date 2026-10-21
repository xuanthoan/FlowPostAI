use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

/// Style used when nothing else is configured and as the fallback for stale config.
pub const DEFAULT_IMAGE_STYLE_ID: &str = "cinematic";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImageStyle {
    pub id: String,
    pub name: String,
    pub content: String,
    pub builtin: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageStylesFile {
    styles: Vec<ImageStyle>,
}

/// Built-in image styles with fixed ids so overrides/restores always match.
/// Tuples are `(id, name, content)`.
pub const DEFAULT_IMAGE_STYLES: &[(&str, &str, &str)] = &[
    (
        "cinematic",
        "Điện ảnh",
        "Ảnh mang phong cách điện ảnh với ánh sáng tương phản mạnh, tông màu trầm ấm và độ sâu trường ảnh nông. Bố cục theo quy tắc một phần ba, chủ thể đặt lệch tâm, hậu cảnh mờ nhẹ tạo cảm giác như một khung hình phim. Cảm xúc lắng đọng, giàu kịch tính. Tuyệt đối không có chữ, không có chữ cái, không có logo hay ký tự văn bản trong ảnh.",
    ),
    (
        "minimal",
        "Tối giản",
        "Ảnh tối giản với nền phẳng một màu sáng, ánh sáng mềm và tán xạ đều, hạn chế bóng đổ gắt. Bố cục thoáng, nhiều khoảng trắng, chủ thể đơn lẻ đặt giữa hoặc lệch nhẹ, bảng màu giới hạn hai đến ba tông. Cảm xúc nhẹ nhàng, sạch sẽ, dễ chịu. Tuyệt đối không có chữ, không có chữ cái, không có logo hay ký tự văn bản trong ảnh.",
    ),
    (
        "illustration",
        "Minh hoạ phẳng",
        "Ảnh minh hoạ phẳng với mảng màu rõ ràng, đường nét gọn gàng và bố cục cân bằng. Ánh sáng mô phỏng nhẹ nhàng, màu sắc tươi nhưng hài hoà, không dùng gradient phức tạp hay hiệu ứng bóng đổ nặng. Cảm xúc vui vẻ, thân thiện, phù hợp minh hoạ cho bài viết. Tuyệt đối không có chữ, không có chữ cái, không có logo hay ký tự văn bản trong ảnh.",
    ),
];

fn image_styles_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("image_styles.json"))
}

fn default_image_styles() -> ImageStylesFile {
    ImageStylesFile {
        styles: DEFAULT_IMAGE_STYLES
            .iter()
            .map(|(id, name, content)| ImageStyle {
                id: (*id).to_string(),
                name: (*name).to_string(),
                content: (*content).to_string(),
                builtin: true,
            })
            .collect(),
    }
}

/// Self-heal the on-disk image styles list. Returns whether anything was modified.
/// (a) drop duplicate ids keeping the first; (b) force canonical entries `builtin = true`;
/// (c) append missing canonical built-ins; (d) reset a canonical entry whose name/content is blank.
fn normalize_image_styles(file: &mut ImageStylesFile) -> bool {
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
    for (cid, cname, ccontent) in DEFAULT_IMAGE_STYLES.iter().copied() {
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
    for (cid, cname, ccontent) in DEFAULT_IMAGE_STYLES.iter().copied() {
        if !file.styles.iter().any(|s| s.id == cid) {
            file.styles.push(ImageStyle {
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

fn validate_name(
    name: &str,
    existing: &[ImageStyle],
    exclude_id: Option<&str>,
) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tên style ảnh không được để trống".into());
    }
    if trimmed.chars().count() > 60 {
        return Err("Tên style ảnh tối đa 60 ký tự".into());
    }
    let lower = trimmed.to_lowercase();
    for s in existing {
        if Some(s.id.as_str()) == exclude_id {
            continue;
        }
        if s.name.trim().to_lowercase() == lower {
            return Err(format!("Tên style ảnh '{}' đã tồn tại", trimmed));
        }
    }
    Ok(())
}

fn validate_content(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Nội dung style ảnh không được để trống".into());
    }
    Ok(())
}

fn create_image_style_in(
    file: &mut ImageStylesFile,
    name: &str,
    content: &str,
) -> Result<ImageStyle, String> {
    validate_name(name, &file.styles, None)?;
    validate_content(content)?;
    let style = ImageStyle {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        content: content.trim().to_string(),
        builtin: false,
    };
    file.styles.push(style.clone());
    Ok(style)
}

fn update_image_style_in(
    file: &mut ImageStylesFile,
    id: &str,
    name: &str,
    content: &str,
) -> Result<ImageStyle, String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style ảnh '{}'", id))?;
    validate_name(name, &file.styles, Some(id))?;
    validate_content(content)?;
    file.styles[idx].name = name.trim().to_string();
    file.styles[idx].content = content.trim().to_string();
    Ok(file.styles[idx].clone())
}

fn duplicate_image_style_in(file: &mut ImageStylesFile, id: &str) -> Result<ImageStyle, String> {
    let source = file
        .styles
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("Không tìm thấy style ảnh '{}'", id))?;

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

    let style = ImageStyle {
        id: uuid::Uuid::new_v4().to_string(),
        name: new_name,
        content: source.content.clone(),
        builtin: false,
    };
    file.styles.push(style.clone());
    Ok(style)
}

fn delete_image_style_in(file: &mut ImageStylesFile, id: &str) -> Result<(), String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style ảnh '{}'", id))?;
    if file.styles[idx].builtin {
        return Err(
            "Không thể xóa style ảnh mặc định. Dùng \"Khôi phục mặc định\" để đặt lại.".into(),
        );
    }
    file.styles.remove(idx);
    Ok(())
}

fn reset_image_style_in(file: &mut ImageStylesFile, id: &str) -> Result<ImageStyle, String> {
    let idx = file
        .styles
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("Không tìm thấy style ảnh '{}'", id))?;
    let (_, cname, ccontent) = DEFAULT_IMAGE_STYLES
        .iter()
        .copied()
        .find(|(cid, _, _)| *cid == id)
        .ok_or("Chỉ có thể khôi phục mặc định cho style ảnh mặc định")?;
    file.styles[idx].name = cname.to_string();
    file.styles[idx].content = ccontent.to_string();
    file.styles[idx].builtin = true;
    Ok(file.styles[idx].clone())
}

fn write_image_styles_at(path: &Path, file: &ImageStylesFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_image_styles_at(path: &Path) -> Result<ImageStylesFile, String> {
    let contents = if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    if contents.trim().is_empty() {
        let file = default_image_styles();
        write_image_styles_at(path, &file)?;
        return Ok(file);
    }
    let mut file: ImageStylesFile = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    if normalize_image_styles(&mut file) {
        write_image_styles_at(path, &file)?;
    }
    Ok(file)
}

/// Resolve a stored style value to the canonical image style.
/// Matches by id first (trimmed, exact), then by name (trimmed, case-insensitive).
pub fn find_image_style(styles: &[ImageStyle], id: &str) -> Option<ImageStyle> {
    let needle = id.trim();
    if needle.is_empty() {
        return None;
    }
    if let Some(by_id) = styles.iter().find(|s| s.id.as_str() == needle) {
        return Some(by_id.clone());
    }
    let needle_lower = needle.to_lowercase();
    styles
        .iter()
        .find(|s| s.name.trim().to_lowercase() == needle_lower)
        .cloned()
}

pub fn read_image_styles<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<ImageStyle>, String> {
    let path = image_styles_path(app)?;
    let file = read_image_styles_at(&path)?;
    Ok(file.styles)
}

// --- Commands ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStyleInput {
    pub name: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateImageStylePayload {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[tauri::command]
pub fn list_image_styles<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ImageStyle>, String> {
    read_image_styles(&app)
}

#[tauri::command]
pub fn create_image_style<R: Runtime>(
    app: AppHandle<R>,
    payload: ImageStyleInput,
) -> Result<ImageStyle, String> {
    let path = image_styles_path(&app)?;
    let mut file = read_image_styles_at(&path)?;
    let style = create_image_style_in(&mut file, &payload.name, &payload.content)?;
    write_image_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn update_image_style<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateImageStylePayload,
) -> Result<ImageStyle, String> {
    let path = image_styles_path(&app)?;
    let mut file = read_image_styles_at(&path)?;
    let style = update_image_style_in(&mut file, &payload.id, &payload.name, &payload.content)?;
    write_image_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn duplicate_image_style<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<ImageStyle, String> {
    let path = image_styles_path(&app)?;
    let mut file = read_image_styles_at(&path)?;
    let style = duplicate_image_style_in(&mut file, &id)?;
    write_image_styles_at(&path, &file)?;
    Ok(style)
}

#[tauri::command]
pub fn delete_image_style<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    let path = image_styles_path(&app)?;
    let mut file = read_image_styles_at(&path)?;
    delete_image_style_in(&mut file, &id)?;
    write_image_styles_at(&path, &file)?;
    Ok(())
}

#[tauri::command]
pub fn reset_image_style<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<ImageStyle, String> {
    let path = image_styles_path(&app)?;
    let mut file = read_image_styles_at(&path)?;
    let style = reset_image_style_in(&mut file, &id)?;
    write_image_styles_at(&path, &file)?;
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

    fn image_styles_path_for_test(dir: &Path) -> PathBuf {
        dir.join("image_styles.json")
    }

    #[test]
    fn default_image_styles_three_builtins() {
        let file = default_image_styles();
        assert_eq!(file.styles.len(), 3);
        let ids: Vec<&str> = file.styles.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["cinematic", "minimal", "illustration"]);
        for s in &file.styles {
            assert!(s.builtin, "built-in {} must be builtin", s.id);
            assert!(!s.name.trim().is_empty());
            assert!(s.content.chars().count() > 80, "style {} content too short", s.id);
        }
        assert_eq!(DEFAULT_IMAGE_STYLE_ID, "cinematic");
        assert!(file.styles.iter().any(|s| s.id == DEFAULT_IMAGE_STYLE_ID));
    }

    #[test]
    fn normalize_seeds_missing_builtin() {
        let mut file = default_image_styles();
        file.styles.retain(|s| s.id != "minimal");
        assert_eq!(file.styles.len(), 2);
        let changed = normalize_image_styles(&mut file);
        assert!(changed);
        let minimal = file.styles.iter().find(|s| s.id == "minimal").unwrap();
        assert!(minimal.builtin);
        let (_, name, content) = DEFAULT_IMAGE_STYLES
            .iter()
            .copied()
            .find(|(id, _, _)| *id == "minimal")
            .unwrap();
        assert_eq!(minimal.name, name);
        assert_eq!(minimal.content, content);
    }

    #[test]
    fn normalize_dedupes_ids() {
        let mut file = default_image_styles();
        file.styles.push(ImageStyle {
            id: "cinematic".into(),
            name: "Điện ảnh 2".into(),
            content: "nội dung khác".into(),
            builtin: false,
        });
        let changed = normalize_image_styles(&mut file);
        assert!(changed);
        assert_eq!(file.styles.iter().filter(|s| s.id == "cinematic").count(), 1);
    }

    #[test]
    fn normalize_resets_empty_canonical_name_or_content() {
        let mut file = default_image_styles();
        let idx = file.styles.iter().position(|s| s.id == "cinematic").unwrap();
        file.styles[idx].name = "   ".into();
        file.styles[idx].content = String::new();
        let changed = normalize_image_styles(&mut file);
        assert!(changed);
        let cinematic = file.styles.iter().find(|s| s.id == "cinematic").unwrap();
        assert_eq!(cinematic.name, "Điện ảnh");
        assert!(!cinematic.content.trim().is_empty());
    }

    #[test]
    fn empty_file_treated_as_default() {
        let dir = unique_temp_dir("flowpost_image_styles_empty");
        let path = image_styles_path_for_test(&dir);
        fs::write(&path, "   \n  ").unwrap();
        let file = read_image_styles_at(&path).unwrap();
        assert_eq!(file.styles.len(), 3);
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.trim().is_empty(), "file must be rewritten");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_json_returns_error() {
        let dir = unique_temp_dir("flowpost_image_styles_corrupt");
        let path = image_styles_path_for_test(&dir);
        fs::write(&path, "{not json").unwrap();
        assert!(read_image_styles_at(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_image_styles_atomic");
        let path = image_styles_path_for_test(&dir);
        let tmp = path.with_extension("json.tmp");
        write_image_styles_at(&path, &default_image_styles()).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn validate_name_empty() {
        let err = validate_name("   ", &[], None).unwrap_err();
        assert_eq!(err, "Tên style ảnh không được để trống");
    }

    #[test]
    fn validate_name_too_long() {
        let long = "a".repeat(61);
        let err = validate_name(&long, &[], None).unwrap_err();
        assert_eq!(err, "Tên style ảnh tối đa 60 ký tự");
        assert!(validate_name(&"a".repeat(60), &[], None).is_ok());
    }

    #[test]
    fn validate_name_duplicate_case_insensitive() {
        let existing = vec![ImageStyle {
            id: "cinematic".into(),
            name: "Điện ảnh".into(),
            content: "c".into(),
            builtin: true,
        }];
        let err = validate_name("điện ảnh", &existing, None).unwrap_err();
        assert_eq!(err, "Tên style ảnh 'điện ảnh' đã tồn tại");
        // excluding self must not error
        assert!(validate_name("điện ảnh", &existing, Some("cinematic")).is_ok());
    }

    #[test]
    fn validate_content_empty() {
        let err = validate_content("  \n ").unwrap_err();
        assert_eq!(err, "Nội dung style ảnh không được để trống");
    }

    #[test]
    fn create_update_duplicate_delete_reset_roundtrip() {
        let mut file = default_image_styles();

        let created = create_image_style_in(&mut file, "  Riêng  ", "  nội dung  ").unwrap();
        assert!(!created.id.trim().is_empty());
        assert!(!created.builtin);
        assert_eq!(created.name, "Riêng");
        assert_eq!(created.content, "nội dung");
        assert_eq!(file.styles.len(), 4);

        let updated = update_image_style_in(&mut file, &created.id, "Riêng 2", "nội dung 2").unwrap();
        assert_eq!(updated.name, "Riêng 2");
        assert_eq!(updated.content, "nội dung 2");

        let first_dup = duplicate_image_style_in(&mut file, &created.id).unwrap();
        let second_dup = duplicate_image_style_in(&mut file, &created.id).unwrap();
        assert_ne!(first_dup.id, second_dup.id);
        assert_eq!(first_dup.name, "Riêng 2 (bản sao)");
        assert_eq!(second_dup.name, "Riêng 2 (bản sao 2)");
        assert_eq!(first_dup.content, "nội dung 2");
        assert!(!first_dup.builtin);

        delete_image_style_in(&mut file, &created.id).unwrap();
        assert!(!file.styles.iter().any(|s| s.id == created.id));

        let idx = file.styles.iter().position(|s| s.id == "minimal").unwrap();
        file.styles[idx].name = "Đổi tên".into();
        file.styles[idx].content = "Đổi nội dung".into();
        let reset = reset_image_style_in(&mut file, "minimal").unwrap();
        let (_, name, content) = DEFAULT_IMAGE_STYLES
            .iter()
            .copied()
            .find(|(id, _, _)| *id == "minimal")
            .unwrap();
        assert_eq!(reset.name, name);
        assert_eq!(reset.content, content);
        assert!(reset.builtin);
    }

    #[test]
    fn delete_builtin_rejected() {
        let mut file = default_image_styles();
        let err = delete_image_style_in(&mut file, "cinematic").unwrap_err();
        assert_eq!(
            err,
            "Không thể xóa style ảnh mặc định. Dùng \"Khôi phục mặc định\" để đặt lại."
        );
        assert!(file.styles.iter().any(|s| s.id == "cinematic"));
    }

    #[test]
    fn reset_custom_rejected() {
        let mut file = default_image_styles();
        let custom = create_image_style_in(&mut file, "Riêng", "nội dung").unwrap();
        let err = reset_image_style_in(&mut file, &custom.id).unwrap_err();
        assert_eq!(err, "Chỉ có thể khôi phục mặc định cho style ảnh mặc định");
    }

    #[test]
    fn find_image_style_by_id_name_and_unknown() {
        let styles = default_image_styles().styles;
        assert_eq!(
            find_image_style(&styles, "cinematic").map(|s| s.name),
            Some("Điện ảnh".to_string())
        );
        assert_eq!(
            find_image_style(&styles, "  điện ẢNH  ").map(|s| s.id),
            Some("cinematic".to_string())
        );
        assert!(find_image_style(&styles, "không-tồn-tại").is_none());
        assert!(find_image_style(&styles, "   ").is_none());
        assert!(find_image_style(&styles, "").is_none());
    }

    #[test]
    fn vietnamese_literals_are_not_mojibaked() {
        // Expected string built from ASCII escape sequences only, so this assertion
        // cannot be corrupted together with the literals it guards.
        let expected = "T\u{00EA}n style \u{1EA3}nh kh\u{00F4}ng \u{0111}\u{01B0}\u{1EE3}c \u{0111}\u{1EC3} tr\u{1ED1}ng";
        assert_eq!(validate_name("", &[], None).unwrap_err(), expected);

        // Mojibake always introduces U+00C3 ('A-tilde') where correct Vietnamese has none.
        let mojibake_marker = '\u{00C3}';
        assert!(!validate_name("", &[], None)
            .unwrap_err()
            .contains(mojibake_marker));
        assert!(!validate_content("   ")
            .unwrap_err()
            .contains(mojibake_marker));
        assert!(!delete_image_style_in(&mut default_image_styles(), "cinematic")
            .unwrap_err()
            .contains(mojibake_marker));

        // Build the custom entry inline instead of adding a new helper.
        let mut with_custom = default_image_styles();
        with_custom.styles.push(ImageStyle {
            id: "custom-1".into(),
            name: "Custom".into(),
            content: "body".into(),
            builtin: false,
        });
        assert!(!reset_image_style_in(&mut with_custom, "custom-1")
            .unwrap_err()
            .contains(mojibake_marker));

        // Built-in bodies must be genuine Vietnamese (non-Latin-1 characters present) and not mojibaked.
        for s in default_image_styles().styles {
            assert!(
                s.content.chars().any(|c| c as u32 > 0x00FF),
                "style {} has no Vietnamese characters",
                s.id
            );
            assert!(
                !s.content.contains(mojibake_marker),
                "style {} content looks mojibaked",
                s.id
            );
            assert!(
                !s.name.contains(mojibake_marker),
                "style {} name looks mojibaked",
                s.id
            );
        }
    }
}
