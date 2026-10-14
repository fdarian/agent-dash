use crate::ui::theme::ThemeMode;
use anyhow::Context;
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
struct WorktreeConfigFile {
    create: Option<String>,
    checkout: Option<String>,
}

/// Shell templates behind the New session dialog's Worktree checkbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeCommands {
    /// Makes a new branch and worktree; takes `{{name}}` and `{{base}}`.
    pub create: String,
    /// Makes a worktree for an existing branch; takes `{{name}}` only.
    pub checkout: Option<String>,
}

/// A named command the New session dialog can start instead of the top-level `command`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Mode {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    command: Option<String>,
    modes: Option<Vec<Mode>>,
    worktree: Option<WorktreeConfigFile>,
    layout: Option<LayoutDirection>,
    shared_state: Option<bool>,
    theme: Option<ThemeMode>,
    auto_track_sessions: Option<bool>,
    default_tmux_session: Option<String>,
    claude_code: Option<ClaudeCodeConfigFile>,
}

pub struct AppConfig {
    pub command: String,
    /// Extra modes in config order; the dialog puts the implicit "default" (`command`) before them.
    pub modes: Vec<Mode>,
    /// The dialog's worktree option is hidden unless `create` is set.
    pub worktree: Option<WorktreeCommands>,
    pub exit_on_switch: bool,
    pub layout: LayoutDirection,
    pub shared_state: bool,
    pub theme: ThemeMode,
    pub auto_track_sessions: bool,
    /// Where tmux clients go when closing a session removes the last pane of their tmux session.
    pub default_tmux_session: Option<String>,
    pub claude_code_preview_scroll_mode: PreviewScrollMode,
}

impl AppConfig {
    fn defaults(exit_on_switch: bool) -> Self {
        Self {
            command: "claude".to_string(),
            modes: Vec::new(),
            worktree: None,
            exit_on_switch,
            layout: LayoutDirection::default(),
            shared_state: false,
            theme: ThemeMode::Dark,
            auto_track_sessions: true,
            default_tmux_session: None,
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

    if let Some(modes) = config_file.modes.as_ref() {
        config.modes = modes.clone();
    }

    config.worktree = config_file.worktree.as_ref().and_then(|worktree| {
        worktree.create.as_ref().map(|create| WorktreeCommands {
            create: create.clone(),
            checkout: worktree.checkout.clone(),
        })
    });

    if let Some(layout) = config_file.layout {
        config.layout = layout;
    }

    if let Some(shared_state) = config_file.shared_state {
        config.shared_state = shared_state;
    }

    if let Some(theme) = config_file.theme {
        config.theme = theme;
    }

    if let Some(auto_track_sessions) = config_file.auto_track_sessions {
        config.auto_track_sessions = auto_track_sessions;
    }

    config.default_tmux_session = config_file.default_tmux_session.clone();

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
sharedState = true
theme = "light"
"#,
        )
        .unwrap();
        let config = build_config(&config_file, false);

        assert_eq!(config.command, "claude --verbose");
        assert_eq!(config.layout, LayoutDirection::Horizontal);
        assert!(config.shared_state);
        assert_eq!(config.theme, ThemeMode::Light);
    }

