use std::collections::HashSet;
use std::path::PathBuf;

use chrono::Local;
use colored::Colorize;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::config::{Global, LogLevel, LogRotation};
use crate::error::AppError;

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────";

const FILE_LEVEL_WIDTH: usize = 7;
const FILE_EVENTS_WIDTH: usize = 27;

#[derive(Debug)]
pub enum LogEntry {
    /// イベントブロック開始
    Match {
        rule_name: String,
        path: String,
        events: HashSet<crate::config::Event>,
    },
    /// アクションチェーン ステップ開始
    Action {
        index: usize,
        total: usize,
        action_type: String,
        detail: String,
    },
    /// アクションチェーン ステップ完了
    ActionOk {
        index: usize,
        total: usize,
        msg: String,
    },
    /// 通常情報
    Info(String),
    /// 警告
    Warn(String),
    /// エラー
    Error(String),
    /// チャネルをクローズしてロガーを終了させる
    Shutdown,
}

pub struct Logger {
    tx: mpsc::UnboundedSender<LogEntry>,
}

impl Logger {
    pub fn new(global: &Global) -> Result<(Self, tokio::task::JoinHandle<()>), AppError> {
        #[cfg(windows)]
        colored::control::set_virtual_terminal(true).ok();
        colored::control::set_override(true);

        let level = global.log_level.clone();
        let log_dir = global.log_dir.clone();
        let log_file_name = global.log_file_name.clone();
        let log_rotation = global.log_rotation.clone();
        let log_to_console = global.log_to_console;
        let log_to_file = global.log_to_file;
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(writer_task(
            rx,
            log_dir,
            log_file_name,
            level,
            log_rotation,
            log_to_console,
            log_to_file,
        ));
        Ok((Self { tx }, handle))
    }

    pub fn log_match(
        &self,
        rule_name: impl Into<String>,
        path: impl Into<String>,
        events: HashSet<crate::config::Event>,
    ) {
        let _ = self.tx.send(LogEntry::Match {
            rule_name: rule_name.into(),
            path: path.into(),
            events,
        });
    }

    pub fn log_action(
        &self,
        index: usize,
        total: usize,
        action_type: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let _ = self.tx.send(LogEntry::Action {
            index,
            total,
            action_type: action_type.into(),
            detail: detail.into(),
        });
    }

    pub fn log_action_ok(&self, index: usize, total: usize, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::ActionOk {
            index,
            total,
            msg: msg.into(),
        });
    }

    pub fn info(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Info(msg.into()));
    }

    pub fn warn(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Warn(msg.into()));
    }

    pub fn error(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Error(msg.into()));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(LogEntry::Shutdown);
    }
}

fn build_log_path(log_dir: &str, log_file_name: &str) -> PathBuf {
    let now = Local::now();
    let file_name = log_file_name
        .replace("{Date}", &now.format("%Y%m%d").to_string())
        .replace("{DateTime}", &now.format("%Y%m%d_%H%M%S").to_string());
    PathBuf::from(log_dir).join(file_name)
}

async fn open_log_file(path: &PathBuf) -> Option<tokio::fs::File> {
    match OpenOptions::new().create(true).append(true).open(path).await {
        Ok(f) => Some(f),
        Err(e) => {
            let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
            eprintln!(
                "{}",
                format!("[{ts}] [ERROR] ログファイルオープン失敗 ({}): {}", path.display(), e)
                    .red()
                    .bold()
            );
            None
        }
    }
}

/// アクション種別を最大4文字の短縮名に変換する。
fn action_type_short(action_type: &str) -> &str {
    match action_type {
        "copy" => "copy",
        "move" => "move",
        "command" => "cmd",
        "execute" => "exec",
        "log" => "log",
        other => {
            if other.len() <= 4 { other } else { &other[..4] }
        }
    }
}

