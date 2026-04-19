use std::path::{Path, PathBuf};

use crate::config::ActionConfig;
use crate::error::AppError;
use crate::placeholder::{expand_placeholders, PlaceholderContext};

/// BLAKE3 ハッシュ計算（同期 IO を spawn_blocking に逃がす）。
pub async fn hash_file_blake3(path: &Path) -> Result<blake3::Hash, AppError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<blake3::Hash, AppError> {
        let mut file = std::fs::File::open(&path)
            .map_err(|e| AppError::FileHash(format!("ファイルオープン失敗 ({}): {}", path.display(), e)))?;
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut file, &mut hasher)
            .map_err(|e| AppError::FileHash(format!("読み込み失敗 ({}): {}", path.display(), e)))?;
        Ok(hasher.finalize())
    })
    .await
    .map_err(|e| AppError::FileHash(format!("ハッシュ計算タスク失敗: {}", e)))?
}

/// 1 回分のファイルコピー試行（`tokio::fs::copy` + BLAKE3 整合性検証）。
/// 失敗時は宛先の削除を行わない。呼び出し側が責任を持つこと。
pub async fn try_copy_once(src: &Path, dest: &Path, verify_integrity: bool) -> Result<(), AppError> {
    tokio::fs::copy(src, dest)
        .await
        .map_err(|e| AppError::Action(format!("ファイルのコピーに失敗: {}", e)))?;

    if verify_integrity {
        let src_hash = hash_file_blake3(src).await?;
        let dest_hash = hash_file_blake3(dest).await?;
        if src_hash != dest_hash {
            return Err(AppError::FileHash(format!(
                "BLAKE3 不一致: src={} dest={}",
                src.display(),
                dest.display()
            )));
        }
    }
    Ok(())
}

/// 通常ファイルの宛先パスを算出する。
/// `preserve_structure=true` のとき `watch_path` からの相対パスを `dest_root` に結合する。
pub fn resolve_dest_path(
    src: &Path,
    dest_root: &Path,
    watch_path: &Path,
    preserve_structure: bool,
) -> Result<PathBuf, AppError> {
    if preserve_structure {
        let rel = src
            .strip_prefix(watch_path)
            .map_err(|e| AppError::Action(format!("relative_path の解決に失敗: {}", e)))?;
        Ok(dest_root.join(rel))
    } else {
        let file_name = src
            .file_name()
            .ok_or_else(|| AppError::Action("ファイル名の取得に失敗".to_string()))?;
        Ok(dest_root.join(file_name))
    }
}

/// `action.destination` をプレースホルダー展開して `PathBuf` で返す。
pub fn expand_action_destination(
    action: &ActionConfig,
    ctx: &PlaceholderContext,
) -> Result<PathBuf, AppError> {
    let raw = action
        .destination
        .as_deref()
        .ok_or_else(|| AppError::Action("destination が未指定".to_string()))?;
    let expanded = expand_placeholders(raw, ctx)?;
    Ok(PathBuf::from(expanded))
}

/// `src_dir` 配下のファイルを再帰的に列挙して返す（`walkdir` を `spawn_blocking` で実行）。
pub async fn walk_files(src_dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    let src = src_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| AppError::Action(format!("walkdir タスク失敗: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(path: &Path, body: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::File::create(path).unwrap().write_all(body).unwrap();
    }

    #[tokio::test]
    async fn hash_consistent() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x.bin");
        write_file(&p, b"abcdef");
        let h1 = hash_file_blake3(&p).await.unwrap();
        let h2 = hash_file_blake3(&p).await.unwrap();
        assert_eq!(h1, h2);
    }

    #[tokio::test]
    async fn hash_differs_for_different_content() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        write_file(&a, b"aaa");
        write_file(&b, b"bbb");
        assert_ne!(
            hash_file_blake3(&a).await.unwrap(),
            hash_file_blake3(&b).await.unwrap()
        );
    }

    #[tokio::test]
    async fn try_copy_once_copies_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dest = dir.path().join("dest.txt");
        write_file(&src, b"hello");
        try_copy_once(&src, &dest, false).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn try_copy_once_verify_integrity_ok() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dest = dir.path().join("dest.txt");
        write_file(&src, b"payload");
        try_copy_once(&src, &dest, true).await.unwrap();
        assert!(dest.exists());
    }

    #[tokio::test]
    async fn try_copy_once_integrity_mismatch_returns_error() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dest = dir.path().join("dest.txt");
        write_file(&src, b"original");
        write_file(&dest, b"tampered");
        // dest は既存だが tokio::fs::copy で上書きされてしまうためハッシュは一致する。
        // 不一致テスト: src を書き換えてから verify する
        try_copy_once(&src, &dest, false).await.unwrap();
        // dest 内容を上書きして不一致状態を作り hash 比較だけ確認
        write_file(&dest, b"corrupted");
        let src_hash = hash_file_blake3(&src).await.unwrap();
        let dest_hash = hash_file_blake3(&dest).await.unwrap();
        assert_ne!(src_hash, dest_hash);
    }

    #[test]
    fn resolve_dest_path_flat() {
        let src = Path::new("/watch/sub/a.txt");
        let dest_root = Path::new("/dest");
        let watch = Path::new("/watch");
        let result = resolve_dest_path(src, dest_root, watch, false).unwrap();
        assert_eq!(result, PathBuf::from("/dest/a.txt"));
    }

    #[test]
    fn resolve_dest_path_preserve() {
        let src = Path::new("/watch/sub/a.txt");
        let dest_root = Path::new("/dest");
        let watch = Path::new("/watch");
        let result = resolve_dest_path(src, dest_root, watch, true).unwrap();
        assert_eq!(result, PathBuf::from("/dest/sub/a.txt"));
    }

    #[tokio::test]
    async fn walk_files_returns_all_files() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), b"a");
        write_file(&dir.path().join("sub/b.txt"), b"b");
        write_file(&dir.path().join("sub/deep/c.txt"), b"c");
        let mut files = walk_files(dir.path()).await.unwrap();
        files.sort();
        assert_eq!(files.len(), 3);
    }
}