    #[test]
    fn auto_track_sessions_defaults_to_true_and_parses_from_toml_and_json() {
        assert!(build_config(&parse_toml("").unwrap(), false).auto_track_sessions);
        let from_toml = parse_toml("autoTrackSessions = false\n").unwrap();
        let from_json = parse_json(r#"{"autoTrackSessions": false}"#).unwrap();
        assert!(!build_config(&from_toml, false).auto_track_sessions);
        assert!(!build_config(&from_json, false).auto_track_sessions);
    }

    #[test]
    fn default_tmux_session_is_unset_by_default_and_parses_from_toml_and_json() {
        assert_eq!(
            build_config(&parse_toml("").unwrap(), false).default_tmux_session,
            None
        );
        let from_toml = parse_toml("defaultTmuxSession = \"main\"\n").unwrap();
        let from_json = parse_json(r#"{"defaultTmuxSession": "main"}"#).unwrap();
        let expected = Some("main".to_string());
        assert_eq!(
            build_config(&from_toml, false).default_tmux_session,
            expected
        );
        assert_eq!(
            build_config(&from_json, false).default_tmux_session,
            expected
        );
    }

    #[test]
    fn worktree_commands_parse_from_toml_and_json() {
        assert_eq!(build_config(&parse_toml("").unwrap(), false).worktree, None);
        let expected = Some(WorktreeCommands {
            create: "git worktree add ../{{name}} -b {{name}} {{base}}".to_string(),
            checkout: Some("git worktree add ../{{name}} {{name}}".to_string()),
        });
        let from_toml = parse_toml(
            r#"
[worktree]
create = "git worktree add ../{{name}} -b {{name}} {{base}}"
checkout = "git worktree add ../{{name}} {{name}}"
"#,
        )
        .unwrap();
        let from_json = parse_json(
            r#"{"worktree": {
                "create": "git worktree add ../{{name}} -b {{name}} {{base}}",
                "checkout": "git worktree add ../{{name}} {{name}}"}}"#,
        )
        .unwrap();
        assert_eq!(build_config(&from_toml, false).worktree, expected);
        assert_eq!(build_config(&from_json, false).worktree, expected);
    }

    #[test]
    fn checkout_is_optional_and_create_is_required_for_the_option() {
        let only_create = parse_toml("[worktree]\ncreate = \"wt\"\n").unwrap();
        assert_eq!(
            build_config(&only_create, false).worktree,
            Some(WorktreeCommands {
                create: "wt".to_string(),
                checkout: None
            })
        );
        let only_checkout = parse_toml("[worktree]\ncheckout = \"wt\"\n").unwrap();
        assert_eq!(build_config(&only_checkout, false).worktree, None);
    }

    #[test]
    fn the_old_worktree_command_key_is_ignored() {
        let config_file = parse_toml("worktreeCommand = \"wt\"\n").unwrap();
        assert_eq!(build_config(&config_file, false).worktree, None);
    }

    #[test]
    fn modes_default_to_none_and_parse_in_order_from_toml_and_json() {
        assert!(build_config(&parse_toml("").unwrap(), false)
            .modes
            .is_empty());
        let expected = vec![
            Mode {
                name: "yolo".to_string(),
                command: "claude --dangerously-skip-permissions".to_string(),
            },
            Mode {
                name: "plan".to_string(),
                command: "claude --permission-mode plan".to_string(),
            },
        ];
        let from_toml = parse_toml(
            r#"
[[modes]]
name = "yolo"
command = "claude --dangerously-skip-permissions"

[[modes]]
name = "plan"
command = "claude --permission-mode plan"
"#,
        )
        .unwrap();
        let from_json = parse_json(
            r#"{"modes": [
                {"name": "yolo", "command": "claude --dangerously-skip-permissions"},
                {"name": "plan", "command": "claude --permission-mode plan"}]}"#,
        )
        .unwrap();
        assert_eq!(build_config(&from_toml, false).modes, expected);
        assert_eq!(build_config(&from_json, false).modes, expected);
    }

    #[test]
    fn a_mode_needs_both_name_and_command() {
        assert!(parse_toml("[[modes]]\nname = \"yolo\"\n").is_err());
        assert!(parse_toml("[[modes]]\ncommand = \"claude\"\n").is_err());
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
    fn removed_grouping_keys_are_ignored() {
        let config_file = parse_toml(
            "defaultView = \"flat\"\ngroupNameSeparator = \"/\"\nsessionNameFormatter = \"fmt\"\n",
        )
        .unwrap();
        assert_eq!(build_config(&config_file, false).command, "claude");
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
