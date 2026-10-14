use crate::sessions::view::ViewConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct InstanceState {
    collapsed_groups: Vec<String>,
    /// Keys this version doesn't know, kept so that other versions sharing the file don't strip
    /// each other's fields.
    #[serde(flatten)]
    unknown: Map<String, Value>,
}

/// Every struct here carries its unknown keys through a load and save, including the old
/// unread/hidden/subgroup state that earlier versions wrote.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct PersistedState {
    /// Kept raw so that unknown keys inside it survive; `ViewConfig` is overlaid on save.
    view: Map<String, Value>,
    per_instance: HashMap<String, InstanceState>,
    new_session_worktree: bool,
    #[serde(flatten)]
    unknown: Map<String, Value>,
}

fn state_path() -> PathBuf {
    crate::config::config_dir().join("state.json")
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

/// UI-only state that outlives a run.
pub struct UiState {
    /// Global: shared by every agent-dash instance.
    pub view: ViewConfig,
    /// Per tmux client unless `sharedState` is set.
    pub collapsed_groups: HashSet<String>,
    /// Global: the New session dialog's Worktree checkbox as of the last submit.
    pub new_session_worktree: bool,
}

fn read_persisted(path: &Path) -> Result<Option<PersistedState>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    let parsed = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(parsed))
}

pub fn load_state(shared_state: bool) -> Result<UiState> {
    load_from(&state_path(), &resolve_instance_id(shared_state))
}

pub fn save_state(ui: &UiState, shared_state: bool) -> Result<()> {
    save_to(&state_path(), &resolve_instance_id(shared_state), ui)
}

fn load_from(path: &Path, instance_id: &str) -> Result<UiState> {
    let Some(persisted) = read_persisted(path)? else {
        return Ok(UiState {
            view: ViewConfig::default(),
            collapsed_groups: HashSet::new(),
            new_session_worktree: false,
        });
    };
    let collapsed_groups = persisted
        .per_instance
        .get(instance_id)
        .map(|instance| instance.collapsed_groups.iter().cloned().collect())
        .unwrap_or_default();
    let view = serde_json::from_value(Value::Object(persisted.view))
        .with_context(|| format!("failed to parse the view config in {}", path.display()))?;
    Ok(UiState {
        view,
        collapsed_groups,
        new_session_worktree: persisted.new_session_worktree,
    })
}

