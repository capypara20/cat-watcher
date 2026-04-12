use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::config::Rule;
use crate::error::AppError;

pub async fn start_watching(rules: &[Rule]) -> Result<(), AppError> {
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(100);

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
        watcher.watch(&path, *mode).map_err(|e| {
            AppError::Watch(format!("watcher 監視登録失敗 ({}): {}", path.display(), e))
        })?;
        println!("監視開始: {} ({:?})", path.display(), mode);
    }

    loop {
        tokio::select! {
            Some(res) = rx.recv() => {
                match res {
                    Ok(event) => {
                        println!("イベント: {:?} - {:?}", event.kind, event.paths);
                    }
                    Err(e) => {
                        eprintln!("watcher エラー: {}", e);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("終了シグナル受信、watcher を停止します...");
                break;
            }
        }
    }
    Ok(())
}
