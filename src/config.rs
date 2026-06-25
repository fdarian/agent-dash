use anyhow::Context;
use crate::session::Agent;
use crate::ui::theme::ThemeMode;
use serde::Deserialize;
use std::io::ErrorKind;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutDirection {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewScrollMode {
    #[default]
    Scrollback,
    Virtualized,
}

impl<'de> Deserialize<'de> for PreviewScrollMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "scrollback" => Ok(PreviewScrollMode::Scrollback),
            "virtualized" => Ok(PreviewScrollMode::Virtualized),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["scrollback", "virtualized"],
            )),
        }
    }
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
struct ClaudeCodeConfigFile {
    preview_scroll_mode: Option<PreviewScrollMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    session_name_formatter: Option<String>,
    command: Option<String>,
    default_view: Option<String>,
    layout: Option<LayoutDirection>,
    shared_state: Option<bool>,
    group_name_separator: Option<String>,
    theme: Option<ThemeMode>,
    claude_code: Option<ClaudeCodeConfigFile>,
}

pub struct AppConfig {
    pub command: String,
    pub exit_on_switch: bool,
    pub session_name_formatter: Option<Vec<String>>,
    pub default_flat_view: bool,
    pub layout: LayoutDirection,
    pub shared_state: bool,
    pub group_name_separator: Option<String>,
    pub theme: ThemeMode,
    pub claude_code_preview_scroll_mode: PreviewScrollMode,
}

impl AppConfig {
    pub fn effective_scroll_mode(&self, agent: Agent) -> PreviewScrollMode {
        match agent {
            Agent::Opencode => PreviewScrollMode::Virtualized,
            Agent::Claude => self.claude_code_preview_scroll_mode,
        }
    }

    fn defaults(exit_on_switch: bool) -> Self {
        Self {
            command: "claude".to_string(),
            exit_on_switch,
            session_name_formatter: None,
            default_flat_view: false,
            layout: LayoutDirection::default(),
            shared_state: false,
            group_name_separator: None,
            theme: ThemeMode::Dark,
            claude_code_preview_scroll_mode: PreviewScrollMode::default(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash/config.json")
}

pub fn load_config(exit_on_switch: bool) -> AppConfig {
    let path = config_path();
    match try_load_config(exit_on_switch) {
        Ok(Some(config)) => config,
        Ok(None) => AppConfig::defaults(exit_on_switch),
        Err(err) => {
            eprintln!(
                "agent-dash: failed to load config from {}: {}",
                path.display(),
                err
            );
            AppConfig::defaults(exit_on_switch)
        }
    }
}

pub fn try_load_config(exit_on_switch: bool) -> anyhow::Result<Option<AppConfig>> {
    let config_file = load_config_file()?;
    Ok(config_file
        .as_ref()
        .map(|config_file| build_config(config_file, exit_on_switch)))
}

fn build_config(config_file: &ConfigFile, exit_on_switch: bool) -> AppConfig {
    let mut config = AppConfig::defaults(exit_on_switch);

    if let Some(command) = config_file.command.as_ref() {
        config.command = command.clone();
    }

    config.session_name_formatter = config_file
        .session_name_formatter
        .as_ref()
        .map(|formatter| parse_formatter_command(formatter));

    config.default_flat_view = config_file
        .default_view
        .as_deref()
        .is_some_and(|view| view == "flat");

    if let Some(layout) = config_file.layout {
        config.layout = layout;
    }

    if let Some(shared_state) = config_file.shared_state {
        config.shared_state = shared_state;
    }

    config.group_name_separator = config_file.group_name_separator.clone();

    if let Some(theme) = config_file.theme {
        config.theme = theme;
    }

    if let Some(preview_scroll_mode) = config_file
        .claude_code
        .as_ref()
        .and_then(|claude_code| claude_code.preview_scroll_mode)
    {
        config.claude_code_preview_scroll_mode = preview_scroll_mode;
    }

    config
}

fn load_config_file() -> anyhow::Result<Option<ConfigFile>> {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read {}", path.display()))
        }
    };

    let config_file = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(config_file))
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
