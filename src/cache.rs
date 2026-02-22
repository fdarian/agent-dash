use crate::session::ClaudeSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry<T> {
    value: T,
    stored_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedSessionData {
    pub sessions: Vec<ClaudeSession>,
    pub display_names: HashMap<String, String>,
}

fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash/cache")
}

fn cache_path() -> PathBuf {
    cache_dir().join("0.json")
}

const MAX_AGE_MS: u64 = 365 * 24 * 60 * 60 * 1000;

pub fn load_cached_sessions() -> Option<CachedSessionData> {
    let path = cache_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let entry: CacheEntry<CachedSessionData> = serde_json::from_str(&content).ok()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let age = now.saturating_sub(entry.stored_at);
    if age < MAX_AGE_MS {
        Some(entry.value)
    } else {
        None
    }
}

pub fn save_cached_sessions(data: &CachedSessionData) {
    let dir = cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let entry = CacheEntry {
        value: data,
        stored_at: now,
    };
    let _ = std::fs::write(cache_path(), serde_json::to_string(&entry).unwrap_or_default());
}
