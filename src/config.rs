use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutDirection {
    #[default]
    Vertical,
    Horizontal,
}

impl<'de> Deserialize<'de> for LayoutDirection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "horizontal" => Ok(LayoutDirection::Horizontal),
            "vertical" => Ok(LayoutDirection::Vertical),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["vertical", "horizontal"],
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    session_name_formatter: Option<String>,
    command: Option<String>,
    default_view: Option<String>,
    layout: Option<LayoutDirection>,
}

pub struct AppConfig {
    pub command: String,
    pub exit_on_switch: bool,
    pub session_name_formatter: Option<Vec<String>>,
    pub default_flat_view: bool,
    pub layout: LayoutDirection,
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
        .map(|s| parse_formatter_command(s));

    let default_flat_view = config_file
        .as_ref()
        .and_then(|c| c.default_view.as_deref())
        .is_some_and(|v| v == "flat");

    let layout = config_file
        .as_ref()
        .and_then(|c| c.layout)
        .unwrap_or_default();

    AppConfig {
        command,
        exit_on_switch,
        session_name_formatter,
        default_flat_view,
        layout,
    }
}

fn load_config_file() -> Option<ConfigFile> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn parse_formatter_command(s: &str) -> Vec<String> {
    let mut parts = s.split_whitespace();
    let Some(cmd) = parts.next() else {
        return Vec::new();
    };
    let exe = expand_tilde(cmd).to_string_lossy().into_owned();
    let mut result = vec![exe];
    result.extend(parts.map(|p| p.to_string()));
    result
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
