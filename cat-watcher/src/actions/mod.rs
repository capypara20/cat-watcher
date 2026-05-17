pub mod command;
pub mod common;
pub mod copy;
pub mod execute;
pub mod r#move;

use std::path::Path;
use std::sync::Arc;

use crate::config::{ActionConfig, ActionType, Global};
use crate::error::AppError;
use crate::logger::Logger;
use crate::placeholder::{expand_placeholders, PlaceholderContext};

/// global ロガーとルール別ロガーの両方に同じエントリを送る。
macro_rules! log_all {
    ($log:expr, $rule_log:expr, $method:ident ( $($arg:expr),* )) => {{
        $log.$method($($arg.clone()),*);
        if let Some(rl) = $rule_log {
            rl.$method($($arg),*);
        }
    }};
}

/// 1 つの監視イベントに対して、ルールの actions を順に実行する。
/// アクション間で PlaceholderContext を保持し、copy/move 完了後に {Destination} を更新する。
pub async fn execute_chain(
    actions: &[ActionConfig],
    src: &Path,
    watch_path: &Path,
    global: &Global,
    log: Arc<Logger>,
    rule_log: Option<Arc<Logger>>,
) -> Result<(), AppError> {
    let mut ctx = PlaceholderContext::new(src, watch_path, "");
    let total = actions.len();

    for (i, action) in actions.iter().enumerate() {
        let index = i + 1;
        let step = (index, total);
        match action.type_ {
            ActionType::Copy => {
                let dest_str = action.destination.as_deref().unwrap_or("");
                let detail = format!("{} → {}", src.display(), dest_str);
                log_all!(log, rule_log.as_deref(), log_action(index, total, "copy", detail));
                let result = copy::execute(action, src, &ctx, global, Arc::clone(&log), step).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Move => {
                let dest_str = action.destination.as_deref().unwrap_or("");
                let detail = format!("{} → {}", src.display(), dest_str);
                log_all!(log, rule_log.as_deref(), log_action(index, total, "move", detail));
                let result = r#move::execute(action, src, &ctx, global, Arc::clone(&log), step).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Command => {
                let shell = action.shell.as_deref().unwrap_or("");
                let cmd = action.command.as_deref().unwrap_or("");
                let detail = format!("shell={shell}  cmd={cmd}");
                log_all!(log, rule_log.as_deref(), log_action(index, total, "command", detail));
                command::execute(action, &ctx, global, Arc::clone(&log), step).await?;
            }
            ActionType::Execute => {
                let program = action.program.as_deref().unwrap_or("");
                let args = action.args.as_deref().unwrap_or(&[]);
                let args_str = args.join(" ");
                let detail = format!("{program} {args_str}").trim_end().to_string();
                log_all!(log, rule_log.as_deref(), log_action(index, total, "execute", detail));
                execute::execute(action, &ctx, global, Arc::clone(&log), step).await?;
            }
            ActionType::Log => {
                let raw = action.message.as_deref().unwrap_or("");
                let msg = expand_placeholders(raw, &ctx)?;
                log_all!(log, rule_log.as_deref(), log_action(index, total, "log", ""));
                log_all!(log, rule_log.as_deref(), log_action_ok(index, total, msg));
            }
        }
    }
    Ok(())
}
