use anyhow::Context;
use crate::session::Agent;
use crate::ui::theme::ThemeMode;
use serde::Deserialize;
use std::path::{Path, PathBuf};

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

pub const TOML_CONFIG_FILE_NAME: &str = "config.toml";
pub const JSON_CONFIG_FILE_NAME: &str = "config.json";

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash")
}

/// `config.toml` wins over `config.json` when both exist.
pub fn existing_config_file() -> Option<PathBuf> {
    let dir = config_dir();
    [TOML_CONFIG_FILE_NAME, JSON_CONFIG_FILE_NAME]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// The config file in use, or the config directory when there is none.
pub fn config_location() -> PathBuf {
    existing_config_file().unwrap_or_else(config_dir)
}

pub fn load_config(exit_on_switch: bool) -> AppConfig {
    let path = config_location();
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
    let Some(path) = existing_config_file() else {
        return Ok(None);
    };
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let config_file = parse_config_file(&path, &content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(config_file))
}

fn parse_config_file(path: &Path, content: &str) -> anyhow::Result<ConfigFile> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("toml") => Ok(toml::from_str(content)?),
        Some("json") => Ok(serde_json::from_str(content)?),
        _ => anyhow::bail!("unsupported config file extension"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_toml(content: &str) -> anyhow::Result<ConfigFile> {
        parse_config_file(Path::new("config.toml"), content)
    }

    fn parse_json(content: &str) -> anyhow::Result<ConfigFile> {
        parse_config_file(Path::new("config.json"), content)
    }

    #[test]
    fn parses_toml_top_level_keys() {
        let config_file = parse_toml(
            r#"
command = "claude --verbose"
layout = "horizontal"
defaultView = "flat"
sharedState = true
groupNameSeparator = "/"
theme = "light"
sessionNameFormatter = "~/bin/format"
"#,
        )
        .unwrap();
        let config = build_config(&config_file, false);

        assert_eq!(config.command, "claude --verbose");
        assert_eq!(config.layout, LayoutDirection::Horizontal);
        assert!(config.default_flat_view);
        assert!(config.shared_state);
        assert_eq!(config.group_name_separator.as_deref(), Some("/"));
        assert_eq!(config.theme, ThemeMode::Light);
        assert!(config.session_name_formatter.is_some());
    }

    #[test]
    fn parses_toml_claude_code_table() {
        let config_file = parse_toml(
            r#"
[claudeCode]
previewScrollMode = "virtualized"
"#,
        )
        .unwrap();
        let config = build_config(&config_file, false);

        assert_eq!(
            config.claude_code_preview_scroll_mode,
            PreviewScrollMode::Virtualized
        );
    }

    #[test]
    fn toml_and_json_produce_the_same_config() {
        let from_toml = parse_toml(
            "layout = \"horizontal\"\n[claudeCode]\npreviewScrollMode = \"virtualized\"\n",
        )
        .unwrap();
        let from_json = parse_json(
            r#"{"layout": "horizontal", "claudeCode": {"previewScrollMode": "virtualized"}}"#,
        )
        .unwrap();
        let toml_config = build_config(&from_toml, false);
        let json_config = build_config(&from_json, false);

        assert_eq!(toml_config.layout, json_config.layout);
        assert_eq!(
            toml_config.claude_code_preview_scroll_mode,
            json_config.claude_code_preview_scroll_mode
        );
    }

    #[test]
    fn empty_toml_yields_defaults() {
        let config_file = parse_toml("").unwrap();
        let config = build_config(&config_file, false);

        assert_eq!(config.command, "claude");
        assert_eq!(config.layout, LayoutDirection::Vertical);
    }

    #[test]
    fn rejects_invalid_toml_enum_value() {
        assert!(parse_toml("layout = \"diagonal\"").is_err());
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse_toml("layout = ").is_err());
    }

    #[test]
    fn rejects_unknown_extension() {
        assert!(parse_config_file(Path::new("config.yaml"), "").is_err());
    }
}
