use crate::session::SessionStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct PersistedState {
    unread_pane_ids: Vec<String>,
    prev_status_map: HashMap<String, SessionStatus>,
    unread_order: HashMap<String, u64>,
    unread_counter: u64,
    hidden_pane_ids: Vec<String>,
    hidden_groups: Vec<String>,
}

impl Default for PersistedState {
    fn default() -> Self {
        PersistedState {
            unread_pane_ids: Vec::new(),
            prev_status_map: HashMap::new(),
            unread_order: HashMap::new(),
            unread_counter: 0,
            hidden_pane_ids: Vec::new(),
            hidden_groups: Vec::new(),
        }
    }
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
    pub unread_order: HashMap<String, u64>,
    pub unread_counter: u64,
    pub hidden_pane_ids: HashSet<String>,
    pub hidden_groups: HashSet<String>,
}

pub fn load_state() -> LoadedState {
    let path = state_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return LoadedState {
                unread_pane_ids: HashSet::new(),
                prev_status_map: HashMap::new(),
                unread_order: HashMap::new(),
                unread_counter: 0,
                hidden_pane_ids: HashSet::new(),
                hidden_groups: HashSet::new(),
            };
        }
    };
    match serde_json::from_str::<PersistedState>(&content) {
        Ok(parsed) => LoadedState {
            unread_pane_ids: parsed.unread_pane_ids.into_iter().collect(),
            prev_status_map: parsed.prev_status_map,
            unread_order: parsed.unread_order,
            unread_counter: parsed.unread_counter,
            hidden_pane_ids: parsed.hidden_pane_ids.into_iter().collect(),
            hidden_groups: parsed.hidden_groups.into_iter().collect(),
        },
        Err(_) => LoadedState {
            unread_pane_ids: HashSet::new(),
            prev_status_map: HashMap::new(),
            unread_order: HashMap::new(),
            unread_counter: 0,
            hidden_pane_ids: HashSet::new(),
            hidden_groups: HashSet::new(),
        },
    }
}

pub fn save_state(
    unread_pane_ids: &HashSet<String>,
    prev_status_map: &HashMap<String, SessionStatus>,
    unread_order: &HashMap<String, u64>,
    unread_counter: u64,
    hidden_pane_ids: &HashSet<String>,
    hidden_groups: &HashSet<String>,
) {
    let data = PersistedState {
        unread_pane_ids: unread_pane_ids.iter().cloned().collect(),
        prev_status_map: prev_status_map.clone(),
        unread_order: unread_order.clone(),
        unread_counter,
        hidden_pane_ids: hidden_pane_ids.iter().cloned().collect(),
        hidden_groups: hidden_groups.iter().cloned().collect(),
    };
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(state_path(), serde_json::to_string(&data).unwrap_or_default());
}
