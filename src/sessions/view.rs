use super::repo::RepoRoots;
use super::{basename, Session};
use crate::fuzzy::fuzzy_rank;
use crate::time_fmt::{activity_bucket, ActivityBucket};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusFilter {
    All,
    #[default]
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ActivityWindow {
    #[serde(rename = "1d")]
    Day,
    #[serde(rename = "3d")]
    ThreeDays,
    #[default]
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "all")]
    All,
}

impl ActivityWindow {
    pub const ALL: [ActivityWindow; 5] = [
        ActivityWindow::Day,
        ActivityWindow::ThreeDays,
        ActivityWindow::Week,
        ActivityWindow::Month,
        ActivityWindow::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ActivityWindow::Day => "1d",
            ActivityWindow::ThreeDays => "3d",
            ActivityWindow::Week => "7d",
            ActivityWindow::Month => "30d",
            ActivityWindow::All => "All",
        }
    }

    fn seconds(self) -> Option<u64> {
        let days = match self {
            ActivityWindow::Day => 1,
            ActivityWindow::ThreeDays => 3,
            ActivityWindow::Week => 7,
            ActivityWindow::Month => 30,
            ActivityWindow::All => return None,
        };
        Some(days * 86_400)
    }
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 3] = [
        StatusFilter::All,
        StatusFilter::Active,
        StatusFilter::Archived,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All",
            StatusFilter::Active => "Active",
            StatusFilter::Archived => "Archived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    #[default]
    State,
    Folder,
    Date,
}

impl Grouping {
    pub const ALL: [Grouping; 3] = [Grouping::State, Grouping::Folder, Grouping::Date];

    pub fn label(self) -> &'static str {
        match self {
            Grouping::State => "State",
            Grouping::Folder => "Folder",
            Grouping::Date => "Date",
        }
    }

    pub fn next(self) -> Grouping {
        match self {
            Grouping::State => Grouping::Folder,
            Grouping::Folder => Grouping::Date,
            Grouping::Date => Grouping::State,
        }
    }
}

/// Global (not per-instance) view settings, persisted in `state.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewConfig {
    pub status: StatusFilter,
    pub grouping: Grouping,
    pub activity: ActivityWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHeader {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub has_busy: bool,
    pub has_unread: bool,
    pub is_collapsed: bool,
}

#[derive(Debug, Clone)]
pub enum VisibleItem {
    Group(GroupHeader),
    Session(Box<Session>),
}

impl VisibleItem {
    pub fn session(&self) -> Option<&Session> {
        match self {
            VisibleItem::Session(session) => Some(session),
            VisibleItem::Group(_) => None,
        }
    }
}

/// Sessions that pass the Status and Last-activity filters. Live sessions always pass the
/// activity filter: a running session is never "too old" to show.
pub fn filter_sessions<'a>(
    sessions: &'a [Session],
    config: &ViewConfig,
    now: u64,
) -> Vec<&'a Session> {
    sessions
        .iter()
        .filter(|session| {
            let status_ok = match config.status {
                StatusFilter::All => true,
                StatusFilter::Active => session.record.archived_at.is_none(),
                StatusFilter::Archived => session.record.archived_at.is_some(),
            };
            let activity_ok = session.is_live()
                || config.activity.seconds().is_none_or(|window| {
                    now.saturating_sub(session.record.last_activity) <= window
                });
            status_ok && activity_ok
        })
        .collect()
}

