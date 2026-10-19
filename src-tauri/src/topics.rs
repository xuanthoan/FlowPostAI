use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopicList {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub lists: Vec<TopicList>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopicsFile {
    topics: Vec<Topic>,
}

pub const MAX_TOPIC_NAME_CHARS: usize = 60;
pub const MAX_TEMPLATE_CHARS: usize = 4000;
pub const MAX_LIST_NAME_CHARS: usize = 60;
pub const MAX_LISTS_PER_TOPIC: usize = 10;
pub const MAX_VARIANTS_PER_LIST: usize = 50;
pub const MAX_VARIANT_CHARS: usize = 100;

fn topics_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("topics.json"))
}

fn write_topics_at(path: &Path, file: &TopicsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_topics_at(path: &Path) -> Result<TopicsFile, String> {
    let contents = if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    if contents.trim().is_empty() {
        let file = TopicsFile { topics: Vec::new() };
        write_topics_at(path, &file)?;
        return Ok(file);
    }
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

/// Trim list names and variants, drop empty variants, keep order, never dedupe.
fn normalize_lists(lists: Vec<TopicList>) -> Vec<TopicList> {
    lists
        .into_iter()
        .map(|list| {
            let variants = list
                .variants
                .into_iter()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect();
            TopicList {
                name: list.name.trim().to_string(),
                variants,
            }
        })
        .collect()
}

fn validate_name(name: &str, existing: &[Topic], exclude_id: Option<&str>) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tên chủ đề không được để trống".into());
    }
    if trimmed.chars().count() > MAX_TOPIC_NAME_CHARS {
        return Err("Tên chủ đề tối đa 60 ký tự".into());
    }
    let lower = trimmed.to_lowercase();
    for t in existing {
        if Some(t.id.as_str()) == exclude_id {
            continue;
        }
        if t.name.trim().to_lowercase() == lower {
            return Err(format!("Tên chủ đề '{}' đã tồn tại", trimmed));
        }
    }
    Ok(())
}

fn validate_template(template: &str) -> Result<(), String> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return Err("Prompt mẫu không được để trống".into());
    }
    if trimmed.chars().count() > MAX_TEMPLATE_CHARS {
        return Err("Prompt mẫu tối đa 4000 ký tự".into());
    }
    Ok(())
}

fn validate_lists(lists: &[TopicList]) -> Result<(), String> {
    if lists.len() > MAX_LISTS_PER_TOPIC {
        return Err("Mỗi chủ đề tối đa 10 danh sách".into());
    }
    let mut seen: Vec<String> = Vec::new();
    for list in lists {
        let name = list.name.trim();
        if name.is_empty() {
            return Err("Tên danh sách không được để trống".into());
        }
        if name.chars().count() > MAX_LIST_NAME_CHARS {
            return Err("Tên danh sách tối đa 60 ký tự".into());
        }
        if name.contains('[') || name.contains(']') {
            return Err("Tên danh sách không được chứa '[' hoặc ']'".into());
        }
        let lower = name.to_lowercase();
        if seen.contains(&lower) {
            return Err(format!("Tên danh sách '{}' đã tồn tại trong chủ đề", name));
        }
        seen.push(lower);
        if list.variants.is_empty() {
            return Err(format!("Danh sách '{}' phải có ít nhất 1 cụm từ", name));
        }
        if list.variants.len() > MAX_VARIANTS_PER_LIST {
            return Err("Mỗi danh sách tối đa 50 cụm từ".into());
        }
        for v in &list.variants {
            if v.trim().chars().count() > MAX_VARIANT_CHARS {
                return Err("Cụm từ tối đa 100 ký tự".into());
            }
        }
    }
    Ok(())
}

fn create_topic_in(file: &mut TopicsFile, payload: &TopicInput) -> Result<Topic, String> {
    validate_name(&payload.name, &file.topics, None)?;
    validate_template(&payload.template)?;
    let lists = normalize_lists(payload.lists.clone());
    validate_lists(&lists)?;
    let topic = Topic {
        id: uuid::Uuid::new_v4().to_string(),
        name: payload.name.trim().to_string(),
        template: payload.template.trim().to_string(),
        lists,
    };
    file.topics.push(topic.clone());
    Ok(topic)
}

