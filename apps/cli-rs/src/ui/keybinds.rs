pub struct KeybindEntry {
    pub key: &'static str,
    pub description: &'static str,
    pub context: &'static str,
}

pub const KEYBINDS: &[KeybindEntry] = &[
    KeybindEntry { key: "0", description: "Focus preview pane", context: "global" },
    KeybindEntry { key: "1", description: "Focus session list", context: "global" },
    KeybindEntry { key: "j / ↓", description: "Next session / Scroll down", context: "sessions" },
    KeybindEntry { key: "k / ↑", description: "Previous session / Scroll up", context: "sessions" },
    KeybindEntry { key: "h", description: "Collapse group", context: "sessions" },
    KeybindEntry { key: "l", description: "Expand group", context: "sessions" },
    KeybindEntry { key: "o", description: "Switch to tmux pane", context: "global" },
    KeybindEntry { key: "O", description: "Open pane scrollback in popup", context: "global" },
    KeybindEntry { key: "r", description: "Mark session as read", context: "sessions" },
    KeybindEntry { key: "c", description: "Create new session", context: "sessions" },
    KeybindEntry { key: "x", description: "Close session pane", context: "sessions" },
    KeybindEntry { key: "?", description: "Toggle help", context: "global" },
    KeybindEntry { key: "/", description: "Filter keybinds", context: "global" },
    KeybindEntry { key: "q", description: "Quit", context: "global" },
];

pub fn filter_keybinds(query: &str) -> Vec<&KeybindEntry> {
    if query.is_empty() {
        return KEYBINDS.iter().collect();
    }
    let lower = query.to_lowercase();
    KEYBINDS
        .iter()
        .filter(|entry| {
            entry.key.to_lowercase().contains(&lower)
                || entry.description.to_lowercase().contains(&lower)
                || entry.context.to_lowercase().contains(&lower)
        })
        .collect()
}
