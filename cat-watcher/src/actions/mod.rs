// cat-watcher/src/actions/mod.rs
pub mod common;
pub mod copy;
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
            // 後続フェーズで実装
            ActionType::Move => {
                eprintln!("[WARN] move アクションは未実装 (Phase 10)");
            }
            ActionType::Command => {
                eprintln!("[WARN] command アクションは未実装 (Phase 11)");
            }
            ActionType::Execute => {
                eprintln!("[WARN] execute アクションは未実装 (Phase 11)");
            }
        }
    }
    Ok(())
}
