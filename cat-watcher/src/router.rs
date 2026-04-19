use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use notify::EventKind;
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use crate::{config::{ActionConfig, Event, Global, Rule, WatchTarget}, error::AppError};

pub struct CompiledRule{
	pub name: String,
	pub enabled: bool,
	pub watch_path: String,
	pub recursive: bool,
	pub target: WatchTarget,
	pub include_hidden: bool,
	pub events: Vec<Event>,
	pub glob_set: Option<GlobSet>,
	pub exclude_glob_set: Option<GlobSet>,
	pub regexes: Option<Regex>,
	pub actions: Vec<ActionConfig>,
}

pub fn compile_rules(rules: &[Rule]) -> Result<Vec<CompiledRule>, AppError> {
	let mut compiled_rules = Vec::new();
	for rule in rules{
		let glob_set = if let Some(patterns) = &rule.watch.patterns {
			let mut builder = GlobSetBuilder::new();
			for p in patterns {
				builder.add(Glob::new(p).map_err(|e| AppError::Watch(e.to_string()))?);
			}
			Some(builder.build().map_err(|e| AppError::Watch(e.to_string()))?)
		} else {
			None
		};
		// exclude_patterns → GlobSet
		// regex → Regex
		// CompiledRule を生成して compiled_rules に追加
		let exclude_glob_set = if !rule.watch.exclude_patterns.is_empty() {
			let mut builder = GlobSetBuilder::new();
			for p in &rule.watch.exclude_patterns {
				builder.add(Glob::new(p).map_err(|e| AppError::Watch(e.to_string()))?);
			}
			Some(builder.build().map_err(|e| AppError::Watch(e.to_string()))?)
		} else {
			None
		};

		let regexes = if let Some(re_str) = &rule.watch.regex {
			Some(Regex::new(re_str).map_err(|e| AppError::Watch(e.to_string()))?)
		} else {
			None
		};
		
		compiled_rules.push(CompiledRule {
			name: rule.name.clone(),
			enabled: rule.enabled,
			watch_path: rule.watch.path.clone(),
			recursive: rule.watch.recursive,
			target: rule.watch.target.clone(),
			include_hidden: rule.watch.include_hidden,
			events: rule.watch.events.clone(),
			glob_set,
			exclude_glob_set,
			regexes,
			actions: rule.actions.clone(),
		});
	}
	Ok(compiled_rules)
}

fn evaluate_rule(path: &Path, detected_events: &HashSet<Event>, rule: &CompiledRule) -> bool {
	if !rule.enabled { return false; }
    if !matches_target(path, &rule.target) { return false; }
    if !matches_hidden(path, rule.include_hidden) { return false; }

    let file_name = match path.file_name().and_then(|n| n.to_str()) {
		Some(name) => name,
		None => return false, // ファイル名が UTF-8 でない場合はルール不適用
	};

    if !matches_pattern(&file_name, rule) { return false; }
    if !matches_events(detected_events, &rule.events) { return false; }

    true
}

/// target フィルタ: file/directory/both の判定
fn matches_target(path: &Path, target: &WatchTarget) -> bool{
	match target {
		WatchTarget::Both => true, // ターゲットが両方なら常にマッチ
		WatchTarget::File => path.is_file(), // ファイルであればマッチ
		WatchTarget::Directory => path.is_dir(), // ディレクトリであればマッチ
		// TODO: deleteイベント時はファイルが存在しないため判定不可
		// 		 notify の EventKindから指定すれば判定可能かも
		
	}
}

/// include_hidden フィルタ（Phase 12 まではスタブ）
fn matches_hidden(_path: &Path, _include_hidden: bool) -> bool {
    true  // 今は常に通過
}

/// patterns / exclude_patterns / regex マッチ
fn matches_pattern(file_name: &str, rule: &CompiledRule) -> bool{
	if let Some(glob_set) = &rule.glob_set {
		if !glob_set.is_match(file_name) {
			return false;
		}
	}

	if let Some(exclude_glob_set) = &rule.exclude_glob_set {
		if exclude_glob_set.is_match(file_name) {
			return false;
		}
	}

	if let Some(regexes) = &rule.regexes {
		if !regexes.is_match(file_name) {
			return false;
		}
	}

	true
}

/// events 積集合判定
fn matches_events(detected: &HashSet<Event>, rule_events: &[Event]) -> bool {
    rule_events.iter().any(|e| detected.contains(e))
}

fn to_config_event(kind:  &EventKind) -> Option<Event> {
	match kind {
		EventKind::Create(_) => Some(Event::Create),
		EventKind::Modify(_) => Some(Event::Modify),
		EventKind::Remove(_) => Some(Event::Delete),
		_ => None,
	}
}

pub async fn run_router(
    mut rx: mpsc::Receiver<notify::Result<notify::Event>>,
    compiled_rules: &[CompiledRule],
    global: &Global,
) -> Result<(), AppError> {
    // デバウンス用マップ: パス → (イベント集合, 最後の受信時刻)
    let mut pending: HashMap<PathBuf, (HashSet<Event>, Instant)> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            // (A) watcher からイベント受信 → pending に蓄積
            Some(res) = rx.recv() => {
                if let Ok(event) = res {
                    // notify::Event は paths を複数持つことがある
                    // EventKind → config::Event に変換
                    if let Some(config_event) = to_config_event(&event.kind) {
                        for path in &event.paths {
                            let entry = pending
                                .entry(path.clone())
                                .or_insert_with(|| (HashSet::new(), Instant::now()));
                            entry.0.insert(config_event.clone());
                            entry.1 = Instant::now(); // 時刻を更新
                        }
                    }
                }
            }

            // (B) 100ms タイマー → 500ms 経過分を取り出して評価
            _ = interval.tick() => {
                let now = Instant::now();
                // 500ms 経過したエントリを収集
                let ready: Vec<(PathBuf, HashSet<Event>)> = pending.iter()
                    .filter(|(_, (_, last))| now.duration_since(*last) >= Duration::from_millis(500))
                    .map(|(path, (events, _))| (path.clone(), events.clone()))
                    .collect();

                // 収集したエントリを pending から削除して評価
                for (path, detected_events) in ready {
                    pending.remove(&path);
                    // 全ルールに対してフィルタ評価
                    for rule in compiled_rules {
                        if !evaluate_rule(&path, &detected_events, rule) {
                            continue;
                        }
                        println!("マッチ: ルール={}, パス={}, イベント={:?}",
                            rule.name, path.display(), detected_events);

                        let watch_path = PathBuf::from(&rule.watch_path);
                        if let Err(e) = crate::actions::execute_chain(
                            &rule.actions,
                            &path,
                            &watch_path,
                            global,
                        ).await {
                            eprintln!("アクションチェーン実行エラー: ルール={}, パス={}, エラー={}",
                                rule.name, path.display(), e);
                        }
                    }
                }
			}
            // (C) Ctrl+C → 終了
            _ = tokio::signal::ctrl_c() => {
                println!("終了シグナル受信");
                break;
            }
        }
    }
    Ok(())
}