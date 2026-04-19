use crate::config::{ActionConfig, Global};
use crate::error::AppError;
use crate::placeholder::{expand_placeholders, PlaceholderContext};

/// execute アクションのエントリポイント。
/// プログラムを直接 fire-and-forget で起動する（シェルを介さない）。
pub async fn execute(
    action: &ActionConfig,
    ctx: &PlaceholderContext,
    global: &Global,
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

    if global.dry_run {
        println!(
            "[INFO] (dry_run) execute: program={} args={:?}",
            program, expanded_args
        );
        return Ok(());
    }

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&expanded_args);

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn().map_err(|e| {
        AppError::Action(format!(
            "execute: プロセス起動失敗 (program={} args={:?}): {}",
            program, expanded_args, e
        ))
    })?;

    println!("[INFO] execute 起動: program={} args={:?}", program, expanded_args);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActionType, Global, LogLevel, LogRotation};
    use tempfile::tempdir;

    fn make_global(dry_run: bool) -> Global {
        Global {
            log_level: LogLevel::Info,
            log_file: "test.log".to_string(),
            log_rotation: LogRotation::Never,
            retry_count: 0,
            retry_interval_ms: 0,
            dry_run,
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
        }
    }

    fn make_ctx(src: &std::path::Path, watch: &std::path::Path) -> PlaceholderContext {
        PlaceholderContext::new(src, watch, "")
    }

    #[tokio::test]
    async fn dry_run_does_not_spawn() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("notepad.exe", vec![], "");
        let global = make_global(true);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn spawns_program_successfully() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        // cmd.exe は常に存在するので起動テストに使用
        let action = make_action("cmd.exe", vec!["/C", "echo hello"], "");
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn placeholder_expands_in_args() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("report.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        // {FullName} が展開された状態で起動する
        let action = make_action("cmd.exe", vec!["/C", "echo {FullName}"], "");
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn working_dir_is_set() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd.exe", vec!["/C", "echo hi"], dir.path().to_str().unwrap());
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn nonexistent_program_returns_error() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("nonexistent_program_xyz.exe", vec![], "");
        let global = make_global(false);
        let result = execute(&action, &ctx, &global).await;
        assert!(result.is_err());
    }
}
