pub struct KeybindEntry {
    pub key: &'static str,
    pub description: &'static str,
    pub context: &'static str,
}

pub const KEYBINDS: &[KeybindEntry] = &[
    KeybindEntry {
        key: "0",
        description: "Focus preview pane",
        context: "global",
    },
    KeybindEntry {
        key: "1",
        description: "Focus session list",
        context: "global",
    },
    KeybindEntry {
        key: "j / ↓",
        description: "Next session / Scroll down",
        context: "sessions",
    },
    KeybindEntry {
        key: "k / ↑",
        description: "Previous session / Scroll up",
        context: "sessions",
    },
    KeybindEntry {
        key: "h / l",
        description: "Collapse / expand group",
        context: "sessions",
    },
    KeybindEntry {
        key: "H",
        description: "Hide/unhide group",
        context: "sessions",
    },
    KeybindEntry {
        key: "h",
        description: "Hide/unhide session",
        context: "sessions",
    },
    KeybindEntry {
        key: "o",
        description: "Switch to tmux pane",
        context: "global",
    },
    KeybindEntry {
        key: "O",
        description: "Attach session in popup",
        context: "global",
    },
    KeybindEntry {
        key: "r",
        description: "Mark session as read",
        context: "sessions",
    },
    KeybindEntry {
        key: "c",
        description: "Create new session",
        context: "sessions",
    },
    KeybindEntry {
        key: "x",
        description: "Close session pane",
        context: "sessions",
    },
    KeybindEntry {
        key: "+",
        description: "Maximize session list",
        context: "sessions",
    },
    KeybindEntry {
        key: "_",
        description: "Minimize session list",
        context: "sessions",
    },
    KeybindEntry {
        key: "`",
        description: "Toggle flat view",
        context: "sessions",
    },
    KeybindEntry {
        key: "/ ?",
        description: "Search forward / backward",
        context: "preview",
    },
    KeybindEntry {
        key: "?",
        description: "Toggle help",
        context: "global",
    },
    KeybindEntry {
        key: "/",
        description: "Filter keybinds",
        context: "global",
    },
    KeybindEntry {
        key: "v",
        description: "Enter copy mode",
        context: "global",
    },
    KeybindEntry {
        key: "Esc",
        description: "Exit copy mode",
        context: "copy",
    },
    KeybindEntry {
        key: "h j k l",
        description: "Move cursor",
        context: "copy",
    },
    KeybindEntry {
        key: "0",
        description: "Start of line",
        context: "copy",
    },
    KeybindEntry {
        key: "$",
        description: "End of line",
        context: "copy",
    },
    KeybindEntry {
        key: "w",
        description: "Next word",
        context: "copy",
    },
    KeybindEntry {
        key: "e",
        description: "End of word",
        context: "copy",
    },
    KeybindEntry {
        key: "b",
        description: "Previous word",
        context: "copy",
    },
    KeybindEntry {
        key: "H / L",
        description: "Top / bottom of screen",
        context: "copy",
    },
    KeybindEntry {
        key: "gg / G",
        description: "Top / bottom of content",
        context: "copy",
    },
    KeybindEntry {
        key: "zz",
        description: "Center cursor in viewport",
        context: "copy",
    },
    KeybindEntry {
        key: "v",
        description: "Toggle selection",
        context: "copy",
    },
    KeybindEntry {
        key: "y",
        description: "Yank selection",
        context: "copy",
    },
    KeybindEntry {
        key: "/ ? n N",
        description: "Search fwd / bwd / next / prev",
        context: "copy",
    },
    KeybindEntry {
        key: "q",
        description: "Quit",
        context: "global",
    },
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
