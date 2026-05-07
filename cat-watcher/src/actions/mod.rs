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
use crate::placeholder::PlaceholderContext;

/// 1 つの監視イベントに対して、ルールの actions を順に実行する。
/// アクション間で PlaceholderContext を保持し、copy/move 完了後に {Destination} を更新する。
pub async fn execute_chain(
    actions: &[ActionConfig],
    src: &Path,
    watch_path: &Path,
    global: &Global,
    log: Arc<Logger>,
) -> Result<(), AppError> {
    let mut ctx = PlaceholderContext::new(src, watch_path, "");
    let total = actions.len();

    for (i, action) in actions.iter().enumerate() {
        let index = i + 1;
        match action.type_ {
            ActionType::Copy => {
                let dest_str = action.destination.as_deref().unwrap_or("");
                log.log_action(index, total, "copy", format!("{} → {}", src.display(), dest_str));
                let result = copy::execute(action, src, &ctx, global, Arc::clone(&log)).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Move => {
                let dest_str = action.destination.as_deref().unwrap_or("");
                log.log_action(index, total, "move", format!("{} → {}", src.display(), dest_str));
                let result = r#move::execute(action, src, &ctx, global, Arc::clone(&log)).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Command => {
                let shell = action.shell.as_deref().unwrap_or("");
                let cmd = action.command.as_deref().unwrap_or("");
                log.log_action(index, total, "command", format!("shell={shell}  cmd={cmd}"));
                command::execute(action, &ctx, global, Arc::clone(&log)).await?;
            }
            ActionType::Execute => {
                let program = action.program.as_deref().unwrap_or("");
                log.log_action(index, total, "execute", format!("program={program}"));
                execute::execute(action, &ctx, global, Arc::clone(&log)).await?;
            }
        }
    }
    Ok(())
}