pub fn build_items(
    sessions: &[Session],
    config: &ViewConfig,
    collapsed_groups: &HashSet<String>,
    repo_roots: &RepoRoots,
    now: u64,
) -> Vec<VisibleItem> {
    let visible = filter_sessions(sessions, config, now);
    let groups = match config.grouping {
        Grouping::State => group_by_state(visible),
        Grouping::Folder => group_by_folder(visible, repo_roots),
        Grouping::Date => group_by_date(visible, now),
    };

    let mut items = Vec::new();
    for mut group in groups {
        group.sessions.sort_by(|a, b| {
            (Reverse(a.record.last_activity), &a.record.session_id)
                .cmp(&(Reverse(b.record.last_activity), &b.record.session_id))
        });
        let is_collapsed = collapsed_groups.contains(&group.key);
        items.push(VisibleItem::Group(GroupHeader {
            count: group.sessions.len(),
            has_busy: group.sessions.iter().any(|s| s.is_busy()),
            has_unread: group.sessions.iter().any(|s| s.record.unread),
            is_collapsed,
            key: group.key,
            label: group.label,
        }));
        if !is_collapsed {
            items.extend(
                group
                    .sessions
                    .into_iter()
                    .map(|s| VisibleItem::Session(Box::new(s.clone()))),
            );
        }
    }
    items
}

/// Fuzzy search across every session that passes the filters, best match first, flat.
pub fn search_items(
    sessions: &[Session],
    config: &ViewConfig,
    query: &str,
    now: u64,
) -> Vec<VisibleItem> {
    let haystacks = filter_sessions(sessions, config, now)
        .into_iter()
        .map(|session| {
            let haystack = format!("{}/{}", session.folder_name(), session.display_title());
            (session, haystack)
        });
    fuzzy_rank(haystacks, query)
        .into_iter()
        .map(|session| VisibleItem::Session(Box::new(session.clone())))
        .collect()
}

struct Group<'a> {
    key: String,
    label: String,
    sessions: Vec<&'a Session>,
}

fn most_recent(group: &Group) -> u64 {
    group
        .sessions
        .iter()
        .map(|s| s.record.last_activity)
        .max()
        .unwrap_or(0)
}

fn sort_by_recency(groups: &mut [Group]) {
    groups
        .sort_by(|a, b| (Reverse(most_recent(a)), &a.key).cmp(&(Reverse(most_recent(b)), &b.key)));
}

/// First matching group wins, so a busy session that is also unread counts as in progress.
fn group_by_state(sessions: Vec<&Session>) -> Vec<Group<'_>> {
    const GROUPS: [(&str, &str); 4] = [
        ("in-progress", "In progress"),
        ("completed", "Completed"),
        ("need-action", "Need action"),
        ("idle", "Idle"),
    ];
    let mut groups: Vec<Group> = GROUPS
        .iter()
        .map(|(key, label)| Group {
            key: key.to_string(),
            label: label.to_string(),
            sessions: Vec::new(),
        })
        .collect();
    for session in sessions {
        let index = if session.is_busy() {
            0
        } else if session.record.unread {
            1
        } else if session.needs_action() {
            2
        } else {
            3
        };
        groups[index].sessions.push(session);
    }
    groups.retain(|group| !group.sessions.is_empty());
    groups
}

fn group_by_folder<'a>(sessions: Vec<&'a Session>, repo_roots: &'a RepoRoots) -> Vec<Group<'a>> {
    let mut by_path: BTreeMap<&str, Vec<&Session>> = BTreeMap::new();
    for session in sessions {
        by_path
            .entry(repo_roots.group_path(session.record.cwd.as_str()))
            .or_default()
            .push(session);
    }

    let mut basename_counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in by_path.keys() {
        *basename_counts.entry(basename(path)).or_default() += 1;
    }

    let mut groups: Vec<Group> = by_path
        .into_iter()
        .map(|(path, sessions)| {
            let name = basename(path);
            let label = if basename_counts[&name] > 1 {
                last_segments(path, 2)
            } else {
                name
            };
            Group {
                key: path.to_string(),
                label,
                sessions,
            }
        })
        .collect();
    sort_by_recency(&mut groups);
    groups
}