fn update_topic_in(file: &mut TopicsFile, payload: &UpdateTopicPayload) -> Result<Topic, String> {
    let idx = file
        .topics
        .iter()
        .position(|t| t.id == payload.id)
        .ok_or_else(|| format!("Không tìm thấy chủ đề '{}'", payload.id))?;
    validate_name(&payload.name, &file.topics, Some(&payload.id))?;
    validate_template(&payload.template)?;
    let lists = normalize_lists(payload.lists.clone());
    validate_lists(&lists)?;
    file.topics[idx].name = payload.name.trim().to_string();
    file.topics[idx].template = payload.template.trim().to_string();
    file.topics[idx].lists = lists;
    Ok(file.topics[idx].clone())
}

fn delete_topic_in(file: &mut TopicsFile, id: &str) -> Result<(), String> {
    let idx = file
        .topics
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| format!("Không tìm thấy chủ đề '{}'", id))?;
    file.topics.remove(idx);
    Ok(())
}

// --- Commands ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicInput {
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub lists: Vec<TopicList>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTopicPayload {
    pub id: String,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub lists: Vec<TopicList>,
}

#[tauri::command]
pub fn list_topics<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Topic>, String> {
    let path = topics_path(&app)?;
    let file = read_topics_at(&path)?;
    Ok(file.topics)
}

#[tauri::command]
pub fn create_topic<R: Runtime>(app: AppHandle<R>, payload: TopicInput) -> Result<Topic, String> {
    let path = topics_path(&app)?;
    let mut file = read_topics_at(&path)?;
    let topic = create_topic_in(&mut file, &payload)?;
    write_topics_at(&path, &file)?;
    Ok(topic)
}

#[tauri::command]
pub fn update_topic<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateTopicPayload,
) -> Result<Topic, String> {
    let path = topics_path(&app)?;
    let mut file = read_topics_at(&path)?;
    let topic = update_topic_in(&mut file, &payload)?;
    write_topics_at(&path, &file)?;
    Ok(topic)
}

