use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
    Opencode,
}

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
pub struct AgentSession {
    pub pane_id: String,
    pub pane_target: String,
    pub title: String,
    #[serde(rename = "sessionName")]
    pub tmux_session_name: String,
    pub status: SessionStatus,
    #[serde(default)]
    pub agent: Agent,
    pub session_id: Option<String>,
}

const BRAILLE_START: u32 = 0x2800;
const BRAILLE_END: u32 = 0x28FF;

pub fn parse_session_status(
    agent: Agent,
    pane_title: &str,
    pane_content: Option<&str>,
) -> SessionStatus {
    match agent {
        Agent::Claude => match pane_title.chars().next() {
            Some(ch) => {
                let code = ch as u32;
                if (BRAILLE_START..=BRAILLE_END).contains(&code) {
                    SessionStatus::Active
                } else {
                    SessionStatus::Idle
                }
            }
            None => SessionStatus::Idle,
        },
        // opencode title is static, so check visible content for "esc interrupt"
        Agent::Opencode => {
            let content = match pane_content {
                Some(c) => c,
                None => return SessionStatus::Idle,
            };
            let busy = content
                .lines()
                .rev()
                .filter(|l| !l.trim().is_empty())
                .take(5)
                .any(|l| l.contains("esc interrupt"));
            if busy {
                SessionStatus::Active
            } else {
                SessionStatus::Idle
            }
        }
    }
}

pub fn detect_prompt_state(agent: Agent, visible_text: &str) -> PromptState {
    if agent == Agent::Opencode {
        return PromptState::None;
    }
    let last_line = visible_text.lines().rev().find(|l| !l.trim().is_empty());
    match last_line {
        Some(line) if line.contains("ctrl-g to edit") => PromptState::Plan,
        Some(line) if line.contains("Enter to select") => PromptState::Ask,
        _ => PromptState::None,
    }
}

// -- Session grouping --

pub struct SessionGroup {
    pub tmux_session_name: String,
    pub sessions: Vec<AgentSession>,
}

#[derive(Debug, Clone)]
pub enum VisibleItem {
    SubgroupHeader {
        prefix: String,
        total_count: usize,
        has_active: bool,
        has_unread: bool,
        is_collapsed: bool,
        in_hidden_section: bool,
    },
    GroupHeader {
        tmux_session_name: String,
        display_name: String,
        session_count: usize,
        has_active: bool,
        has_unread: bool,
        is_collapsed: bool,
        in_subgroup: bool,
        in_hidden_section: bool,
    },
    Session {
        session: AgentSession,
        display_name: String,
        is_unread: bool,
        in_subgroup: bool,
    },
    GroupHiddenHeader {
        tmux_session_name: String,
        count: usize,
        is_collapsed: bool,
    },
    HiddenHeader {
        count: usize,
        is_collapsed: bool,
    },
}

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

