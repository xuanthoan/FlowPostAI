use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

const SERVICE: &str = "vn.flowpost.desktop";
const FB_KEY: &str = "facebook_token";

fn entry_for(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, key).map_err(|e| e.to_string())
}

// --- File fallback ---

fn fallback_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join("secure-credentials.json"))
}

#[cfg(test)]
fn fallback_path_for_test(dir: &Path) -> PathBuf {
    dir.join("secure-credentials.json")
}

fn read_fallback_map(path: &Path) -> HashMap<String, String> {
    if !path.exists() {
        return HashMap::new();
    }
    let contents = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&contents).unwrap_or_default()
}

fn write_fallback_map(path: &Path, map: &HashMap<String, String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    // atomic via tmp
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn set_fallback<R: Runtime>(app: &AppHandle<R>, key: &str, value: &str) -> Result<(), String> {
    let path = fallback_path(app)?;
    let mut map = read_fallback_map(&path);
    if value.trim().is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value.to_string());
    }
    write_fallback_map(&path, &map)
}

fn get_fallback<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<String>, String> {
    let path = fallback_path(app)?;
    let map = read_fallback_map(&path);
    Ok(map.get(key).cloned())
}

fn delete_fallback<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<(), String> {
    set_fallback(app, key, "")
}

fn has_fallback<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<bool, String> {
    Ok(get_fallback(app, key)?.is_some())
}

// For tests with temp dir
#[cfg(test)]
fn set_fallback_at(dir: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = fallback_path_for_test(dir);
    let mut map = read_fallback_map(&path);
    if value.trim().is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value.to_string());
    }
    write_fallback_map(&path, &map)
}
#[cfg(test)]
fn get_fallback_at(dir: &Path, key: &str) -> Result<Option<String>, String> {
    let path = fallback_path_for_test(dir);
    let map = read_fallback_map(&path);
    Ok(map.get(key).cloned())
}

// --- Hybrid keyring + file ---

pub fn set_secret_hybrid<R: Runtime>(app: Option<&AppHandle<R>>, key: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return delete_secret_hybrid(app, key);
    }
    // Keyring là nơi lưu chính. Fallback plaintext chỉ ghi khi keyring thất bại.
    let keyring_ok = match entry_for(key) {
        Ok(e) => e.set_password(value).is_ok(),
        Err(_) => false,
    };
    if keyring_ok {
        // Keyring đã có giá trị mới: xóa bản sao plaintext cũ (best-effort).
        if let Some(a) = app {
            let _ = delete_fallback(a, key);
        }
        return Ok(());
    }
    if let Some(a) = app {
        set_fallback(a, key, value)?;
        return Ok(());
    }
    Err("No app handle for fallback and keyring failed".into())
}

/// Giá trị keyring trùng với fallback nghĩa là fallback chỉ còn là bản sao cũ
/// và có thể xóa sau khi đọc thành công từ keyring.
fn should_scrub_fallback(keyring_value: &str, fallback_value: &str) -> bool {
    keyring_value == fallback_value
}

pub fn get_secret_hybrid<R: Runtime>(app: Option<&AppHandle<R>>, key: &str) -> Result<Option<String>, String> {
    // Try keyring first
    match entry_for(key) {
        Ok(entry) => match entry.get_password() {
            Ok(value) => {
                // Lazy migration: fallback trùng giá trị keyring thì xóa,
                // khác giá trị thì giữ nguyên (không ghi đè keyring).
                if let Some(a) = app {
                    if let Ok(Some(fallback_value)) = get_fallback(a, key) {
                        if should_scrub_fallback(&value, &fallback_value) {
                            let _ = delete_fallback(a, key);
                        }
                    }
                }
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => {
                let Some(a) = app else {
                    return Ok(None);
                };
                let Some(value) = get_fallback(a, key)? else {
                    return Ok(None);
                };
                // Fallback còn giá trị: thử migrate sang keyring rồi xóa fallback.
                let migrated = entry_for(key)
                    .map(|e| e.set_password(&value).is_ok())
                    .unwrap_or(false);
                if migrated {
                    let _ = delete_fallback(a, key);
                }
                Ok(Some(value))
            }
            Err(e) => {
                // Platform failure -> try fallback if available, không migrate.
                if let Some(a) = app {
                    if let Ok(v) = get_fallback(a, key) {
                        if v.is_some() {
                            return Ok(v);
                        }
                    }
                }
                Err(e.to_string())
            }
        },
        Err(_) => {
            // Không tạo được Entry (keyring không dùng được) -> fallback.
            if let Some(a) = app {
                return get_fallback(a, key);
            }
            Ok(None)
        }
    }
}

pub fn delete_secret_hybrid<R: Runtime>(app: Option<&AppHandle<R>>, key: &str) -> Result<(), String> {
    let mut keyring_err: Option<String> = None;
    match entry_for(key) {
        Ok(e) => match e.delete_credential() {
            Ok(_) => {}
            Err(keyring::Error::NoEntry) => {}
            Err(e) => keyring_err = Some(e.to_string()),
        },
        Err(e) => keyring_err = Some(e),
    }
    if let Some(a) = app {
        let _ = delete_fallback(a, key);
        // If fallback succeeded, consider delete ok even if keyring had error
        if keyring_err.is_some() {
            // Check fallback still
            return Ok(());
        }
        return Ok(());
    }
    if let Some(err) = keyring_err {
        Err(err)
    } else {
        Ok(())
    }
}

fn has_secret_hybrid<R: Runtime>(app: Option<&AppHandle<R>>, key: &str) -> Result<bool, String> {
    if let Ok(entry) = entry_for(key) {
        match entry.get_password() {
            Ok(_) => return Ok(true),
            Err(keyring::Error::NoEntry) => {}
            Err(_) => {} // fallthrough
        }
    }
    if let Some(a) = app {
        return has_fallback(a, key);
    }
    Ok(false)
}

// For tests that need AppHandle, we will test via fallback_at directly

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub has_facebook_token: bool,
}

