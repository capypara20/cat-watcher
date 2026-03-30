use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::AppError;

const BUF_SIZE: usize = 64 * 1024; // 64 KB

/// ファイル全体のBLAKE3ハッシュ値をストリーミング方式で計算し、hex文字列で返す。
pub fn compute_blake3_hash(path: &Path) -> Result<String, AppError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUF_SIZE, file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// コピー元とコピー先のBLAKE3ハッシュ値を比較し、一致すれば `Ok(true)` を返す。
pub fn verify_integrity(source: &Path, destination: &Path) -> Result<bool, AppError> {
    let src_hash = compute_blake3_hash(source)?;
    let dst_hash = compute_blake3_hash(destination)?;
    Ok(src_hash == dst_hash)
}

/// ファイルコピーとBLAKE3ハッシュによる整合性検証を行う。
/// 不一致の場合は `retry_count` 回までリトライする。
pub fn copy_with_verification(
    source: &Path,
    destination: &Path,
    retry_count: u32,
) -> Result<(), AppError> {
    let src_hash = compute_blake3_hash(source)?;

    let mut last_dst_hash = String::new();

    for attempt in 0..=retry_count {
        std::fs::copy(source, destination)?;

        last_dst_hash = compute_blake3_hash(destination)?;
        if src_hash == last_dst_hash {
            return Ok(());
        }

        if attempt < retry_count {
            eprintln!(
                "ハッシュ不一致 (試行 {}/{}): src={}, dst={} — リトライします",
                attempt + 1,
                retry_count + 1,
                src_hash,
                last_dst_hash,
            );
        }
    }

    Err(AppError::Action(format!(
        "整合性検証失敗: ハッシュ不一致 (src={}, dst={})",
        src_hash, last_dst_hash,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn create_temp_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_compute_blake3_hash_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "a.txt", b"hello world");

        let h1 = compute_blake3_hash(&path).unwrap();
        let h2 = compute_blake3_hash(&path).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_blake3_hash_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "b.txt", b"hello world");

        let hash = compute_blake3_hash(&path).unwrap();
        // blake3::hash(b"hello world").to_hex() の既知値
        let expected = blake3::hash(b"hello world").to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_compute_blake3_hash_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "empty.txt", b"");

        let hash = compute_blake3_hash(&path).unwrap();
        let expected = blake3::hash(b"").to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_compute_blake3_hash_large_file() {
        let dir = tempfile::tempdir().unwrap();
        // 256KB のデータ（64KBバッファを複数回跨ぐ）
        let data: Vec<u8> = (0u8..=255).cycle().take(256 * 1024).collect();
        let path = create_temp_file(dir.path(), "large.bin", &data);

        let hash = compute_blake3_hash(&path).unwrap();
        let expected = blake3::hash(&data).to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_compute_blake3_hash_different_content() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = create_temp_file(dir.path(), "x.txt", b"aaa");
        let p2 = create_temp_file(dir.path(), "y.txt", b"bbb");

        let h1 = compute_blake3_hash(&p1).unwrap();
        let h2 = compute_blake3_hash(&p2).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compute_blake3_hash_nonexistent_file() {
        let result = compute_blake3_hash(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_integrity_identical_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = create_temp_file(dir.path(), "src.txt", b"same content");
        let dst = create_temp_file(dir.path(), "dst.txt", b"same content");

        assert!(verify_integrity(&src, &dst).unwrap());
    }

    #[test]
    fn test_verify_integrity_different_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = create_temp_file(dir.path(), "src.txt", b"content A");
        let dst = create_temp_file(dir.path(), "dst.txt", b"content B");

        assert!(!verify_integrity(&src, &dst).unwrap());
    }

    #[test]
    fn test_copy_with_verification_success() {
        let dir = tempfile::tempdir().unwrap();
        let src = create_temp_file(dir.path(), "src.bin", b"payload data");
        let dst = dir.path().join("dst.bin");

        copy_with_verification(&src, &dst, 3).unwrap();

        assert!(dst.exists());
        assert_eq!(fs::read(&src).unwrap(), fs::read(&dst).unwrap());
    }

    #[test]
    fn test_copy_with_verification_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0u8..=255).cycle().take(256 * 1024).collect();
        let src = create_temp_file(dir.path(), "big.bin", &data);
        let dst = dir.path().join("big_copy.bin");

        copy_with_verification(&src, &dst, 0).unwrap();

        assert_eq!(fs::read(&src).unwrap(), fs::read(&dst).unwrap());
    }

    #[test]
    fn test_copy_with_verification_zero_retry() {
        let dir = tempfile::tempdir().unwrap();
        let src = create_temp_file(dir.path(), "s.txt", b"zero retry test");
        let dst = dir.path().join("d.txt");

        copy_with_verification(&src, &dst, 0).unwrap();
        assert!(dst.exists());
    }

    #[test]
    fn test_copy_with_verification_source_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("missing.txt");
        let dst = dir.path().join("dst.txt");

        let result = copy_with_verification(&src, &dst, 3);
        assert!(result.is_err());
    }
}
