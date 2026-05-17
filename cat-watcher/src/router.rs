use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use notify::EventKind;
use notify::event::{CreateKind, RemoveKind};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use crate::{config::{ActionConfig, Event, Global, Rule, WatchTarget}, error::AppError};
use crate::logger::Logger;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// notify のイベントサブタイプから EntryKind を取得する。
/// Linux は File/Folder が明確に来る。Windows は Any なので None を返す。
fn kind_from_notify_event(event: &notify::Event) -> Option<EntryKind> {
    match &event.kind {
        EventKind::Create(CreateKind::File) | EventKind::Remove(RemoveKind::File) => {
            Some(EntryKind::File)
        }
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => {
            Some(EntryKind::Dir)
        }
        _ => None,
    }
}

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

fn evaluate_rule(
    path: &Path,
    detected_events: &HashSet<Event>,
    kind: Option<EntryKind>,
    rule: &CompiledRule,
) -> bool {
	if !rule.enabled { return false; }
    if !matches_target(path, &rule.target, kind) { return false; }
    if !matches_hidden(path, rule.include_hidden) { return false; }

    let watch_path = Path::new(&rule.watch_path);
    if rule.recursive {
        if !path.starts_with(watch_path) { return false; }
    } else {
        if path.parent() != Some(watch_path) { return false; }
    }

    let file_name = match path.file_name().and_then(|n| n.to_str()) {
		Some(name) => name,
		None => return false, // ファイル名が UTF-8 でない場合はルール不適用
	};

    if !matches_pattern(&file_name, rule) { return false; }
    if !matches_events(detected_events, &rule.events) { return false; }

    true
}