fn group_by_date(sessions: Vec<&Session>, now: u64) -> Vec<Group<'_>> {
    let mut by_bucket: BTreeMap<&'static str, Vec<&Session>> = BTreeMap::new();
    for session in sessions {
        let bucket: ActivityBucket =
            activity_bucket(now as i64, session.record.last_activity as i64);
        by_bucket.entry(bucket.label()).or_default().push(session);
    }
    let mut groups: Vec<Group> = by_bucket
        .into_iter()
        .map(|(label, sessions)| Group {
            key: label.to_string(),
            label: label.to_string(),
            sessions,
        })
        .collect();
    sort_by_recency(&mut groups);
    groups
}

fn last_segments(path: &str, count: usize) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments[segments.len().saturating_sub(count)..].join("/")
}

pub fn identity_matches(a: &VisibleItem, b: &VisibleItem) -> bool {
    match (a, b) {
        (VisibleItem::Group(a), VisibleItem::Group(b)) => a.key == b.key,
        (VisibleItem::Session(a), VisibleItem::Session(b)) => {
            a.record.session_id == b.record.session_id
        }
        _ => false,
    }
}

/// Keeps the cursor on the same session or group after the list is rebuilt.
pub fn resolve_selected_index(
    new_items: &[VisibleItem],
    old_items: &[VisibleItem],
    old_index: usize,
) -> usize {
    if let Some(old_item) = old_items.get(old_index) {
        if let Some(found) = new_items
            .iter()
            .position(|item| identity_matches(item, old_item))
        {
            return found;
        }
    }
    old_index.min(new_items.len().saturating_sub(1))
}

