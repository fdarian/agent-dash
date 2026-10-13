use std::ffi::OsStr;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::Message;
use crate::config::{JSON_CONFIG_FILE_NAME, TOML_CONFIG_FILE_NAME};

pub fn spawn(tx: UnboundedSender<Message>) -> notify::Result<RecommendedWatcher> {
    let config_dir = crate::config::config_dir();

    let mut watcher =
        notify::recommended_watcher(move |result: notify::Result<Event>| match result {
            Ok(event) if is_relevant_event(&event) => {
                let _ = tx.send(Message::ConfigChanged);
            }
            Ok(_) => {}
            Err(err) => eprintln!("agent-dash: config watcher error: {}", err),
        })?;

    watcher.watch(&config_dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

fn is_relevant_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event
        .paths
        .iter()
        .any(|path| path.file_name().is_some_and(is_config_file_name))
}

fn is_config_file_name(name: &OsStr) -> bool {
    name == TOML_CONFIG_FILE_NAME || name == JSON_CONFIG_FILE_NAME
}
