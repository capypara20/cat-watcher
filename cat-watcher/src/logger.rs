use std::collections::HashSet;
use std::path::PathBuf;

use chrono::Local;
use colored::Colorize;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::config::{Global, LogLevel};
use crate::error::AppError;

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────";

#[derive(Debug)]
pub enum LogEntry {
    /// イベントブロック開始（セパレーター + MATCHラベル）
    Match {
        rule_name: String,
        path: String,
        events: HashSet<crate::config::Event>,
    },
    /// アクション開始
    Action {
        index: usize,
        total: usize,
        action_type: String,
        detail: String,
    },
    /// アクション正常完了
    Success(String),
    /// 通常情報
    Info(String),
    /// 警告
    Warn(String),
    /// エラー
    Error(String),
    /// dry_run 実行時
    DryRun(String),
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
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(writer_task(rx, log_dir, log_file_name, level));
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

    pub fn info(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Info(msg.into()));
    }

    pub fn success(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Success(msg.into()));
    }

    pub fn warn(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Warn(msg.into()));
    }

    pub fn error(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::Error(msg.into()));
    }

    pub fn dry_run(&self, msg: impl Into<String>) {
        let _ = self.tx.send(LogEntry::DryRun(msg.into()));
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
            eprintln!(
                "{}",
                format!("[ERROR] ログファイルオープン失敗 ({}): {}", path.display(), e)
                    .red()
                    .bold()
            );
            None
        }
    }
}

async fn writer_task(
    mut rx: mpsc::UnboundedReceiver<LogEntry>,
    log_dir: String,
    log_file_name: String,
    level: LogLevel,
) {
    let mut current_date = Local::now().format("%Y%m%d").to_string();
    let log_path = build_log_path(&log_dir, &log_file_name);
    let mut file = open_log_file(&log_path).await;

    while let Some(entry) = rx.recv().await {
        if matches!(entry, LogEntry::Shutdown) {
            break;
        }

        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        if today != current_date {
            if let Some(mut f) = file.take() {
                let _ = f.flush().await;
            }
            current_date = today;
            let new_path = build_log_path(&log_dir, &log_file_name);
            file = open_log_file(&new_path).await;
        }

        let ts = now.format("%Y-%m-%d %H:%M:%S").to_string();

        match &entry {
            LogEntry::Shutdown => break,

            LogEntry::Match { rule_name, path, events } => {
                if !level_enabled(&level, &LogLevel::Info) {
                    continue;
                }
                let event_str = format_events(events);
                let file_line = format!(
                    "[{ts}] [MATCH]   ルール={rule_name} | パス={path} | {event_str}\n"
                );
                let term_line = format!(
                    "{}\n{} {}",
                    SEPARATOR.bright_green().dimmed(),
                    format!("[{ts}] [MATCH]").bright_green().bold(),
                    format!("  ルール={rule_name} | パス={path} | {event_str}")
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Action { index, total, action_type, detail } => {
                if !level_enabled(&level, &LogLevel::Info) {
                    continue;
                }
                let file_line = format!(
                    "[{ts}] [ACTION]  ({index}/{total}) {action_type}  {detail}\n"
                );
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [ACTION]").blue().bold(),
                    format!("  ({index}/{total}) {action_type}  {detail}")
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Success(msg) => {
                if !level_enabled(&level, &LogLevel::Info) {
                    continue;
                }
                let file_line = format!("[{ts}] [SUCCESS] {msg}\n");
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [SUCCESS]").green().bold(),
                    msg
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Info(msg) => {
                if !level_enabled(&level, &LogLevel::Info) {
                    continue;
                }
                let file_line = format!("[{ts}] [INFO]    {msg}\n");
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [INFO]").cyan(),
                    msg
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Warn(msg) => {
                if !level_enabled(&level, &LogLevel::Warn) {
                    continue;
                }
                let file_line = format!("[{ts}] [WARN]    {msg}\n");
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [WARN]").yellow().bold(),
                    msg
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Error(msg) => {
                if !level_enabled(&level, &LogLevel::Error) {
                    continue;
                }
                let file_line = format!("[{ts}] [ERROR]   {msg}\n");
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [ERROR]").red().bold(),
                    msg
                );
                eprintln!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::DryRun(msg) => {
                if !level_enabled(&level, &LogLevel::Info) {
                    continue;
                }
                let file_line = format!("[{ts}] [DRY_RUN] {msg}\n");
                let term_line = format!(
                    "{} {}",
                    format!("[{ts}] [DRY_RUN]").magenta().bold(),
                    msg
                );
                println!("{}", term_line);
                write_file(&mut file, &file_line).await;
            }

            LogEntry::Shutdown => break,
        }
    }

    if let Some(mut f) = file {
        let _ = f.flush().await;
    }
}

async fn write_file(file: &mut Option<tokio::fs::File>, line: &str) {
    if let Some(f) = file {
        if let Err(e) = f.write_all(line.as_bytes()).await {
            eprintln!("{}", format!("[ERROR] ログ書き込み失敗: {e}").red().bold());
        }
    }
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
    names.join(", ")
}
