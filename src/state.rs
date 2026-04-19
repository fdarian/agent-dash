use crate::session::SessionStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct InstanceState {
    collapsed_groups: Vec<String>,
    hidden_section_collapsed: Option<bool>,
    group_hidden_collapsed: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct PersistedState {
    unread_pane_ids: Vec<String>,
    prev_status_map: HashMap<String, SessionStatus>,
    unread_order: HashMap<String, u64>,
    unread_counter: u64,
    hidden_pane_ids: Vec<String>,
    hidden_groups: Vec<String>,
    per_instance: HashMap<String, InstanceState>,
}

fn state_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash")
}

fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

fn resolve_instance_id(shared_state: bool) -> String {
    if shared_state {
        return "__shared__".to_string();
    }
    match std::env::var("TMUX") {
        Ok(tmux) => tmux.split(',').next().unwrap_or("__default__").to_string(),
        Err(_) => "__default__".to_string(),
    }
}

pub struct LoadedState {
    pub unread_pane_ids: HashSet<String>,
    pub prev_status_map: HashMap<String, SessionStatus>,
    pub unread_order: HashMap<String, u64>,
    pub unread_counter: u64,
    pub hidden_pane_ids: HashSet<String>,
    pub hidden_groups: HashSet<String>,
    pub collapsed_groups: HashSet<String>,
    pub hidden_section_collapsed: bool,
    pub group_hidden_collapsed: HashSet<String>,
}

pub fn load_state(shared_state: bool) -> LoadedState {
    let path = state_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return empty_loaded_state();
        }
    };
    match serde_json::from_str::<PersistedState>(&content) {
        Ok(parsed) => {
            let instance_id = resolve_instance_id(shared_state);
            let instance = parsed.per_instance.get(&instance_id);
            LoadedState {
                unread_pane_ids: parsed.unread_pane_ids.into_iter().collect(),
                prev_status_map: parsed.prev_status_map,
                unread_order: parsed.unread_order,
                unread_counter: parsed.unread_counter,
                hidden_pane_ids: parsed.hidden_pane_ids.into_iter().collect(),
                hidden_groups: parsed.hidden_groups.into_iter().collect(),
                collapsed_groups: instance
                    .map(|i| i.collapsed_groups.iter().cloned().collect())
                    .unwrap_or_default(),
                hidden_section_collapsed: instance
                    .and_then(|i| i.hidden_section_collapsed)
                    .unwrap_or(true),
                group_hidden_collapsed: instance
                    .map(|i| i.group_hidden_collapsed.iter().cloned().collect())
                    .unwrap_or_default(),
            }
        }
        Err(_) => empty_loaded_state(),
    }
}

fn empty_loaded_state() -> LoadedState {
    LoadedState {
        unread_pane_ids: HashSet::new(),
        prev_status_map: HashMap::new(),
        unread_order: HashMap::new(),
        unread_counter: 0,
        hidden_pane_ids: HashSet::new(),
        hidden_groups: HashSet::new(),
        collapsed_groups: HashSet::new(),
        hidden_section_collapsed: true,
        group_hidden_collapsed: HashSet::new(),
    }
}

pub struct SaveArgs<'a> {
    pub unread_pane_ids: &'a HashSet<String>,
    pub prev_status_map: &'a HashMap<String, SessionStatus>,
    pub unread_order: &'a HashMap<String, u64>,
    pub unread_counter: u64,
    pub hidden_pane_ids: &'a HashSet<String>,
    pub hidden_groups: &'a HashSet<String>,
    pub instance: Option<InstanceSaveArgs<'a>>,
    pub shared_state: bool,
}

pub struct InstanceSaveArgs<'a> {
    pub collapsed_groups: &'a HashSet<String>,
    pub hidden_section_collapsed: bool,
    pub group_hidden_collapsed: &'a HashSet<String>,
}

pub fn save_state(args: SaveArgs) {
    let path = state_path();
    let mut persisted: PersistedState = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();

    persisted.unread_pane_ids = args.unread_pane_ids.iter().cloned().collect();
    persisted.prev_status_map = args.prev_status_map.clone();
    persisted.unread_order = args.unread_order.clone();
    persisted.unread_counter = args.unread_counter;
    persisted.hidden_pane_ids = args.hidden_pane_ids.iter().cloned().collect();
    persisted.hidden_groups = args.hidden_groups.iter().cloned().collect();

    if let Some(inst_args) = args.instance {
        let instance_id = resolve_instance_id(args.shared_state);
        let instance = persisted.per_instance.entry(instance_id).or_default();
        instance.collapsed_groups = inst_args.collapsed_groups.iter().cloned().collect();
        instance.hidden_section_collapsed = Some(inst_args.hidden_section_collapsed);
        instance.group_hidden_collapsed =
            inst_args.group_hidden_collapsed.iter().cloned().collect();
    }

    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        state_path(),
        serde_json::to_string(&persisted).unwrap_or_default(),
    );
}