/// target フィルタ: file/directory/both の判定
///
/// kind はイベント受信時点でキャッシュ or notify サブタイプから解決済み。
/// Delete 後はパスが消えているため is_file()/is_dir() が false になるが、
/// その場合は kind=None かつ !path.exists() → 判別不能なので通過させる。
/// (旧 Windows の Remove(Any) でキャッシュミスが発生しても Delete を検知できるようにするため)
fn matches_target(path: &Path, target: &WatchTarget, kind: Option<EntryKind>) -> bool {
    match target {
        WatchTarget::Both => true,
        WatchTarget::File => kind
            .map(|k| k == EntryKind::File)
            .unwrap_or_else(|| path.is_file() || !path.exists()),
        WatchTarget::Directory => kind
            .map(|k| k == EntryKind::Dir)
            .unwrap_or_else(|| path.is_dir() || !path.exists()),
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
    log: Arc<Logger>,
    mut path_cache: HashMap<PathBuf, EntryKind>,
) -> Result<(), AppError> {
    // デバウンス用マップ: パス → (イベント集合, 最後の受信時刻, EntryKind)
    let mut pending: HashMap<PathBuf, (HashSet<Event>, Instant, Option<EntryKind>)> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            // (A) watcher からイベント受信 → pending に蓄積
            Some(res) = rx.recv() => {
                if let Ok(event) = res {
                    // Create 時はパスがまだ存在するのでキャッシュ更新できる
                    if matches!(event.kind, EventKind::Create(_)) {
                        for path in &event.paths {
                            if path.is_file() {
                                path_cache.insert(path.clone(), EntryKind::File);
                            } else if path.is_dir() {
                                path_cache.insert(path.clone(), EntryKind::Dir);
                            }
                        }
                    }

                    // notify サブタイプ優先、なければキャッシュ参照（Windows 対応）
                    let notify_kind = kind_from_notify_event(&event);

                    if let Some(config_event) = to_config_event(&event.kind) {
                        for path in &event.paths {
                            let kind = notify_kind.or_else(|| path_cache.get(path).copied());

                            // Remove 時はキャッシュから削除（kind は取得済み）
                            if matches!(event.kind, EventKind::Remove(_)) {
                                path_cache.remove(path);
                            }

                            let entry = pending
                                .entry(path.clone())
                                .or_insert_with(|| (HashSet::new(), Instant::now(), kind));
                            entry.0.insert(config_event.clone());
                            entry.1 = Instant::now();
                            if entry.2.is_none() {
                                entry.2 = kind;
                            }
                        }
                    }
                }
            }

            // (B) 100ms タイマー → 500ms 経過分を取り出して評価
            _ = interval.tick() => {
                let now = Instant::now();
                let ready: Vec<(PathBuf, HashSet<Event>, Option<EntryKind>)> = pending.iter()
                    .filter(|(_, (_, last, _))| now.duration_since(*last) >= Duration::from_millis(500))
                    .map(|(path, (events, _, kind))| (path.clone(), events.clone(), *kind))
                    .collect();

                for (path, detected_events, kind) in ready {
                    pending.remove(&path);
                    for rule in compiled_rules {
                        if !evaluate_rule(&path, &detected_events, kind, rule) {
                            continue;
                        }

                        log.log_match(
                            &rule.name,
                            path.display().to_string(),
                            detected_events.clone(),
                        );

                        let watch_path = PathBuf::from(&rule.watch_path);
                        if let Err(e) = crate::actions::execute_chain(
                            &rule.actions,
                            &path,
                            &watch_path,
                            global,
                            Arc::clone(&log),
                        ).await {
                            log.error(format!(
                                "アクションチェーン実行エラー: ルール={}, パス={}, エラー={}",
                                rule.name, path.display(), e
                            ));
                        }
                    }
                }
            }

            // (C) Ctrl+C → 終了
            _ = tokio::signal::ctrl_c() => {
                log.info("終了シグナル受信");
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    fn make_rule(watch_path: &str, recursive: bool, patterns: Option<Vec<&str>>) -> CompiledRule {
        let glob_set = patterns.map(|pats| {
            let mut builder = GlobSetBuilder::new();
            for p in pats {
                builder.add(Glob::new(p).unwrap());
            }
            builder.build().unwrap()
        });
        CompiledRule {
            name: format!("rule-{}", watch_path),
            enabled: true,
            watch_path: watch_path.to_string(),
            recursive,
            target: WatchTarget::Both,
            include_hidden: false,
            events: vec![Event::Create],
            glob_set,
            exclude_glob_set: None,
            regexes: None,
            actions: vec![],
        }
    }

    fn create_events(e: Event) -> HashSet<Event> {
        let mut s = HashSet::new();
        s.insert(e);
        s
    }

    // 2つの監視ディレクトリを用意し、片方で検知したファイルが
    // もう片方のルールに誤マッチしないことを確認する（バグ再現テスト）
    #[test]
    fn test_no_cross_directory_match() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();

        let rule_a = make_rule(dir_a.path().to_str().unwrap(), false, Some(vec!["*.csv"]));
        let rule_b = make_rule(dir_b.path().to_str().unwrap(), false, Some(vec!["*.csv"]));

        let file_in_a = dir_a.path().join("data.csv");
        std::fs::write(&file_in_a, "").unwrap();
        let events = create_events(Event::Create);

        assert!(evaluate_rule(&file_in_a, &events, None, &rule_a), "dir_a のルールはマッチすべき");
        assert!(!evaluate_rule(&file_in_a, &events, None, &rule_b), "dir_b のルールはマッチしてはいけない");
    }

    // recursive=false でサブディレクトリのファイルが除外されることを確認
    #[test]
    fn test_non_recursive_excludes_subdir() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("sub");
        std::fs::create_dir(&subdir).unwrap();
        let file_in_sub = subdir.join("data.csv");
        std::fs::write(&file_in_sub, "").unwrap();

        let rule = make_rule(dir.path().to_str().unwrap(), false, Some(vec!["*.csv"]));
        let events = create_events(Event::Create);

        assert!(!evaluate_rule(&file_in_sub, &events, None, &rule), "サブディレクトリのファイルはマッチしてはいけない");
    }

    // recursive=true ではサブディレクトリのファイルもマッチすることを確認
    #[test]
    fn test_recursive_includes_subdir() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("sub");
        std::fs::create_dir(&subdir).unwrap();
        let file_in_sub = subdir.join("data.csv");
        std::fs::write(&file_in_sub, "").unwrap();

        let rule = make_rule(dir.path().to_str().unwrap(), true, Some(vec!["*.csv"]));
        let events = create_events(Event::Create);

        assert!(evaluate_rule(&file_in_sub, &events, None, &rule), "recursive=true ならサブディレクトリもマッチすべき");
    }

    // 直下のファイルは recursive=false でもマッチすることを確認
    #[test]
    fn test_non_recursive_matches_direct_child() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("data.csv");
        std::fs::write(&file, "").unwrap();

        let rule = make_rule(dir.path().to_str().unwrap(), false, Some(vec!["*.csv"]));
        let events = create_events(Event::Create);

        assert!(evaluate_rule(&file, &events, None, &rule));
    }

    // パターンに合わないファイルは除外されることを確認
    #[test]
    fn test_pattern_mismatch_excluded() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("image.png");
        std::fs::write(&file, "").unwrap();

        let rule = make_rule(dir.path().to_str().unwrap(), false, Some(vec!["*.csv"]));
        let events = create_events(Event::Create);

        assert!(!evaluate_rule(&file, &events, None, &rule));
    }

    fn make_rule_with_target_and_events(
        watch_path: &str,
        target: WatchTarget,
        events: Vec<Event>,
    ) -> CompiledRule {
        CompiledRule {
            name: "test-rule".to_string(),
            enabled: true,
            watch_path: watch_path.to_string(),
            recursive: true,
            target,
            include_hidden: false,
            events,
            glob_set: None,
            exclude_glob_set: None,
            regexes: None,
            actions: vec![],
        }
    }

    // ── Issue #23 回帰テスト: target=file + delete ────────────────────────────

    // kind=File が明示的に渡された場合、削除後でも target=file にマッチする
    #[test]
    fn test_delete_file_with_kind_file_matches_target_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.csv");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::File,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        // ファイルは削除済み (path が存在しない) だが kind=File があるのでマッチする
        assert!(evaluate_rule(&path, &events, Some(EntryKind::File), &rule));
    }

    // kind=Dir が来た場合、target=file にはマッチしない
    #[test]
    fn test_delete_dir_does_not_match_target_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::File,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(!evaluate_rule(&path, &events, Some(EntryKind::Dir), &rule));
    }

    // kind=None かつパスが存在しない場合 (旧 Windows 無キャッシュ / Delete 後) はマッチする
    // ファイルか ディレクトリかを区別できないが、削除済みパスは通過させて false negative を防ぐ
    #[test]
    fn test_delete_no_kind_no_path_matches_target_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ghost.txt"); // 存在しないパス (削除済み想定)
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::File,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        // kind=None かつ path が存在しない → 判別不能なので通過 (target=file も target=dir も true)
        assert!(evaluate_rule(&path, &events, None, &rule));
    }

    // target=dir + kind=Dir → マッチする
    #[test]
    fn test_delete_dir_with_kind_dir_matches_target_dir() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("removed_dir");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Directory,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(evaluate_rule(&path, &events, Some(EntryKind::Dir), &rule));
    }

    // target=dir + kind=File → マッチしない
    #[test]
    fn test_delete_file_does_not_match_target_dir() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.csv");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Directory,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(!evaluate_rule(&path, &events, Some(EntryKind::File), &rule));
    }

    // target=both + kind=File → マッチする
    #[test]
    fn test_delete_file_matches_target_both() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.csv");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Both,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(evaluate_rule(&path, &events, Some(EntryKind::File), &rule));
    }

    // ── Issue #24 回帰テスト: target=both + delete で親ディレクトリが誤マッチしない ──

    // ファイル削除後に親ディレクトリが Modify だけ受け取った場合、
    // events=["delete"] ルールにマッチしないこと
    #[test]
    fn test_parent_dir_modify_does_not_match_delete_rule() {
        let dir = TempDir::new().unwrap();
        let parent = dir.path().to_path_buf(); // 親ディレクトリ自身のパス
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Both,
            vec![Event::Delete],
        );
        // 親ディレクトリは Modify のみ蓄積（ファイル削除の副作用）
        let events = create_events(Event::Modify);
        assert!(!evaluate_rule(&parent, &events, Some(EntryKind::Dir), &rule));
    }

    // Modify と Delete が混在している場合は Delete ルールにマッチする
    #[test]
    fn test_mixed_modify_delete_events_matches_delete_rule() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.csv");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Both,
            vec![Event::Delete],
        );
        let mut events = HashSet::new();
        events.insert(Event::Modify);
        events.insert(Event::Delete);
        assert!(evaluate_rule(&path, &events, Some(EntryKind::File), &rule));
    }

    // 親ディレクトリが Modify のみ、events=["delete","modify"] の場合はマッチする
    // (これは仕様通りの動作 — modify も監視対象なので)
    #[test]
    fn test_parent_dir_modify_matches_when_modify_in_rule() {
        let dir = TempDir::new().unwrap();
        let parent = dir.path().to_path_buf();
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Both,
            vec![Event::Delete, Event::Modify],
        );
        let events = create_events(Event::Modify);
        assert!(evaluate_rule(&parent, &events, Some(EntryKind::Dir), &rule));
    }

    // rm -r: ディレクトリ内ファイルが Remove(File) で来た場合、target=file にマッチ
    #[test]
    fn test_rm_r_inner_file_matches_target_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir").join("inner.txt");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::File,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(evaluate_rule(&path, &events, Some(EntryKind::File), &rule));
    }

    // rm -r: ディレクトリ自体が Remove(Folder) で来た場合、target=dir にマッチ
    #[test]
    fn test_rm_r_dir_matches_target_dir() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::Directory,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(evaluate_rule(&path, &events, Some(EntryKind::Dir), &rule));
    }

    // rm -r: ディレクトリ自体が Remove(Folder) で来た場合、target=file にはマッチしない
    #[test]
    fn test_rm_r_dir_does_not_match_target_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir");
        let rule = make_rule_with_target_and_events(
            dir.path().to_str().unwrap(),
            WatchTarget::File,
            vec![Event::Delete],
        );
        let events = create_events(Event::Delete);
        assert!(!evaluate_rule(&path, &events, Some(EntryKind::Dir), &rule));
    }
}