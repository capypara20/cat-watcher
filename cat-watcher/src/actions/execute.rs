use std::sync::Arc;

use std::process::Stdio;

use crate::config::{ActionConfig, Global};
use crate::error::AppError;
use crate::logger::Logger;
use crate::placeholder::{expand_placeholders, PlaceholderContext};

pub async fn execute(
    action: &ActionConfig,
    ctx: &PlaceholderContext,
    global: &Global,
    log: Arc<Logger>,
) -> Result<(), AppError> {
    let program = action
        .program
        .as_deref()
        .ok_or_else(|| AppError::Action("execute: program が未指定".to_string()))?;

    let raw_args = action
        .args
        .as_deref()
        .ok_or_else(|| AppError::Action("execute: args が未指定".to_string()))?;

    let expanded_args: Vec<String> = raw_args
        .iter()
        .map(|a| expand_placeholders(a, ctx))
        .collect::<Result<_, _>>()?;

    let working_dir = action
        .working_dir
        .as_deref()
        .filter(|s| !s.is_empty());

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&expanded_args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn().map_err(|e| {
        AppError::Action(format!(
            "execute: プロセス起動失敗 (program={program} args={expanded_args:?}): {e}"
        ))
    })?;

    log.info(format!("プロセス起動: program={program} args={expanded_args:?}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActionType, Global, LogLevel, LogRotation};
    use tempfile::tempdir;

    fn make_global() -> Global {
        let dir = tempdir().unwrap();
        Global {
            log_level: LogLevel::Info,
            log_dir: dir.path().to_str().unwrap().to_string(),
            log_file_name: "test.log".to_string(),
            log_rotation: LogRotation::Never,
            retry_count: 0,
            retry_interval_ms: 0,
        }
    }

    fn make_action(program: &str, args: Vec<&str>, working_dir: &str) -> ActionConfig {
        ActionConfig {
            type_: ActionType::Execute,
            destination: None,
            overwrite: None,
            preserve_structure: None,
            verify_integrity: None,
            shell: None,
            command: None,
            working_dir: Some(working_dir.to_string()),
            program: Some(program.to_string()),
            args: Some(args.into_iter().map(|s| s.to_string()).collect()),
            message: None,
        }
    }

    fn make_ctx(src: &std::path::Path, watch: &std::path::Path) -> PlaceholderContext {
        PlaceholderContext::new(src, watch, "")
    }

    fn make_logger() -> Arc<Logger> {
        let dir = tempdir().unwrap();
        let global = Global {
            log_level: LogLevel::Info,
            log_dir: dir.path().to_str().unwrap().to_string(),
            log_file_name: "test.log".to_string(),
            log_rotation: LogRotation::Never,
            retry_count: 0,
            retry_interval_ms: 0,
        };
        std::mem::forget(dir);
        let (logger, _) = Logger::new(&global).unwrap();
        Arc::new(logger)
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn spawns_program_successfully() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd.exe", vec!["/C", "echo hello"], "");
        let global = make_global();
        assert!(execute(&action, &ctx, &global, make_logger()).await.is_ok());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn placeholder_expands_in_args() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("report.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd.exe", vec!["/C", "echo {FullName}"], "");
        let global = make_global();
        assert!(execute(&action, &ctx, &global, make_logger()).await.is_ok());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn working_dir_is_set() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd.exe", vec!["/C", "echo hi"], dir.path().to_str().unwrap());
        let global = make_global();
        assert!(execute(&action, &ctx, &global, make_logger()).await.is_ok());
    }

    #[tokio::test]
    async fn nonexistent_program_returns_error() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("nonexistent_program_xyz.exe", vec![], "");
        let global = make_global();
        let result = execute(&action, &ctx, &global, make_logger()).await;
        assert!(result.is_err());
    }
}
