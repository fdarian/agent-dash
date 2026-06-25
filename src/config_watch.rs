use std::ffi::OsStr;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::Message;

pub fn spawn(tx: UnboundedSender<Message>) -> notify::Result<RecommendedWatcher> {
    let config_path = crate::config::config_path();
    let config_dir = config_path
        .parent()
        .ok_or_else(|| notify::Error::generic("config path has no parent directory"))?;
    let config_file_name = config_path
        .file_name()
        .ok_or_else(|| notify::Error::generic("config path has no file name"))?
        .to_owned();

    let mut watcher =
        notify::recommended_watcher(move |result: notify::Result<Event>| match result {
            Ok(event) if is_relevant_event(&event, &config_file_name) => {
                let _ = tx.send(Message::ConfigChanged);
            }
            Ok(_) => {}
            Err(err) => eprintln!("agent-dash: config watcher error: {}", err),
        })?;

    watcher.watch(config_dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

fn is_relevant_event(event: &Event, config_file_name: &OsStr) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event
        .paths
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == config_file_name))
}