fn save_to(path: &Path, instance_id: &str, ui: &UiState) -> Result<()> {
    let mut persisted = read_persisted(path)?.unwrap_or_default();
    let Value::Object(view) = serde_json::to_value(ui.view)? else {
        unreachable!("ViewConfig serializes to an object");
    };
    persisted.view.extend(view);
    let instance = persisted
        .per_instance
        .entry(instance_id.to_string())
        .or_default();
    let mut collapsed: Vec<String> = ui.collapsed_groups.iter().cloned().collect();
    collapsed.sort();
    instance.collapsed_groups = collapsed;
    persisted.new_session_worktree = ui.new_session_worktree;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }
    std::fs::write(path, serde_json::to_string(&persisted)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::record::tests::TempDir;
    use crate::sessions::view::{ActivityWindow, Grouping, StatusFilter};

    fn ui(collapsed: &[&str]) -> UiState {
        UiState {
            view: ViewConfig {
                status: StatusFilter::All,
                grouping: Grouping::Folder,
                activity: ActivityWindow::Month,
            },
            collapsed_groups: collapsed.iter().map(|g| g.to_string()).collect(),
            new_session_worktree: false,
        }
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn view_config_and_collapsed_groups_round_trip() {
        let dir = TempDir::new("state-round-trip");
        let path = dir.0.join("state.json");
        save_to(&path, "inst-a", &ui(&["folder:api", "folder:web"])).unwrap();

        let loaded = load_from(&path, "inst-a").unwrap();
        assert_eq!(loaded.view, ui(&[]).view);
        assert_eq!(
            loaded.collapsed_groups,
            HashSet::from(["folder:api".to_string(), "folder:web".to_string()])
        );
    }

    #[test]
    fn collapsed_groups_are_per_instance_and_view_is_shared() {
        let dir = TempDir::new("state-instances");
        let path = dir.0.join("state.json");
        save_to(&path, "inst-a", &ui(&["a"])).unwrap();
        save_to(&path, "inst-b", &ui(&["b"])).unwrap();

        let a = load_from(&path, "inst-a").unwrap();
        assert_eq!(a.collapsed_groups, HashSet::from(["a".to_string()]));
        assert_eq!(a.view, ui(&[]).view);
        let b = load_from(&path, "inst-b").unwrap();
        assert_eq!(b.collapsed_groups, HashSet::from(["b".to_string()]));
        assert!(load_from(&path, "inst-c")
            .unwrap()
            .collapsed_groups
            .is_empty());
    }

    #[test]
    fn new_session_worktree_is_global_and_defaults_to_false() {
        let dir = TempDir::new("state-new-session-worktree");
        let path = dir.0.join("state.json");
        assert!(!load_from(&path, "inst-a").unwrap().new_session_worktree);

        let mut checked = ui(&[]);
        checked.new_session_worktree = true;
        save_to(&path, "inst-a", &checked).unwrap();
        assert_eq!(
            read_json(&path)["newSessionWorktree"],
            serde_json::json!(true)
        );
        assert!(load_from(&path, "inst-a").unwrap().new_session_worktree);
        assert!(load_from(&path, "inst-b").unwrap().new_session_worktree);

        save_to(&path, "inst-b", &ui(&[])).unwrap();
        assert!(!load_from(&path, "inst-a").unwrap().new_session_worktree);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = TempDir::new("state-missing");
        let loaded = load_from(&dir.0.join("state.json"), "inst-a").unwrap();
        assert_eq!(loaded.view, ViewConfig::default());
        assert!(loaded.collapsed_groups.is_empty());
        assert!(!loaded.new_session_worktree);
    }

    #[test]
    fn save_keeps_keys_this_version_does_not_know() {
        let dir = TempDir::new("state-unknown-keys");
        let path = dir.0.join("state.json");
        std::fs::write(
            &path,
            r#"{"unreadPaneIds":["%1"],"futureTopLevel":{"x":1},
                "view":{"status":"archived","futureViewKey":true},
                "perInstance":{
                    "inst-a":{"collapsedGroups":["old"],"hiddenSectionCollapsed":true},
                    "inst-other":{"collapsedGroups":["keep"]}}}"#,
        )
        .unwrap();

        save_to(&path, "inst-a", &ui(&["new"])).unwrap();

        let saved = read_json(&path);
        assert_eq!(saved["unreadPaneIds"], serde_json::json!(["%1"]));
        assert_eq!(saved["futureTopLevel"], serde_json::json!({"x": 1}));
        assert_eq!(saved["view"]["futureViewKey"], serde_json::json!(true));
        assert_eq!(saved["view"]["status"], "all");
        assert_eq!(saved["view"]["grouping"], "folder");
        assert_eq!(
            saved["perInstance"]["inst-a"]["hiddenSectionCollapsed"],
            serde_json::json!(true)
        );
        assert_eq!(
            saved["perInstance"]["inst-a"]["collapsedGroups"],
            serde_json::json!(["new"])
        );
        assert_eq!(
            saved["perInstance"]["inst-other"]["collapsedGroups"],
            serde_json::json!(["keep"])
        );
    }

    #[test]
    fn old_state_files_load_with_unknown_fields_ignored() {
        let dir = TempDir::new("state-old");
        let path = dir.0.join("state.json");
        std::fs::write(
            &path,
            r#"{"unreadPaneIds":["%1"],"prevStatusMap":{"%1":"active"},"unreadOrder":{},
                "unreadCounter":3,"hiddenPaneIds":[],"hiddenGroups":["x"],
                "perInstance":{"__default__":{"collapsedGroups":["a"],"hiddenSectionCollapsed":true}}}"#,
        )
        .unwrap();
        let loaded = load_from(&path, "__default__").unwrap();
        assert_eq!(loaded.view, ViewConfig::default());
        assert_eq!(loaded.collapsed_groups, HashSet::from(["a".to_string()]));
    }
}