/// ファイルログ用 level カラム文字列（FILE_LEVEL_WIDTH 幅）を生成する。
fn file_level_col(entry: &LogEntry) -> String {
    match entry {
        LogEntry::Match { .. } => format!("{:<width$}", "MATCH", width = FILE_LEVEL_WIDTH),
        LogEntry::Action { index, total, action_type, .. } => {
            let tree = if *index == *total { '└' } else { '├' };
            let short = action_type_short(action_type);
            let index_str = index.to_string();
            let index_len = index_str.len();
            // tree(1) + index + space(1) + type; truncate type to fit in FILE_LEVEL_WIDTH
            let type_max = FILE_LEVEL_WIDTH.saturating_sub(1 + index_len + 1);
            let type_part: String = short.chars().take(type_max).collect();
            format!("{:<width$}", format!("{}{} {}", tree, index_str, type_part), width = FILE_LEVEL_WIDTH)
        }
        LogEntry::ActionOk { index, total, .. } => {
            if *index == *total {
                // 最終ステップ完了: 継続パイプなし
                format!("{:<width$}", "   OK", width = FILE_LEVEL_WIDTH)
            } else {
                // 中間ステップ完了: 継続パイプあり
                format!("{:<width$}", "│   OK", width = FILE_LEVEL_WIDTH)
            }
        }
        LogEntry::Info(_) => format!("{:<width$}", "INFO", width = FILE_LEVEL_WIDTH),
        LogEntry::Warn(_) => format!("{:<width$}", "WARN", width = FILE_LEVEL_WIDTH),
        LogEntry::Error(_) => format!("{:<width$}", "ERROR", width = FILE_LEVEL_WIDTH),
        LogEntry::Shutdown => unreachable!(),
    }
}

/// ファイルログ用 events カラム文字列（FILE_EVENTS_WIDTH 幅）を生成する。
/// MATCH エントリのみイベント名を出力し、それ以外は空白で埋める。
fn file_events_col(entry: &LogEntry) -> String {
    let s = match entry {
        LogEntry::Match { events, .. } => format_events(events),
        _ => String::new(),
    };
    format!("{:<width$}", s, width = FILE_EVENTS_WIDTH)
}

