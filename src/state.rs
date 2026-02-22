use crate::session::SessionStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    unread_pane_ids: Vec<String>,
    prev_status_map: HashMap<String, SessionStatus>,
}

fn state_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash")
}

fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

pub struct LoadedState {
    pub unread_pane_ids: HashSet<String>,
    pub prev_status_map: HashMap<String, SessionStatus>,
}

pub fn load_state() -> LoadedState {
    let path = state_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return LoadedState {
                unread_pane_ids: HashSet::new(),
                prev_status_map: HashMap::new(),
            };
        }
    };
    match serde_json::from_str::<PersistedState>(&content) {
        Ok(parsed) => LoadedState {
            unread_pane_ids: parsed.unread_pane_ids.into_iter().collect(),
            prev_status_map: parsed.prev_status_map,
        },
        Err(_) => LoadedState {
            unread_pane_ids: HashSet::new(),
            prev_status_map: HashMap::new(),
        },
    }
}

pub fn save_state(unread_pane_ids: &HashSet<String>, prev_status_map: &HashMap<String, SessionStatus>) {
    let data = PersistedState {
        unread_pane_ids: unread_pane_ids.iter().cloned().collect(),
        prev_status_map: prev_status_map.clone(),
    };
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(state_path(), serde_json::to_string(&data).unwrap_or_default());
}
