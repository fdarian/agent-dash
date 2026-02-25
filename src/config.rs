use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    session_name_formatter: Option<String>,
    command: Option<String>,
    default_view: Option<String>,
}

pub struct AppConfig {
    pub command: String,
    pub exit_on_switch: bool,
    pub session_name_formatter: Option<PathBuf>,
    pub default_flat_view: bool,
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash/config.json")
}

pub fn load_config(exit_on_switch: bool) -> AppConfig {
    let config_file = load_config_file();
    let command = config_file
        .as_ref()
        .and_then(|c| c.command.clone())
        .unwrap_or_else(|| "claude".to_string());
    let session_name_formatter = config_file
        .as_ref()
        .and_then(|c| c.session_name_formatter.as_ref())
        .map(|p| expand_tilde(p));

    let default_flat_view = config_file
        .as_ref()
        .and_then(|c| c.default_view.as_deref())
        .is_some_and(|v| v == "flat");

    AppConfig {
        command,
        exit_on_switch,
        session_name_formatter,
        default_flat_view,
    }
}

fn load_config_file() -> Option<ConfigFile> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .expect("home directory not found")
            .join(rest)
    } else {
        PathBuf::from(path)
    }
}