#[tauri::command]
pub fn delete_topic<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    let path = topics_path(&app)?;
    let mut file = read_topics_at(&path)?;
    delete_topic_in(&mut file, &id)?;
    write_topics_at(&path, &file)?;
    Ok(())
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

    fn topics_path_for_test(dir: &Path) -> PathBuf {
        dir.join("topics.json")
    }

    fn list(name: &str, variants: &[&str]) -> TopicList {
        TopicList {
            name: name.into(),
            variants: variants.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn topic(id: &str, name: &str) -> Topic {
        Topic {
            id: id.into(),
            name: name.into(),
            template: "template".into(),
            lists: vec![],
        }
    }

    fn input(name: &str, template: &str, lists: Vec<TopicList>) -> TopicInput {
        TopicInput {
            name: name.into(),
            template: template.into(),
            lists,
        }
    }

    #[test]
    fn test_empty_topics_file_has_no_topics() {
        let dir = unique_temp_dir("flowpost_topics_empty");
        let path = topics_path_for_test(&dir);
        fs::write(&path, "{\"topics\":[]}").unwrap();
        let file = read_topics_at(&path).unwrap();
        assert!(file.topics.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_missing_file_is_created_empty() {
        let dir = unique_temp_dir("flowpost_topics_missing");
        let path = topics_path_for_test(&dir);
        assert!(!path.exists());
        let file = read_topics_at(&path).unwrap();
        assert!(file.topics.is_empty());
        assert!(path.exists(), "missing file must be created");
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.trim().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_empty_file_treated_as_empty() {
        let dir = unique_temp_dir("flowpost_topics_blank");
        let path = topics_path_for_test(&dir);
        fs::write(&path, "   \n  ").unwrap();
        let file = read_topics_at(&path).unwrap();
        assert!(file.topics.is_empty());
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.trim().is_empty(), "file must be rewritten");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_corrupt_json_returns_error() {
        let dir = unique_temp_dir("flowpost_topics_corrupt");
        let path = topics_path_for_test(&dir);
        std::fs::write(&path, "{not json").unwrap();
        assert!(read_topics_at(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_write_topics_atomic_cleans_tmp() {
        let dir = unique_temp_dir("flowpost_topics_atomic");
        let path = topics_path_for_test(&dir);
        let tmp = path.with_extension("json.tmp");
        let file = TopicsFile {
            topics: vec![topic("t1", "Chủ đề")],
        };
        write_topics_at(&path, &file).unwrap();
        assert!(path.exists());
        assert!(!tmp.exists(), "tmp should be cleaned");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_validate_name_empty() {
        let err = validate_name("   ", &[], None).unwrap_err();
        assert_eq!(err, "Tên chủ đề không được để trống");
    }

    #[test]
    fn test_validate_name_too_long() {
        let long = "a".repeat(61);
        let err = validate_name(&long, &[], None).unwrap_err();
        assert_eq!(err, "Tên chủ đề tối đa 60 ký tự");
        assert!(validate_name(&"a".repeat(60), &[], None).is_ok());
    }

    #[test]
    fn test_validate_name_duplicate_case_insensitive() {
        let existing = vec![topic("t1", "Viral")];
        let err = validate_name("viral", &existing, None).unwrap_err();
        assert!(err.contains("đã tồn tại"), "got {}", err);
        assert!(validate_name("viral", &existing, Some("t1")).is_ok());
    }

    #[test]
    fn test_validate_template_empty() {
        let err = validate_template("  \n ").unwrap_err();
        assert_eq!(err, "Prompt mẫu không được để trống");
    }

    #[test]
    fn test_validate_template_too_long() {
        let err = validate_template(&"a".repeat(4001)).unwrap_err();
        assert_eq!(err, "Prompt mẫu tối đa 4000 ký tự");
        assert!(validate_template(&"a".repeat(4000)).is_ok());
    }

    #[test]
    fn test_validate_list_name_empty() {
        let err = validate_lists(&[list("   ", &["a"])]).unwrap_err();
        assert_eq!(err, "Tên danh sách không được để trống");
    }

    #[test]
    fn test_validate_list_name_too_long() {
        let long_name = "a".repeat(61);
        let err = validate_lists(&[list(&long_name, &["a"])]).unwrap_err();
        assert_eq!(err, "Tên danh sách tối đa 60 ký tự");
    }

    #[test]
    fn test_validate_list_name_rejects_brackets() {
        let err = validate_lists(&[list("phong [cach]", &["a"])]).unwrap_err();
        assert_eq!(err, "Tên danh sách không được chứa '[' hoặc ']'");
        let err2 = validate_lists(&[list("phong]cach", &["a"])]).unwrap_err();
        assert_eq!(err2, "Tên danh sách không được chứa '[' hoặc ']'");
    }

    #[test]
    fn test_validate_duplicate_list_name_case_insensitive() {
        let err = validate_lists(&[list("Phong cách", &["a"]), list("phong CÁCH", &["b"])]).unwrap_err();
        assert!(err.contains("đã tồn tại trong chủ đề"), "got {}", err);
    }

    #[test]
    fn test_validate_variants_empty_after_trim() {
        let lists = normalize_lists(vec![list("x", &["  ", "\t"])]);
        assert!(lists[0].variants.is_empty());
        let err = validate_lists(&lists).unwrap_err();
        assert_eq!(err, "Danh sách 'x' phải có ít nhất 1 cụm từ");
    }

    #[test]
    fn test_validate_variant_too_long() {
        let long_variant = "a".repeat(101);
        let lists = normalize_lists(vec![list("x", &[&long_variant])]);
        let err = validate_lists(&lists).unwrap_err();
        assert_eq!(err, "Cụm từ tối đa 100 ký tự");
    }

    #[test]
    fn test_validate_too_many_variants() {
        let variants: Vec<String> = (0..51).map(|i| format!("v{}", i)).collect();
        let too_many = TopicList {
            name: "x".into(),
            variants,
        };
        let err = validate_lists(&[too_many]).unwrap_err();
        assert_eq!(err, "Mỗi danh sách tối đa 50 cụm từ");
    }

    #[test]
    fn test_validate_too_many_lists() {
        let lists: Vec<TopicList> = (0..11)
            .map(|i| list(&format!("l{}", i), &["a"]))
            .collect();
        let err = validate_lists(&lists).unwrap_err();
        assert_eq!(err, "Mỗi chủ đề tối đa 10 danh sách");
    }

    #[test]
    fn test_create_topic_trims_name_template_and_variants() {
        let mut file = TopicsFile { topics: vec![] };
        let created = create_topic_in(
            &mut file,
            &input(
                "  Chủ đề  ",
                "  prompt mẫu  ",
                vec![list("  List  ", &[" a ", "  ", " b "])],
            ),
        )
        .unwrap();
        assert!(!created.id.trim().is_empty());
        assert_eq!(created.name, "Chủ đề");
        assert_eq!(created.template, "prompt mẫu");
        assert_eq!(created.lists.len(), 1);
        assert_eq!(created.lists[0].name, "List");
        assert_eq!(created.lists[0].variants, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(file.topics.len(), 1);
        assert_eq!(file.topics[0].id, created.id);
    }

    #[test]
    fn test_update_topic_excludes_self_in_duplicate_check() {
        let mut file = TopicsFile { topics: vec![] };
        let created = create_topic_in(&mut file, &input("Chủ đề", "p", vec![])).unwrap();
        let payload = UpdateTopicPayload {
            id: created.id.clone(),
            name: "Chủ đề".into(),
            template: "p mới".into(),
            lists: vec![list("List", &["a"])],
        };
        let updated = update_topic_in(&mut file, &payload).unwrap();
        assert_eq!(updated.name, "Chủ đề");
        assert_eq!(updated.template, "p mới");
        assert_eq!(updated.lists.len(), 1);
    }

    #[test]
    fn test_delete_topic_ok() {
        let mut file = TopicsFile { topics: vec![] };
        let created = create_topic_in(&mut file, &input("Chủ đề", "p", vec![])).unwrap();
        assert_eq!(file.topics.len(), 1);
        delete_topic_in(&mut file, &created.id).unwrap();
        assert!(file.topics.is_empty());
    }

    #[test]
    fn test_delete_nonexistent_topic_errors() {
        let mut file = TopicsFile { topics: vec![] };
        let err = delete_topic_in(&mut file, "missing").unwrap_err();
        assert_eq!(err, "Không tìm thấy chủ đề 'missing'");
    }

    #[test]
    fn test_roundtrip_persists_lists() {
        let dir = unique_temp_dir("flowpost_topics_roundtrip");
        let path = topics_path_for_test(&dir);
        let file = TopicsFile {
            topics: vec![Topic {
                id: "t1".into(),
                name: "Chủ đề".into(),
                template: "p [a]".into(),
                lists: vec![list("a", &["x", "y"])],
            }],
        };
        write_topics_at(&path, &file).unwrap();
        let read = read_topics_at(&path).unwrap();
        assert_eq!(read.topics.len(), 1);
        assert_eq!(read.topics[0].lists.len(), 1);
        assert_eq!(read.topics[0].lists[0].name, "a");
        assert_eq!(read.topics[0].lists[0].variants, vec!["x".to_string(), "y".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_vietnamese_literals_are_not_mojibaked() {
        // Expected strings built from ASCII escapes only so this assertion cannot
        // be corrupted together with the literals it guards.
        let expected_name =
            "T\u{00EA}n ch\u{1EE7} \u{0111}\u{1EC1} kh\u{00F4}ng \u{0111}\u{01B0}\u{1EE3}c \u{0111}\u{1EC3} tr\u{1ED1}ng";
        assert_eq!(validate_name("", &[], None).unwrap_err(), expected_name);

        let expected_template = "Prompt m\u{1EAB}u kh\u{00F4}ng \u{0111}\u{01B0}\u{1EE3}c \u{0111}\u{1EC3} tr\u{1ED1}ng";
        assert_eq!(validate_template("").unwrap_err(), expected_template);

        let expected_variant =
            "C\u{1EE5}m t\u{1EEB} t\u{1ED1}i \u{0111}a 100 k\u{00FD} t\u{1EF1}";
        assert_eq!(
            validate_lists(&normalize_lists(vec![list("x", &[&"a".repeat(101)])])).unwrap_err(),
            expected_variant
        );

        let expected_not_found = "Kh\u{00F4}ng t\u{00EC}m th\u{1EA5}y ch\u{1EE7} \u{0111}\u{1EC1} 'x'";
        assert_eq!(
            delete_topic_in(&mut TopicsFile { topics: vec![] }, "x").unwrap_err(),
            expected_not_found
        );

        // Mojibake always introduces U+00C3 ('A-tilde') where correct Vietnamese has none.
        let mojibake_marker = '\u{00C3}';
        assert!(!validate_name("", &[], None).unwrap_err().contains(mojibake_marker));
        assert!(!validate_template("").unwrap_err().contains(mojibake_marker));
        assert!(!validate_lists(&[list("", &["a"])]).unwrap_err().contains(mojibake_marker));
        assert!(!validate_lists(&[list("x", &["a"]), list("x", &["b"])])
            .unwrap_err()
            .contains(mojibake_marker));
    }
}
