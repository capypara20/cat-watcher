use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::config::{Global, Rule};
use crate::error::AppError;
use crate::logger::Logger;

pub async fn start_watching(
    rules: &[Rule],
    global: &Global,
    log: Arc<Logger>,
) -> Result<(), AppError> {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>(100);

    let mut watcher = recommended_watcher(move |res| {
        let _ = tx.blocking_send(res);
    })
    .map_err(|e| AppError::Watch(format!("watcher 作成失敗: {}", e)))?;

    let mut watch_map: HashMap<PathBuf, RecursiveMode> = HashMap::new();
    for rule in rules {
        let path = PathBuf::from(&rule.watch.path);
        let mode = if rule.watch.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watch_map
            .entry(path)
            .and_modify(|existing| {
                if mode == RecursiveMode::Recursive {
                    *existing = RecursiveMode::Recursive;
                }
            })
            .or_insert(mode);
    }

    for (path, mode) in &watch_map {
        watcher.watch(path, *mode).map_err(|e| {
            AppError::Watch(format!("watcher 監視登録失敗 ({}): {}", path.display(), e))
        })?;
        log.info(format!("監視開始: {} ({:?})", path.display(), mode));
    }

    let compiled_rules = crate::router::compile_rules(rules)?;
    crate::router::run_router(rx, &compiled_rules, global, Arc::clone(&log)).await?;
    Ok(())
}