pub fn group_sessions_by_name(sessions: &[AgentSession]) -> Vec<SessionGroup> {
    let mut map: indexmap::IndexMap<String, Vec<AgentSession>> = indexmap::IndexMap::new();
    for session in sessions {
        map.entry(session.tmux_session_name.clone())
            .or_default()
            .push(session.clone());
    }
    map.into_iter()
        .map(|(tmux_session_name, sessions)| SessionGroup {
            tmux_session_name,
            sessions,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build_visible_items(
    groups: &[SessionGroup],
    collapsed_groups: &HashSet<String>,
    collapsed_hidden_groups: &HashSet<String>,
    unread_pane_ids: &HashSet<String>,
    unread_order: &HashMap<String, u64>,
    prompt_states: &HashMap<String, PromptState>,
    display_name_map: &HashMap<String, String>,
    hidden_pane_ids: &HashSet<String>,
    hidden_groups: &HashSet<String>,
    hidden_section_collapsed: bool,
    group_hidden_collapsed: &HashSet<String>,
    include_hidden: bool,
    group_name_separator: Option<&str>,
    collapsed_subgroups: &HashSet<String>,
    collapsed_hidden_subgroups: &HashSet<String>,
) -> Vec<VisibleItem> {
    let mut items = Vec::new();
    let mut visible_groups: Vec<(&SessionGroup, Vec<&AgentSession>)> = Vec::new();
    let mut all_hidden_groups: Vec<&SessionGroup> = Vec::new();
    for group in groups {
        if !include_hidden && hidden_groups.contains(&group.tmux_session_name) {
            continue;
        }
        let mut visible_sessions: Vec<&AgentSession> = group
            .sessions
            .iter()
            .filter(|s| include_hidden || !hidden_pane_ids.contains(&s.pane_id))
            .collect();
        if visible_sessions.is_empty() {
            all_hidden_groups.push(group);
            continue;
        }
        visible_sessions.sort_by(|a, b| {
            let tier_a = session_priority_tier(a, unread_pane_ids, prompt_states);
            let tier_b = session_priority_tier(b, unread_pane_ids, prompt_states);
            if tier_a != tier_b {
                return tier_a.cmp(&tier_b);
            }
            if tier_a <= 1 {
                let order_a = unread_order.get(&a.pane_id).copied().unwrap_or(0);
                let order_b = unread_order.get(&b.pane_id).copied().unwrap_or(0);
                return order_b.cmp(&order_a);
            }
            Ordering::Equal
        });
        visible_groups.push((group, visible_sessions));
    }
    visible_groups.sort_by(|a, b| {
        let tier_a =
            a.1.iter()
                .map(|s| session_priority_tier(s, unread_pane_ids, prompt_states))
                .min()
                .unwrap_or(u8::MAX);
        let tier_b =
            b.1.iter()
                .map(|s| session_priority_tier(s, unread_pane_ids, prompt_states))
                .min()
                .unwrap_or(u8::MAX);
        if tier_a != tier_b {
            return tier_a.cmp(&tier_b);
        }
        if tier_a <= 1 {
            let order_a =
                a.1.iter()
                    .filter(|s| unread_pane_ids.contains(&s.pane_id))
                    .map(|s| unread_order.get(&s.pane_id).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
            let order_b =
                b.1.iter()
                    .filter(|s| unread_pane_ids.contains(&s.pane_id))
                    .map(|s| unread_order.get(&s.pane_id).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
            return order_b.cmp(&order_a);
        }
        Ordering::Equal
    });

    emit_groups_with_subgrouping(
        visible_groups,
        &mut items,
        collapsed_groups,
        unread_pane_ids,
        display_name_map,
        hidden_pane_ids,
        group_hidden_collapsed,
        group_name_separator,
        collapsed_subgroups,
        !include_hidden,
        false,
    );

    if !include_hidden {
        let mut hidden_groups_data: Vec<(&SessionGroup, Vec<&AgentSession>)> = Vec::new();
        for group in groups {
            if !hidden_groups.contains(&group.tmux_session_name) {
                continue;
            }
            let sessions: Vec<&AgentSession> = group.sessions.iter().collect();
            hidden_groups_data.push((group, sessions));
        }
        for group in all_hidden_groups {
            let sessions: Vec<&AgentSession> = group.sessions.iter().collect();
            hidden_groups_data.push((group, sessions));
        }

        if !hidden_groups_data.is_empty() {
            let hidden_item_count = hidden_groups_data.len();
            let mut hidden_items = Vec::new();
            emit_groups_with_subgrouping(
                hidden_groups_data,
                &mut hidden_items,
                collapsed_hidden_groups,
                unread_pane_ids,
                display_name_map,
                hidden_pane_ids,
                group_hidden_collapsed,
                group_name_separator,
                collapsed_hidden_subgroups,
                false,
                true,
            );
            items.push(VisibleItem::HiddenHeader {
                count: hidden_item_count,
                is_collapsed: hidden_section_collapsed,
            });
            if !hidden_section_collapsed {
                items.extend(hidden_items);
            }
        }
    }

    items
}

#[allow(clippy::too_many_arguments)]
fn emit_groups_with_subgrouping<'a>(
    groups: Vec<(&'a SessionGroup, Vec<&'a AgentSession>)>,
    items: &mut Vec<VisibleItem>,
    collapsed_groups: &HashSet<String>,
    unread_pane_ids: &HashSet<String>,
    display_name_map: &HashMap<String, String>,
    hidden_pane_ids: &HashSet<String>,
    group_hidden_collapsed: &HashSet<String>,
    group_name_separator: Option<&str>,
    collapsed_subgroups: &HashSet<String>,
    with_hidden_subsection: bool,
    in_hidden_section: bool,
) {
    if let Some(sep) = group_name_separator {
        let mut prefix_map: indexmap::IndexMap<String, Vec<(&SessionGroup, Vec<&AgentSession>)>> =
            indexmap::IndexMap::new();
        for (group, sessions) in groups {
            let display_name = display_name_map
                .get(&group.tmux_session_name)
                .map(String::as_str)
                .unwrap_or(&group.tmux_session_name);
            let prefix = display_name
                .split_once(sep)
                .map(|(p, _)| p.to_string())
                .unwrap_or_else(|| display_name.to_string());
            prefix_map
                .entry(prefix)
                .or_default()
                .push((group, sessions));
        }

        for (prefix, sub_groups) in prefix_map {
            let any_has_sep = sub_groups.iter().any(|(group, _)| {
                let dn = display_name_map
                    .get(&group.tmux_session_name)
                    .map(String::as_str)
                    .unwrap_or(&group.tmux_session_name);
                dn.contains(sep)
            });

            if any_has_sep {
                let total_count: usize = sub_groups.iter().map(|(_, ss)| ss.len()).sum();
                let has_active = sub_groups
                    .iter()
                    .flat_map(|(_, ss)| ss.iter())
                    .any(|s| s.status == SessionStatus::Active);
                let has_unread = sub_groups
                    .iter()
                    .flat_map(|(_, ss)| ss.iter())
                    .any(|s| unread_pane_ids.contains(&s.pane_id));
                let is_collapsed = collapsed_subgroups.contains(&prefix);
                items.push(VisibleItem::SubgroupHeader {
                    prefix: prefix.clone(),
                    total_count,
                    has_active,
                    has_unread,
                    is_collapsed,
                    in_hidden_section,
                });
                if !is_collapsed {
                    for (group, sessions) in sub_groups {
                        let display_name = display_name_map
                            .get(&group.tmux_session_name)
                            .map(String::as_str)
                            .unwrap_or(&group.tmux_session_name);
                        let header_display = display_name
                            .split_once(sep)
                            .map(|(_, suffix)| suffix.to_string())
                            .unwrap_or_else(|| display_name.to_string());
                        emit_single_group(
                            group,
                            &header_display,
                            display_name,
                            sessions,
                            items,
                            collapsed_groups,
                            unread_pane_ids,
                            hidden_pane_ids,
                            group_hidden_collapsed,
                            true,
                            with_hidden_subsection,
                            in_hidden_section,
                        );
                    }
                }
            } else {
                for (group, sessions) in sub_groups {
                    let display_name = display_name_map
                        .get(&group.tmux_session_name)
                        .map(String::as_str)
                        .unwrap_or(&group.tmux_session_name);
                    emit_single_group(
                        group,
                        display_name,
                        display_name,
                        sessions,
                        items,
                        collapsed_groups,
                        unread_pane_ids,
                        hidden_pane_ids,
                        group_hidden_collapsed,
                        false,
                        with_hidden_subsection,
                        in_hidden_section,
                    );
                }
            }
        }
    } else {
        for (group, sessions) in groups {
            let display_name = display_name_map
                .get(&group.tmux_session_name)
                .map(String::as_str)
                .unwrap_or(&group.tmux_session_name);
            emit_single_group(
                group,
                display_name,
                display_name,
                sessions,
                items,
                collapsed_groups,
                unread_pane_ids,
                hidden_pane_ids,
                group_hidden_collapsed,
                false,
                with_hidden_subsection,
                in_hidden_section,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_single_group<'a>(
    group: &'a SessionGroup,
    header_display: &str,
    session_display: &str,
    sessions: Vec<&'a AgentSession>,
    items: &mut Vec<VisibleItem>,
    collapsed_groups: &HashSet<String>,
    unread_pane_ids: &HashSet<String>,
    hidden_pane_ids: &HashSet<String>,
    group_hidden_collapsed: &HashSet<String>,
    in_subgroup: bool,
    with_hidden_subsection: bool,
    in_hidden_section: bool,
) {
    let has_active = sessions.iter().any(|s| s.status == SessionStatus::Active);
    let has_unread = sessions
        .iter()
        .any(|s| unread_pane_ids.contains(&s.pane_id));
    let is_collapsed = collapsed_groups.contains(&group.tmux_session_name);
    items.push(VisibleItem::GroupHeader {
        tmux_session_name: group.tmux_session_name.clone(),
        display_name: header_display.to_string(),
        session_count: sessions.len(),
        has_active,
        has_unread,
        is_collapsed,
        in_subgroup,
        in_hidden_section,
    });
    if !is_collapsed {
        for session in &sessions {
            items.push(VisibleItem::Session {
                session: (*session).clone(),
                display_name: session_display.to_string(),
                is_unread: unread_pane_ids.contains(&session.pane_id),
                in_subgroup,
            });
        }
        if with_hidden_subsection {
            let hidden_in_group: Vec<&AgentSession> = group
                .sessions
                .iter()
                .filter(|s| hidden_pane_ids.contains(&s.pane_id))
                .collect();
            if !hidden_in_group.is_empty() {
                let is_section_collapsed =
                    group_hidden_collapsed.contains(&group.tmux_session_name);
                items.push(VisibleItem::GroupHiddenHeader {
                    tmux_session_name: group.tmux_session_name.clone(),
                    count: hidden_in_group.len(),
                    is_collapsed: is_section_collapsed,
                });
                if !is_section_collapsed {
                    for session in &hidden_in_group {
                        items.push(VisibleItem::Session {
                            session: (*session).clone(),
                            display_name: session_display.to_string(),
                            is_unread: unread_pane_ids.contains(&session.pane_id),
                            in_subgroup,
                        });
                    }
                }
            }
        }
    }
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
            VisibleItem::GroupHeader { tmux_session_name, .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::GroupHeader { tmux_session_name: name, .. } if name == tmux_session_name)
                }) {
                    return found;
                }
            }
            VisibleItem::GroupHiddenHeader { tmux_session_name, .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::GroupHiddenHeader { tmux_session_name: name, .. } if name == tmux_session_name)
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
            VisibleItem::SubgroupHeader { prefix, .. } => {
                if let Some(found) = new_items.iter().position(|item| {
                    matches!(item, VisibleItem::SubgroupHeader { prefix: p, .. } if p == prefix)
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
    focused_tmux_session_name: &str,
) -> usize {
    if let Some(idx) = visible_items.iter().position(|item| {
        matches!(item, VisibleItem::Session { session, .. } if session.pane_id == focused_pane_id)
    }) {
        return idx;
    }
    if let Some(idx) = visible_items.iter().position(|item| {
        matches!(item, VisibleItem::Session { session, .. } if session.tmux_session_name == focused_tmux_session_name)
    }) {
        return idx;
    }
    0
}

fn session_priority_tier(
    session: &AgentSession,
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

#[allow(clippy::too_many_arguments)]
pub fn build_flat_visible_items(
    sessions: &[AgentSession],
    unread_pane_ids: &HashSet<String>,
    unread_order: &HashMap<String, u64>,
    prompt_states: &HashMap<String, PromptState>,
    display_name_map: &HashMap<String, String>,
    hidden_pane_ids: &HashSet<String>,
    hidden_groups: &HashSet<String>,
    hidden_section_collapsed: bool,
    include_hidden: bool,
) -> Vec<VisibleItem> {
    let (hidden_sessions, visible_sessions): (Vec<&AgentSession>, Vec<&AgentSession>) =
        sessions.iter().partition(|s| {
            !include_hidden
                && (hidden_pane_ids.contains(&s.pane_id)
                    || hidden_groups.contains(&s.tmux_session_name))
        });

    let mut items: Vec<VisibleItem> = visible_sessions
        .iter()
        .map(|session| {
            let is_unread = unread_pane_ids.contains(&session.pane_id);
            let display_name = display_name_map
                .get(&session.tmux_session_name)
                .cloned()
                .unwrap_or_else(|| session.tmux_session_name.clone());
            VisibleItem::Session {
                session: (*session).clone(),
                display_name,
                is_unread,
                in_subgroup: false,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        let session_a = match a {
            VisibleItem::Session { session, .. } => session,
            VisibleItem::SubgroupHeader { .. }
            | VisibleItem::GroupHeader { .. }
            | VisibleItem::GroupHiddenHeader { .. }
            | VisibleItem::HiddenHeader { .. } => return Ordering::Equal,
        };
        let session_b = match b {
            VisibleItem::Session { session, .. } => session,
            VisibleItem::SubgroupHeader { .. }
            | VisibleItem::GroupHeader { .. }
            | VisibleItem::GroupHiddenHeader { .. }
            | VisibleItem::HiddenHeader { .. } => return Ordering::Equal,
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
                    .get(&session.tmux_session_name)
                    .cloned()
                    .unwrap_or_else(|| session.tmux_session_name.clone());
                items.push(VisibleItem::Session {
                    session: session.clone(),
                    display_name,
                    is_unread,
                    in_subgroup: false,
                });
            }
        }
    }

    items
}