/// ファイルログ用 content カラムを生成する。
fn file_content(entry: &LogEntry) -> String {
    match entry {
        LogEntry::Match { path, .. } => path.clone(),
        LogEntry::Action { detail, .. } => detail.replace('\n', r"\n"),
        LogEntry::ActionOk { msg, .. } => msg.clone(),
        LogEntry::Info(msg) => msg.clone(),
        LogEntry::Warn(msg) => msg.clone(),
        LogEntry::Error(msg) => msg.clone(),
        LogEntry::Shutdown => unreachable!(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn writer_task(
    mut rx: mpsc::UnboundedReceiver<LogEntry>,
    log_dir: String,
    log_file_name: String,
    level: LogLevel,
    log_rotation: LogRotation,
    log_to_console: bool,
    log_to_file: bool,
) {
    let mut current_date = Local::now().format("%Y%m%d").to_string();
    let log_path = build_log_path(&log_dir, &log_file_name);
    let mut file = if log_to_file { open_log_file(&log_path).await } else { None };

    while let Some(entry) = rx.recv().await {
        if matches!(entry, LogEntry::Shutdown) {
            break;
        }

        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        if log_to_file && matches!(log_rotation, LogRotation::Daily) && today != current_date {
            if let Some(mut f) = file.take() {
                let _ = f.flush().await;
            }
            current_date = today;
            let new_path = build_log_path(&log_dir, &log_file_name);
            file = open_log_file(&new_path).await;
        }

        let ts = now.format("%Y-%m-%d %H:%M:%S").to_string();

        // ─── ファイルログ（4カラム固定幅フォーマット）───────────────────────
        if log_to_file {
            let file_line = match &entry {
                LogEntry::Match { .. } | LogEntry::Action { .. } | LogEntry::ActionOk { .. }
                | LogEntry::Info(_) | LogEntry::Warn(_) | LogEntry::Error(_) => {
                    if !level_enabled_for_entry(&level, &entry) {
                        String::new()
                    } else {
                        let lv = file_level_col(&entry);
                        let ev = file_events_col(&entry);
                        let ct = file_content(&entry);
                        format!("{} │ {} │ {} │ {}\n", ts, lv, ev, ct)
                    }
                }
                LogEntry::Shutdown => unreachable!(),
            };
            if !file_line.is_empty() {
                write_file(&mut file, &file_line).await;
            }
        }

        // ─── ターミナル出力（カラー付き従来フォーマット）─────────────────────
        if log_to_console {
            match &entry {
                LogEntry::Match { rule_name, path, events } => {
                    if !level_enabled(&level, &LogLevel::Info) { continue; }
                    let event_str = format_events(events);
                    let term_line = format!(
                        "{}\n{} {}",
                        SEPARATOR.bright_green().dimmed(),
                        format!("[{ts}] [MATCH]").bright_green().bold(),
                        format!("  ルール={rule_name} | パス={path} | {event_str}")
                    );
                    println!("{}", term_line);
                }

                LogEntry::Action { index, total, action_type, detail } => {
                    if !level_enabled(&level, &LogLevel::Info) { continue; }
                    let term_line = format!(
                        "{} {}",
                        format!("[{ts}] [ACTION]").blue().bold(),
                        format!("  ({index}/{total}) {action_type}  {detail}")
                    );
                    println!("{}", term_line);
                }

                LogEntry::ActionOk { msg, .. } => {
                    if !level_enabled(&level, &LogLevel::Info) { continue; }
                    let term_line = format!(
                        "{} {}",
                        format!("[{ts}] [OK]    ").green().bold(),
                        msg
                    );
                    println!("{}", term_line);
                }

                LogEntry::Info(msg) => {
                    if !level_enabled(&level, &LogLevel::Info) { continue; }
                    let term_line = format!(
                        "{} {}",
                        format!("[{ts}] [INFO]").cyan(),
                        msg
                    );
                    println!("{}", term_line);
                }

                LogEntry::Warn(msg) => {
                    if !level_enabled(&level, &LogLevel::Warn) { continue; }
                    let term_line = format!(
                        "{} {}",
                        format!("[{ts}] [WARN]").yellow().bold(),
                        msg
                    );
                    println!("{}", term_line);
                }

                LogEntry::Error(msg) => {
                    if !level_enabled(&level, &LogLevel::Error) { continue; }
                    let term_line = format!(
                        "{} {}",
                        format!("[{ts}] [ERROR]").red().bold(),
                        msg
                    );
                    eprintln!("{}", term_line);
                }

                LogEntry::Shutdown => unreachable!(),
            }
        }
    }

    if let Some(mut f) = file {
        let _ = f.flush().await;
    }
}

async fn write_file(file: &mut Option<tokio::fs::File>, line: &str) {
    if let Some(f) = file {
        if let Err(e) = f.write_all(line.as_bytes()).await {
            let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
            eprintln!("{}", format!("[{ts}] [ERROR] ログ書き込み失敗: {e}").red().bold());
        }
    }
}

fn level_enabled_for_entry(current: &LogLevel, entry: &LogEntry) -> bool {
    let required = match entry {
        LogEntry::Warn(_) => &LogLevel::Warn,
        LogEntry::Error(_) => &LogLevel::Error,
        _ => &LogLevel::Info,
    };
    level_enabled(current, required)
}

fn level_enabled(current: &LogLevel, required: &LogLevel) -> bool {
    level_to_u8(current) <= level_to_u8(required)
}

fn level_to_u8(level: &LogLevel) -> u8 {
    match level {
        LogLevel::Trace => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
    }
}

fn format_events(events: &HashSet<crate::config::Event>) -> String {
    let mut names: Vec<&str> = events
        .iter()
        .map(|e| match e {
            crate::config::Event::Create => "Create",
            crate::config::Event::Modify => "Modify",
            crate::config::Event::Delete => "Delete",
            crate::config::Event::Rename => "Rename",
        })
        .collect();
    names.sort();
    names.join(",")
}
