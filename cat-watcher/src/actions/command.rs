use crate::config::{ActionConfig, Global};
use crate::error::AppError;
use crate::placeholder::{expand_placeholders, PlaceholderContext};

/// command アクションのエントリポイント。
/// シェル経由でコマンドを fire-and-forget で起動する。
pub async fn execute(
    action: &ActionConfig,
    ctx: &PlaceholderContext,
    global: &Global,
) -> Result<(), AppError> {
    let raw_command = action
        .command
        .as_deref()
        .ok_or_else(|| AppError::Action("command: command が未指定".to_string()))?;
    let expanded = expand_placeholders(raw_command, ctx)?;

    let shell = action
        .shell
        .as_deref()
        .ok_or_else(|| AppError::Action("command: shell が未指定".to_string()))?;

    let working_dir = action
        .working_dir
        .as_deref()
        .filter(|s| !s.is_empty());

    if global.dry_run {
        println!("[INFO] (dry_run) command: shell={} cmd={}", shell, expanded);
        return Ok(());
    }

    let mut cmd = build_shell_command(shell, &expanded)?;

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn().map_err(|e| {
        AppError::Action(format!(
            "command: プロセス起動失敗 (shell={} cmd={}): {}",
            shell, expanded, e
        ))
    })?;

    println!("[INFO] command 起動: shell={} cmd={}", shell, expanded);
    Ok(())
}

fn build_shell_command(
    shell: &str,
    expanded: &str,
) -> Result<tokio::process::Command, AppError> {
    match shell {
        "cmd" => {
            let mut c = tokio::process::Command::new("cmd.exe");
            c.args(["/C", expanded]);
            Ok(c)
        }
        "powershell" => {
            let mut c = tokio::process::Command::new("powershell.exe");
            c.args(["-NoProfile", "-Command", expanded]);
            Ok(c)
        }
        "pwsh" => {
            let mut c = tokio::process::Command::new("pwsh.exe");
            c.args(["-NoProfile", "-Command", expanded]);
            Ok(c)
        }
        other => Err(AppError::Action(format!(
            "command: 不明なシェル '{}'。cmd / powershell / pwsh のいずれかを指定してください",
            other
        ))),
    }
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

    fn make_action(shell: &str, command: &str, working_dir: &str) -> ActionConfig {
        ActionConfig {
            type_: ActionType::Command,
            destination: None,
            overwrite: None,
            preserve_structure: None,
            verify_integrity: None,
            shell: Some(shell.to_string()),
            command: Some(command.to_string()),
            working_dir: Some(working_dir.to_string()),
            program: None,
            args: None,
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
        let action = make_action("cmd", "echo hello", "");
        let global = make_global(true);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn unknown_shell_returns_error() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("bash", "echo hi", "");
        let global = make_global(false);
        let result = execute(&action, &ctx, &global).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("不明なシェル"));
    }

    #[tokio::test]
    async fn cmd_spawns_successfully() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd", "echo hello", "");
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn placeholder_expands_in_command() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("report.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd", "echo {Name}", "");
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[tokio::test]
    async fn working_dir_is_set() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"x").unwrap();
        let ctx = make_ctx(&src, dir.path());
        let action = make_action("cmd", "echo hi", dir.path().to_str().unwrap());
        let global = make_global(false);
        assert!(execute(&action, &ctx, &global).await.is_ok());
    }

    #[test]
    fn build_shell_command_cmd() {
        assert!(build_shell_command("cmd", "echo test").is_ok());
    }

    #[test]
    fn build_shell_command_powershell() {
        assert!(build_shell_command("powershell", "Get-Date").is_ok());
    }

    #[test]
    fn build_shell_command_pwsh() {
        assert!(build_shell_command("pwsh", "Get-Date").is_ok());
    }

    #[test]
    fn build_shell_command_unknown() {
        assert!(build_shell_command("bash", "echo hi").is_err());
    }
}
