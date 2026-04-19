// cat-watcher/src/actions/mod.rs
pub mod command;
pub mod common;
pub mod copy;
pub mod execute;
pub mod r#move;

use std::path::Path;

use crate::config::{ActionConfig, ActionType, Global};
use crate::error::AppError;
use crate::placeholder::PlaceholderContext;

/// 1 つの監視イベントに対して、ルールの actions を順に実行する。
/// アクション間で PlaceholderContext を保持し、copy/move 完了後に {Destination} を更新する。
pub async fn execute_chain(
    actions: &[ActionConfig],
    src: &Path,
    watch_path: &Path,
    global: &Global,
) -> Result<(), AppError> {
    let mut ctx = PlaceholderContext::new(src, watch_path, "");

    for action in actions {
        match action.type_ {
            ActionType::Copy => {
                let result = copy::execute(action, src, &ctx, global).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Move => {
                let result = r#move::execute(action, src, &ctx, global).await?;
                if let Some(dest_file) = result {
                    ctx.destination = dest_file.to_string_lossy().replace('\\', "/");
                }
            }
            ActionType::Command => {
                command::execute(action, &ctx, global).await?;
            }
            ActionType::Execute => {
                execute::execute(action, &ctx, global).await?;
            }
        }
    }
    Ok(())
}
