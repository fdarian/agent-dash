use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSession {
    pub pane_id: String,
    pub pane_target: String,
    pub title: String,
    pub session_name: String,
    pub status: SessionStatus,
}

const BRAILLE_START: u32 = 0x2800;
const BRAILLE_END: u32 = 0x28FF;

pub fn parse_session_status(pane_title: &str) -> SessionStatus {
    match pane_title.chars().next() {
        Some(ch) => {
            let code = ch as u32;
            if (BRAILLE_START..=BRAILLE_END).contains(&code) {
                SessionStatus::Active
            } else {
                SessionStatus::Idle
            }
        }
        None => SessionStatus::Idle,
    }
}

// -- Session grouping --

pub struct SessionGroup {
    pub session_name: String,
    pub sessions: Vec<ClaudeSession>,
}

#[derive(Debug, Clone)]
pub enum VisibleItem {
    GroupHeader {
        session_name: String,
        display_name: String,
        session_count: usize,
        has_active: bool,
        has_unread: bool,
        is_collapsed: bool,
    },
    Session {
        session: ClaudeSession,
        group_session_name: String,
        display_name: String,
        is_unread: bool,
    },
}

use std::collections::{HashMap, HashSet};

pub fn group_sessions_by_name(sessions: &[ClaudeSession]) -> Vec<SessionGroup> {
    let mut map: indexmap::IndexMap<String, Vec<ClaudeSession>> = indexmap::IndexMap::new();
    for session in sessions {
        map.entry(session.session_name.clone())
            .or_default()
            .push(session.clone());
    }
    map.into_iter()
        .map(|(session_name, sessions)| SessionGroup {
            session_name,
            sessions,
        })
        .collect()
}

pub fn build_visible_items(
    groups: &[SessionGroup],
    collapsed_groups: &HashSet<String>,
    unread_pane_ids: &HashSet<String>,
    display_name_map: &HashMap<String, String>,
) -> Vec<VisibleItem> {
    let mut items = Vec::new();
    for group in groups {
        let has_active = group.sessions.iter().any(|s| s.status == SessionStatus::Active);
        let has_unread = group.sessions.iter().any(|s| unread_pane_ids.contains(&s.pane_id));
        let is_collapsed = collapsed_groups.contains(&group.session_name);
        let display_name = display_name_map
            .get(&group.session_name)
            .cloned()
            .unwrap_or_else(|| group.session_name.clone());
        items.push(VisibleItem::GroupHeader {
            session_name: group.session_name.clone(),
            display_name: display_name.clone(),
            session_count: group.sessions.len(),
            has_active,
            has_unread,
            is_collapsed,
        });
        if !is_collapsed {
            for session in &group.sessions {
                items.push(VisibleItem::Session {
                    session: session.clone(),
                    group_session_name: group.session_name.clone(),
                    display_name: display_name.clone(),
                    is_unread: unread_pane_ids.contains(&session.pane_id),
                });
            }
        }
    }
    items
}

pub fn resolve_selected_index(
    new_items: &[VisibleItem],
    old_items: &[VisibleItem],
    old_index: usize,
) -> usize {
    if let Some(old_item) = old_items.get(old_index) {
        match old_item {
            VisibleItem::Session { session, .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::Session { session: s, .. } if s.pane_id == session.pane_id)
                }) {
                    return found;
                }
            }
            VisibleItem::GroupHeader { session_name, .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::GroupHeader { session_name: name, .. } if name == session_name)
                }) {
                    return found;
                }
            }
        }
    }
    if new_items.is_empty() {
        0
    } else {
        old_index.min(new_items.len() - 1)
    }
}

pub fn auto_select_index(
    visible_items: &[VisibleItem],
    focused_pane_id: &str,
    focused_session_name: &str,
) -> usize {
    // Priority 1: focused pane is itself an agent session
    if let Some(idx) = visible_items.iter().position(|item| {
        matches!(item, VisibleItem::Session { session, .. } if session.pane_id == focused_pane_id)
    }) {
        return idx;
    }
    // Priority 2: first agent session in the focused tmux session
    if let Some(idx) = visible_items.iter().position(|item| {
        matches!(item, VisibleItem::Session { session, .. } if session.session_name == focused_session_name)
    }) {
        return idx;
    }
    // Priority 3: any first agent session (default)
    0
}
