use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use walkdir::WalkDir;

use crate::config::{Global, Rule};
use crate::error::AppError;
use crate::logger::Logger;
use crate::router::EntryKind;

/// ウォッチ対象を walk して EntryKind キャッシュを初期構築する。
/// 監視開始前から存在するファイル/ディレクトリを Delete 時に識別するために使う。
fn build_initial_cache(watch_map: &HashMap<PathBuf, RecursiveMode>) -> HashMap<PathBuf, EntryKind> {
    let mut cache = HashMap::new();
    for (path, mode) in watch_map {
        let max_depth = if *mode == RecursiveMode::Recursive {
            usize::MAX
        } else {
            1
        };
        for entry in WalkDir::new(path)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path().to_path_buf();
            if entry.file_type().is_file() {
                cache.insert(p, EntryKind::File);
            } else if entry.file_type().is_dir() {
                cache.insert(p, EntryKind::Dir);
            }
        }
    }
    cache
}

fn strip_unc_prefix(path: &PathBuf) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

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
        if !rule.enabled {
            continue;
        }

        let path = PathBuf::from(&rule.watch.path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let canonical_display = strip_unc_prefix(&canonical);
        let mode = if rule.watch.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        let pattern_str = if let Some(pats) = &rule.watch.patterns {
            pats.join(", ")
        } else if let Some(re) = &rule.watch.regex {
            format!("regex: {re}")
        } else {
            "*".to_string()
        };

        let events_str = rule
            .watch
            .events
            .iter()
            .map(|e| match e {
                crate::config::Event::Create => "作成",
                crate::config::Event::Modify => "変更",
                crate::config::Event::Delete => "削除",
                crate::config::Event::Rename => "リネーム",
            })
            .collect::<Vec<_>>()
            .join(", ");

        let recursive_str = if rule.watch.recursive { "あり" } else { "なし" };

        log.info(format!(
            "監視ルール [{}]  パス={}  パターン={}  イベント={}  サブフォルダ={}",
            rule.name,
            canonical_display,
            pattern_str,
            events_str,
            recursive_str,
        ));

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
    }

    let initial_cache = build_initial_cache(&watch_map);

    let (compiled_rules, rule_log_handles) = crate::router::compile_rules(rules, global)?;
    crate::router::run_router(rx, &compiled_rules, global, Arc::clone(&log), initial_cache).await?;

    // ルール別ロガーをシャットダウン
    for rule in &compiled_rules {
        if let Some(rl) = &rule.rule_logger {
            rl.shutdown();
        }
    }
    for handle in rule_log_handles {
        let _ = handle.await;
    }
    Ok(())
}
