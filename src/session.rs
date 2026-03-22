use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptState {
    None,
    Plan,
    Ask,
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

pub fn detect_prompt_state(visible_text: &str) -> PromptState {
    let last_line = visible_text.lines().rev().find(|l| !l.trim().is_empty());
    match last_line {
        Some(line) if line.contains("ctrl-g to edit") => PromptState::Plan,
        Some(line) if line.contains("Enter to select") => PromptState::Ask,
        _ => PromptState::None,
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
        display_name: String,
        is_unread: bool,
    },
    HiddenHeader {
        count: usize,
        is_collapsed: bool,
    },
}

use std::collections::{HashMap, HashSet};
use std::cmp::Ordering;

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
    hidden_pane_ids: &HashSet<String>,
    hidden_groups: &HashSet<String>,
    hidden_section_collapsed: bool,
) -> Vec<VisibleItem> {
    let mut items = Vec::new();
    for group in groups {
        if hidden_groups.contains(&group.session_name) {
            continue;
        }
        let visible_sessions: Vec<&ClaudeSession> = group
            .sessions
            .iter()
            .filter(|s| !hidden_pane_ids.contains(&s.pane_id))
            .collect();
        if visible_sessions.is_empty() {
            continue;
        }
        let has_active = visible_sessions.iter().any(|s| s.status == SessionStatus::Active);
        let has_unread = visible_sessions.iter().any(|s| unread_pane_ids.contains(&s.pane_id));
        let is_collapsed = collapsed_groups.contains(&group.session_name);
        let display_name = display_name_map
            .get(&group.session_name)
            .cloned()
            .unwrap_or_else(|| group.session_name.clone());
        items.push(VisibleItem::GroupHeader {
            session_name: group.session_name.clone(),
            display_name: display_name.clone(),
            session_count: visible_sessions.len(),
            has_active,
            has_unread,
            is_collapsed,
        });
        if !is_collapsed {
            for session in &visible_sessions {
                items.push(VisibleItem::Session {
                    session: (*session).clone(),
                    display_name: display_name.clone(),
                    is_unread: unread_pane_ids.contains(&session.pane_id),
                });
            }
        }
    }

    let mut hidden_items = Vec::new();
    for group in groups {
        if !hidden_groups.contains(&group.session_name) {
            continue;
        }
        let display_name = display_name_map
            .get(&group.session_name)
            .cloned()
            .unwrap_or_else(|| group.session_name.clone());
        let has_active = group.sessions.iter().any(|s| s.status == SessionStatus::Active);
        let has_unread = group.sessions.iter().any(|s| unread_pane_ids.contains(&s.pane_id));
        hidden_items.push(VisibleItem::GroupHeader {
            session_name: group.session_name.clone(),
            display_name: display_name.clone(),
            session_count: group.sessions.len(),
            has_active,
            has_unread,
            is_collapsed: true,
        });
    }
    for group in groups {
        if hidden_groups.contains(&group.session_name) {
            continue;
        }
        let display_name = display_name_map
            .get(&group.session_name)
            .cloned()
            .unwrap_or_else(|| group.session_name.clone());
        for session in &group.sessions {
            if !hidden_pane_ids.contains(&session.pane_id) {
                continue;
            }
            hidden_items.push(VisibleItem::Session {
                session: session.clone(),

                display_name: display_name.clone(),
                is_unread: unread_pane_ids.contains(&session.pane_id),
            });
        }
    }

    if !hidden_items.is_empty() {
        items.push(VisibleItem::HiddenHeader {
            count: hidden_items.len(),
            is_collapsed: hidden_section_collapsed,
        });
        if !hidden_section_collapsed {
            items.extend(hidden_items);
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
            VisibleItem::HiddenHeader { .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::HiddenHeader { .. })
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

fn session_priority_tier(
    session: &ClaudeSession,
    unread_pane_ids: &HashSet<String>,
    prompt_states: &HashMap<String, PromptState>,
) -> u8 {
    let is_unread = unread_pane_ids.contains(&session.pane_id);
    if is_unread {
        let prompt_state = prompt_states.get(&session.pane_id);
        match prompt_state {
            Some(PromptState::Plan) | Some(PromptState::Ask) => 1,
            _ => 0,
        }
    } else if session.status == SessionStatus::Active {
        2
    } else {
        match prompt_states.get(&session.pane_id) {
            Some(PromptState::Plan) | Some(PromptState::Ask) => 3,
            _ => 4,
        }
    }
}

pub fn build_flat_visible_items(
    sessions: &[ClaudeSession],
    unread_pane_ids: &HashSet<String>,
    unread_order: &HashMap<String, u64>,
    prompt_states: &HashMap<String, PromptState>,
    display_name_map: &HashMap<String, String>,
    hidden_pane_ids: &HashSet<String>,
    hidden_groups: &HashSet<String>,
    hidden_section_collapsed: bool,
) -> Vec<VisibleItem> {
    let (hidden_sessions, visible_sessions): (Vec<&ClaudeSession>, Vec<&ClaudeSession>) = sessions
        .iter()
        .partition(|s| hidden_pane_ids.contains(&s.pane_id) || hidden_groups.contains(&s.session_name));

    let mut items: Vec<VisibleItem> = visible_sessions
        .iter()
        .map(|session| {
            let is_unread = unread_pane_ids.contains(&session.pane_id);
            let display_name = display_name_map
                .get(&session.session_name)
                .cloned()
                .unwrap_or_else(|| session.session_name.clone());
            VisibleItem::Session {
                session: (*session).clone(),

                display_name,
                is_unread,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        let session_a = match a {
            VisibleItem::Session { session, .. } => session,
            VisibleItem::GroupHeader { .. } | VisibleItem::HiddenHeader { .. } => return Ordering::Equal,
        };
        let session_b = match b {
            VisibleItem::Session { session, .. } => session,
            VisibleItem::GroupHeader { .. } | VisibleItem::HiddenHeader { .. } => return Ordering::Equal,
        };

        let tier_a = session_priority_tier(session_a, unread_pane_ids, prompt_states);
        let tier_b = session_priority_tier(session_b, unread_pane_ids, prompt_states);

        if tier_a != tier_b {
            return tier_a.cmp(&tier_b);
        }

        // Within tiers 0 and 1, sort by unread_order descending (higher counter = more recent = first)
        if tier_a <= 1 {
            let order_a = unread_order.get(&session_a.pane_id).copied().unwrap_or(0);
            let order_b = unread_order.get(&session_b.pane_id).copied().unwrap_or(0);
            return order_b.cmp(&order_a);
        }

        Ordering::Equal
    });

    if !hidden_sessions.is_empty() {
        items.push(VisibleItem::HiddenHeader {
            count: hidden_sessions.len(),
            is_collapsed: hidden_section_collapsed,
        });
        if !hidden_section_collapsed {
            for session in hidden_sessions {
                let is_unread = unread_pane_ids.contains(&session.pane_id);
                let display_name = display_name_map
                    .get(&session.session_name)
                    .cloned()
                    .unwrap_or_else(|| session.session_name.clone());
                items.push(VisibleItem::Session {
                    session: session.clone(),
    
                    display_name,
                    is_unread,
                });
            }
        }
    }

    items
}