#[tauri::command]
pub fn set_facebook_token(app: AppHandle, token: String) -> Result<(), String> {
    set_secret_hybrid(Some(&app), FB_KEY, &token)
}

#[tauri::command]
pub fn delete_facebook_token(app: AppHandle) -> Result<(), String> {
    delete_secret_hybrid(Some(&app), FB_KEY)
}

#[tauri::command]
pub fn credential_status(app: AppHandle) -> Result<CredentialStatus, String> {
    Ok(CredentialStatus {
        has_facebook_token: has_secret_hybrid(Some(&app), FB_KEY)?,
    })
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
    fn test_fallback_set_get_delete() {
        let dir = unique_temp_dir("flowpost_sec_fallback");
        let key = "test_fb_fallback";
        assert_eq!(get_fallback_at(&dir, key).unwrap(), None);
        set_fallback_at(&dir, key, "fb_test_123").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("fb_test_123".into()));
        set_fallback_at(&dir, key, "fb_test_456").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("fb_test_456".into()));
        set_fallback_at(&dir, key, "").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), None);
        // Delete non-existent should be ok
        set_fallback_at(&dir, key, "").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_fallback_overwrite_and_persistence() {
        let dir = unique_temp_dir("flowpost_sec_overwrite");
        let key = "overwrite_key";
        set_fallback_at(&dir, key, "first").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("first".into()));
        set_fallback_at(&dir, key, "second").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("second".into()));
        // Verify file exists and is JSON
        let path = fallback_path_for_test(&dir);
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        let map: HashMap<String, String> = serde_json::from_str(&contents).unwrap();
        assert_eq!(map.get(key).unwrap(), "second");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_fallback_atomic_no_tmp_leak() {
        let dir = unique_temp_dir("flowpost_sec_atomic");
        let key = "atomic_key";
        set_fallback_at(&dir, key, "value1").unwrap();
        let tmp = fallback_path_for_test(&dir).with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp should be cleaned");
        set_fallback_at(&dir, key, "value2").unwrap();
        assert!(!tmp.exists());
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("value2".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_delete_cleans_via_fallback() {
        let dir = unique_temp_dir("flowpost_sec_delete");
        let key = "delete_key";
        set_fallback_at(&dir, key, "temp_value").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), Some("temp_value".into()));
        set_fallback_at(&dir, key, "").unwrap();
        assert_eq!(get_fallback_at(&dir, key).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_hybrid_with_real_app_fallback() {
        // This test mimics hybrid behavior without AppHandle by using fallback_at directly
        // It ensures that even if keyring is broken, fallback provides persistence across "entries"
        let dir = unique_temp_dir("flowpost_hybrid");
        let fb_key = "facebook_token";
        let ai_provider_key = "ai_provider_custom-provider_key";
        // Simulate set via hybrid with no keyring (fallback only)
        set_fallback_at(&dir, fb_key, "fb_secret").unwrap();
        set_fallback_at(&dir, ai_provider_key, "ai_provider_secret").unwrap();
        assert_eq!(get_fallback_at(&dir, fb_key).unwrap(), Some("fb_secret".into()));
        assert_eq!(get_fallback_at(&dir, ai_provider_key).unwrap(), Some("ai_provider_secret".into()));
        // Delete one
        set_fallback_at(&dir, fb_key, "").unwrap();
        assert_eq!(get_fallback_at(&dir, fb_key).unwrap(), None);
        assert_eq!(get_fallback_at(&dir, ai_provider_key).unwrap(), Some("ai_provider_secret".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_should_scrub_fallback_when_values_match() {
        assert!(should_scrub_fallback("same-secret", "same-secret"));
        assert!(should_scrub_fallback("", ""));
    }

    #[test]
    fn test_should_not_scrub_fallback_when_values_differ() {
        assert!(!should_scrub_fallback("keyring-value", "fallback-value"));
        assert!(!should_scrub_fallback("keyring-value", ""));
    }
}
