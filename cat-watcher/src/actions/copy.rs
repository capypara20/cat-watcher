use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::config::{CopyAction, GlobalSettings};
use crate::error::AppError;
use crate::hash::compute_file_hash;

/// コピー元ファイルを宛先にコピーし、BLAKE3 ハッシュで整合性を検証する。
///
/// 手順:
/// 1. コピー元のハッシュ値を計算する
/// 2. ファイルをコピーする
/// 3. コピー先のハッシュ値を計算する
/// 4. 両者を比較し、一致した場合のみ成功とする
/// 5. 不一致の場合は `global.retry_count` 回までリトライする
pub fn execute_copy(
    src: &Path,
    action: &CopyAction,
    global: &GlobalSettings,
) -> Result<PathBuf, AppError> {
    let dst = resolve_destination(src, action)?;

    // コピー前にコピー元のハッシュ値を計算する
    let src_hash = compute_file_hash(src)?;

    copy_with_retry(src, &dst, &src_hash, global)?;

    Ok(dst)
}

/// 宛先パスを解決する。
fn resolve_destination(src: &Path, action: &CopyAction) -> Result<PathBuf, AppError> {
    let dst_root = Path::new(&action.destination);

    let dst = dst_root.join(src.file_name().ok_or_else(|| {
        AppError::Action(format!("ファイル名を取得できません: {}", src.display()))
    })?);

    Ok(dst)
}

/// コピーを実行し、ハッシュが一致するまで最大 `retry_count` 回リトライする。
fn copy_with_retry(
    src: &Path,
    dst: &Path,
    src_hash: &str,
    global: &GlobalSettings,
) -> Result<(), AppError> {
    // 宛先ディレクトリを作成する
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut last_error: Option<AppError> = None;

    for attempt in 0..=global.retry_count {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(global.retry_interval_ms));
        }

        // ファイルをコピーする
        match std::fs::copy(src, dst) {
            Err(e) => {
                last_error = Some(AppError::Io(e));
                continue;
            }
            Ok(_) => {}
        }

        // コピー先のハッシュ値を計算して比較する
        match compute_file_hash(dst) {
            Err(e) => {
                last_error = Some(e);
                continue;
            }
            Ok(dst_hash) => {
                if dst_hash == src_hash {
                    return Ok(());
                }
                last_error = Some(AppError::HashMismatch {
                    src: src_hash.to_string(),
                    dst: dst_hash,
                });
                // ハッシュ不一致の場合はリトライ
            }
        }
    }

    Err(last_error.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_global(retry_count: u32) -> GlobalSettings {
        GlobalSettings {
            retry_count,
            retry_interval_ms: 0,
        }
    }

    #[test]
    fn test_copy_success() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();

        let src_path = src_dir.path().join("test.txt");
        std::fs::write(&src_path, b"hello blake3").unwrap();

        let action = CopyAction {
            destination: dst_dir.path().to_string_lossy().to_string(),
            overwrite: true,
            preserve_structure: false,
        };
        let global = make_global(3);

        let dst = execute_copy(&src_path, &action, &global).unwrap();
        assert!(dst.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello blake3");
    }

    #[test]
    fn test_copy_hash_matches() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();

        let data: Vec<u8> = (0u8..=255).cycle().take(256 * 1024).collect();
        let src_path = src_dir.path().join("large.bin");
        std::fs::write(&src_path, &data).unwrap();

        let action = CopyAction {
            destination: dst_dir.path().to_string_lossy().to_string(),
            overwrite: true,
            preserve_structure: false,
        };
        let global = make_global(3);

        let dst = execute_copy(&src_path, &action, &global).unwrap();

        let src_hash = compute_file_hash(&src_path).unwrap();
        let dst_hash = compute_file_hash(&dst).unwrap();
        assert_eq!(src_hash, dst_hash);
    }

    #[test]
    fn test_copy_destination_dir_created() {
        let src_dir = tempdir().unwrap();
        let dst_root = tempdir().unwrap();
        let dst_subdir = dst_root.path().join("sub").join("dir");

        let src_path = src_dir.path().join("file.txt");
        std::fs::write(&src_path, b"data").unwrap();

        let action = CopyAction {
            destination: dst_subdir.to_string_lossy().to_string(),
            overwrite: true,
            preserve_structure: false,
        };
        let global = make_global(0);

        let dst = execute_copy(&src_path, &action, &global).unwrap();
        assert!(dst.exists());
    }

    #[test]
    fn test_copy_src_not_found() {
        let dst_dir = tempdir().unwrap();
        let src_path = Path::new("/nonexistent/path/file.txt");

        let action = CopyAction {
            destination: dst_dir.path().to_string_lossy().to_string(),
            overwrite: true,
            preserve_structure: false,
        };
        let global = make_global(0);

        let result = execute_copy(src_path, &action, &global);
        assert!(result.is_err());
    }
}