/// Prefers the session in the focused pane, then any session in the focused multiplexer session.
pub fn auto_select_index(
    items: &[VisibleItem],
    focused_pane_id: &str,
    focused_session_name: &str,
) -> usize {
    let live_pane = |item: &VisibleItem| {
        item.session()
            .and_then(|s| s.live.as_ref())
            .map(|live| live.pane.clone())
    };
    items
        .iter()
        .position(|item| live_pane(item).is_some_and(|pane| pane.pane_id == focused_pane_id))
        .or_else(|| {
            items.iter().position(|item| {
                live_pane(item).is_some_and(|pane| pane.session_name() == focused_session_name)
            })
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplexer::PaneRef;
    use crate::sessions::live::{LiveSession, LiveStatus, PromptState};
    use crate::sessions::record::{Origin, SessionRecord};
    use crate::sessions::repo::RepoRoots;

    const NOW: u64 = 10_000_000;
    const DAY: u64 = 86_400;

    fn record(id: &str, cwd: &str, last_activity: u64) -> SessionRecord {
        SessionRecord {
            session_id: id.to_string(),
            cwd: cwd.to_string(),
            title: format!("title {id}"),
            created_at: 0,
            last_activity,
            archived_at: None,
            archive_reason: None,
            archive_changed_at: 0,
            unread: false,
            unread_changed_at: 0,
            origin: Origin::Detected,
        }
    }

    fn closed(id: &str, cwd: &str, last_activity: u64) -> Session {
        Session {
            record: record(id, cwd, last_activity),
            live: None,
        }
    }

    fn live_session(
        id: &str,
        cwd: &str,
        last_activity: u64,
        status: LiveStatus,
        prompt: PromptState,
    ) -> Session {
        Session {
            record: record(id, cwd, last_activity),
            live: Some(LiveSession {
                session_id: id.to_string(),
                pid: 1,
                pane: PaneRef {
                    pane_id: format!("%{id}"),
                    target: "work:0.1".to_string(),
                },
                status,
                prompt_state: prompt,
                name: None,
                derived_name: None,
                cwd: cwd.to_string(),
                started_at: None,
                last_activity: None,
            }),
        }
    }

    fn idle(id: &str, last_activity: u64) -> Session {
        live_session(
            id,
            "/w/app",
            last_activity,
            LiveStatus::Idle,
            PromptState::None,
        )
    }

    fn archived(mut session: Session) -> Session {
        session.record.archived_at = Some(1);
        session
    }

    fn unread(mut session: Session) -> Session {
        session.record.unread = true;
        session
    }

    fn config(grouping: Grouping) -> ViewConfig {
        ViewConfig {
            status: StatusFilter::All,
            grouping,
            activity: ActivityWindow::All,
        }
    }

    fn ids(sessions: Vec<&Session>) -> Vec<String> {
        sessions
            .iter()
            .map(|s| s.record.session_id.clone())
            .collect()
    }

    /// `group label: [session ids]` for each group, in order.
    fn outline(items: &[VisibleItem]) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for item in items {
            match item {
                VisibleItem::Group(group) => out.push((group.label.clone(), Vec::new())),
                VisibleItem::Session(session) => out
                    .last_mut()
                    .unwrap()
                    .1
                    .push(session.record.session_id.clone()),
            }
        }
        out
    }

    fn build(sessions: &[Session], config: ViewConfig) -> Vec<(String, Vec<String>)> {
        outline(&build_items(
            sessions,
            &config,
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        ))
    }

    // -- defaults / serde --

    #[test]
    fn defaults_are_active_state_seven_days() {
        let config = ViewConfig::default();
        assert_eq!(config.status, StatusFilter::Active);
        assert_eq!(config.grouping, Grouping::State);
        assert_eq!(config.activity, ActivityWindow::Week);
    }

    #[test]
    fn view_config_round_trips_and_tolerates_missing_fields() {
        let config = ViewConfig {
            status: StatusFilter::Archived,
            grouping: Grouping::Folder,
            activity: ActivityWindow::ThreeDays,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(
            json,
            r#"{"status":"archived","grouping":"folder","activity":"3d"}"#
        );
        assert_eq!(serde_json::from_str::<ViewConfig>(&json).unwrap(), config);
        assert_eq!(
            serde_json::from_str::<ViewConfig>(r#"{"grouping":"date"}"#).unwrap(),
            ViewConfig {
                grouping: Grouping::Date,
                ..ViewConfig::default()
            }
        );
    }

    #[test]
    fn grouping_cycles_state_folder_date() {
        assert_eq!(Grouping::State.next(), Grouping::Folder);
        assert_eq!(Grouping::Folder.next(), Grouping::Date);
        assert_eq!(Grouping::Date.next(), Grouping::State);
    }

    // -- filters --

    #[test]
    fn status_filter_selects_by_archived() {
        let sessions = vec![idle("live", NOW), archived(closed("old", "/w/app", NOW))];
        let with_status = |status| {
            ids(filter_sessions(
                &sessions,
                &ViewConfig {
                    status,
                    activity: ActivityWindow::All,
                    ..ViewConfig::default()
                },
                NOW,
            ))
        };
        assert_eq!(with_status(StatusFilter::All), vec!["live", "old"]);
        assert_eq!(with_status(StatusFilter::Active), vec!["live"]);
        assert_eq!(with_status(StatusFilter::Archived), vec!["old"]);
    }

    #[test]
    fn activity_window_hides_old_closed_sessions() {
        let sessions = vec![
            closed("fresh", "/w/app", NOW - DAY),
            closed("edge", "/w/app", NOW - 7 * DAY),
            closed("stale", "/w/app", NOW - 7 * DAY - 1),
        ];
        let kept = |activity| {
            ids(filter_sessions(
                &sessions,
                &ViewConfig {
                    status: StatusFilter::All,
                    activity,
                    ..ViewConfig::default()
                },
                NOW,
            ))
        };
        assert_eq!(kept(ActivityWindow::Day), vec!["fresh"]);
        assert_eq!(kept(ActivityWindow::ThreeDays), vec!["fresh"]);
        assert_eq!(kept(ActivityWindow::Week), vec!["fresh", "edge"]);
        assert_eq!(kept(ActivityWindow::Month), vec!["fresh", "edge", "stale"]);
        assert_eq!(kept(ActivityWindow::All), vec!["fresh", "edge", "stale"]);
    }

    #[test]
    fn live_sessions_pass_the_activity_window_however_old() {
        let sessions = vec![idle("live", NOW - 100 * DAY)];
        let config = ViewConfig {
            status: StatusFilter::All,
            activity: ActivityWindow::Day,
            ..ViewConfig::default()
        };
        assert_eq!(ids(filter_sessions(&sessions, &config, NOW)), vec!["live"]);
    }

    #[test]
    fn activity_window_tolerates_future_timestamps() {
        let sessions = vec![closed("future", "/w/app", NOW + 50)];
        let config = ViewConfig {
            status: StatusFilter::All,
            activity: ActivityWindow::Day,
            ..ViewConfig::default()
        };
        assert_eq!(
            ids(filter_sessions(&sessions, &config, NOW)),
            vec!["future"]
        );
    }

    // -- state grouping --

    #[test]
    fn state_grouping_buckets_and_orders_groups() {
        let sessions = vec![
            closed("closed", "/w/app", NOW - 5),
            live_session(
                "busy",
                "/w/app",
                NOW - 4,
                LiveStatus::Busy,
                PromptState::None,
            ),
            unread(idle("done", NOW - 3)),
            live_session(
                "plan",
                "/w/app",
                NOW - 2,
                LiveStatus::Idle,
                PromptState::Plan,
            ),
            live_session("ask", "/w/app", NOW - 1, LiveStatus::Idle, PromptState::Ask),
            idle("quiet", NOW - 6),
        ];
        assert_eq!(
            build(&sessions, config(Grouping::State)),
            vec![
                ("In progress".to_string(), vec!["busy".to_string()]),
                ("Completed".to_string(), vec!["done".to_string()]),
                (
                    "Need action".to_string(),
                    vec!["ask".to_string(), "plan".to_string()]
                ),
                (
                    "Idle".to_string(),
                    vec!["closed".to_string(), "quiet".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn a_session_waiting_on_the_user_needs_action_without_a_pane_prompt() {
        let waiting = live_session(
            "waiting",
            "/w/app",
            NOW,
            LiveStatus::Waiting,
            PromptState::None,
        );
        assert_eq!(
            build(&[waiting, idle("quiet", NOW - 1)], config(Grouping::State)),
            vec![
                ("Need action".to_string(), vec!["waiting".to_string()]),
                ("Idle".to_string(), vec!["quiet".to_string()]),
            ]
        );
    }

    #[test]
    fn state_grouping_first_match_wins() {
        let busy_and_unread = unread(live_session(
            "a",
            "/w/app",
            NOW,
            LiveStatus::Busy,
            PromptState::Ask,
        ));
        let unread_with_prompt = unread(live_session(
            "b",
            "/w/app",
            NOW,
            LiveStatus::Idle,
            PromptState::Plan,
        ));
        assert_eq!(
            build(
                &[busy_and_unread, unread_with_prompt],
                config(Grouping::State)
            ),
            vec![
                ("In progress".to_string(), vec!["a".to_string()]),
                ("Completed".to_string(), vec!["b".to_string()]),
            ]
        );
    }

    #[test]
    fn closed_unread_session_is_completed_and_closed_prompt_is_ignored() {
        let mut closed_unread = unread(closed("a", "/w/app", NOW));
        closed_unread.record.unread = true;
        assert_eq!(
            build(&[closed_unread], config(Grouping::State)),
            vec![("Completed".to_string(), vec!["a".to_string()])]
        );
    }

    #[test]
    fn empty_state_groups_are_hidden() {
        assert!(build(&[], config(Grouping::State)).is_empty());
        assert_eq!(
            build(&[closed("a", "/w/app", NOW)], config(Grouping::State)),
            vec![("Idle".to_string(), vec!["a".to_string()])]
        );
    }

    #[test]
    fn archived_sessions_group_by_the_same_rules() {
        let sessions = vec![archived(closed("a", "/w/app", NOW))];
        assert_eq!(
            build(&sessions, config(Grouping::State)),
            vec![("Idle".to_string(), vec!["a".to_string()])]
        );
    }

    // -- folder grouping --

    #[test]
    fn folder_grouping_groups_by_cwd_ordered_by_recent_activity() {
        let sessions = vec![
            closed("old", "/w/alpha", NOW - 500),
            closed("new", "/w/beta", NOW - 10),
            closed("mid", "/w/alpha", NOW - 100),
        ];
        assert_eq!(
            build(&sessions, config(Grouping::Folder)),
            vec![
                ("beta".to_string(), vec!["new".to_string()]),
                (
                    "alpha".to_string(),
                    vec!["mid".to_string(), "old".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn folder_labels_use_two_segments_only_when_basenames_collide() {
        let sessions = vec![
            closed("a", "/home/me/work/app", NOW - 1),
            closed("b", "/home/me/play/app", NOW - 2),
            closed("c", "/home/me/other", NOW - 3),
        ];
        let items = build_items(
            &sessions,
            &config(Grouping::Folder),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let labels: Vec<String> = items
            .iter()
            .filter_map(|item| match item {
                VisibleItem::Group(group) => Some(group.label.clone()),
                VisibleItem::Session(_) => None,
            })
            .collect();
        assert_eq!(labels, vec!["work/app", "play/app", "other"]);
    }

    #[test]
    fn folder_group_key_is_the_full_cwd() {
        let items = build_items(
            &[closed("a", "/home/me/app", NOW)],
            &config(Grouping::Folder),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let VisibleItem::Group(group) = &items[0] else {
            panic!("expected a group header first");
        };
        assert_eq!(group.key, "/home/me/app");
    }

    #[test]
    fn folder_grouping_merges_worktrees_of_one_repo_and_keeps_non_git_cwds_apart() {
        let sessions = vec![
            closed("a", "/home/me/app", NOW - 1),
            closed("b", "/home/me/.worktrees/app/feature", NOW - 2),
            closed("c", "/home/me/scratch", NOW - 3),
        ];
        let mut repo_roots = RepoRoots::default();
        repo_roots.insert("/home/me/app", "/home/me/app");
        repo_roots.insert("/home/me/.worktrees/app/feature", "/home/me/app");

        let items = build_items(
            &sessions,
            &config(Grouping::Folder),
            &HashSet::new(),
            &repo_roots,
            NOW,
        );
        let outline: Vec<(String, usize)> = items
            .iter()
            .filter_map(|item| match item {
                VisibleItem::Group(group) => Some((group.key.clone(), group.count)),
                VisibleItem::Session(_) => None,
            })
            .collect();
        assert_eq!(
            outline,
            vec![
                ("/home/me/app".to_string(), 2),
                ("/home/me/scratch".to_string(), 1)
            ]
        );
    }

    #[test]
    fn basename_falls_back_to_the_whole_path_at_root() {
        assert_eq!(basename("/"), "/");
        assert_eq!(basename("/a/b"), "b");
        assert_eq!(last_segments("/only", 2), "only");
        assert_eq!(last_segments("/a/b/c", 2), "b/c");
    }

    // -- date grouping --

    #[test]
    fn date_grouping_uses_activity_buckets_newest_first() {
        let sessions = vec![
            closed("older", "/w/app", NOW - 40 * DAY),
            closed("hour", "/w/app", NOW - 1_800),
            closed("now", "/w/app", NOW - 10),
        ];
        assert_eq!(
            build(&sessions, config(Grouping::Date)),
            vec![
                ("Now".to_string(), vec!["now".to_string()]),
                ("Last hour".to_string(), vec!["hour".to_string()]),
                ("Older".to_string(), vec!["older".to_string()]),
            ]
        );
    }

    // -- collapse / header flags --

    #[test]
    fn collapsed_group_keeps_its_header_and_count_but_hides_sessions() {
        let sessions = vec![idle("a", NOW), idle("b", NOW - 1)];
        let collapsed = HashSet::from(["idle".to_string()]);
        let items = build_items(
            &sessions,
            &config(Grouping::State),
            &collapsed,
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(items.len(), 1);
        let VisibleItem::Group(group) = &items[0] else {
            panic!("expected a header");
        };
        assert!(group.is_collapsed);
        assert_eq!(group.count, 2);
    }

    #[test]
    fn collapse_keys_are_per_grouping() {
        let sessions = vec![closed("a", "/w/app", NOW)];
        let collapsed = HashSet::from(["/w/app".to_string()]);
        let by_folder = build_items(
            &sessions,
            &config(Grouping::Folder),
            &collapsed,
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(by_folder.len(), 1);
        let by_state = build_items(
            &sessions,
            &config(Grouping::State),
            &collapsed,
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(by_state.len(), 2);
    }

    #[test]
    fn header_reports_busy_and_unread_members() {
        let sessions = vec![
            live_session("a", "/w/app", NOW, LiveStatus::Busy, PromptState::None),
            unread(closed("b", "/w/app", NOW - 1)),
        ];
        let items = build_items(
            &sessions,
            &config(Grouping::Folder),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let VisibleItem::Group(group) = &items[0] else {
            panic!("expected a header");
        };
        assert!(group.has_busy);
        assert!(group.has_unread);
    }

    #[test]
    fn filters_apply_before_grouping() {
        let sessions = vec![
            closed("stale", "/w/app", NOW - 30 * DAY),
            archived(closed("shelved", "/w/app", NOW)),
            closed("keep", "/w/app", NOW),
        ];
        assert_eq!(
            build(&sessions, ViewConfig::default()),
            vec![("Idle".to_string(), vec!["keep".to_string()])]
        );
    }

    // -- search --

    #[test]
    fn search_is_flat_ranked_and_respects_filters() {
        let mut target = closed("target", "/w/billing", NOW);
        target.record.title = "refactor invoices".to_string();
        let other = closed("other", "/w/app", NOW);
        let hidden = archived({
            let mut s = closed("hidden", "/w/billing", NOW);
            s.record.title = "refactor invoices".to_string();
            s
        });
        let items = search_items(
            &[target, other, hidden],
            &ViewConfig::default(),
            "invoices",
            NOW,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].session().unwrap().record.session_id, "target");
    }

    // -- selection --

    #[test]
    fn selection_follows_the_same_session_across_rebuilds() {
        let before = build_items(
            &[idle("a", NOW), idle("b", NOW - 1)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let after = build_items(
            &[idle("b", NOW), idle("a", NOW - 1)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(resolve_selected_index(&after, &before, 1), 2);
    }

    #[test]
    fn selection_follows_a_group_header_by_key() {
        let before = build_items(
            &[idle("a", NOW)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let after = build_items(
            &[
                live_session("x", "/w/app", NOW, LiveStatus::Busy, PromptState::None),
                idle("a", NOW),
            ],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(resolve_selected_index(&after, &before, 0), 2);
    }

    #[test]
    fn selection_clamps_when_the_item_vanished() {
        let before = build_items(
            &[idle("a", NOW), idle("b", NOW - 1)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        let after = build_items(
            &[idle("a", NOW)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(resolve_selected_index(&after, &before, 2), 1);
        assert_eq!(resolve_selected_index(&[], &before, 2), 0);
    }

    #[test]
    fn auto_select_prefers_focused_pane_then_same_tmux_session() {
        let items = build_items(
            &[idle("a", NOW), idle("b", NOW - 1)],
            &config(Grouping::State),
            &HashSet::new(),
            &RepoRoots::default(),
            NOW,
        );
        assert_eq!(auto_select_index(&items, "%b", "elsewhere"), 2);
        assert_eq!(auto_select_index(&items, "%missing", "work"), 1);
        assert_eq!(auto_select_index(&items, "%missing", "elsewhere"), 0);
    }
}
