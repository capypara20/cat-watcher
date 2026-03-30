use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::AppError;

const BUF_SIZE: usize = 64 * 1024; // 64 KB

/// ファイル全体を 64 KB 単位でストリーミング読み込みし、BLAKE3 ハッシュを計算して
/// 小文字 hex 文字列として返す。
pub fn compute_file_hash(path: &Path) -> Result<String, AppError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUF_SIZE, file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; BUF_SIZE];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_empty_file() {
        let f = NamedTempFile::new().unwrap();
        let hash = compute_file_hash(f.path()).unwrap();
        // blake3 の空入力ハッシュは既知の値
        assert_eq!(
            hash,
            blake3::hash(b"").to_hex().to_string()
        );
    }

    #[test]
    fn test_known_content() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello world").unwrap();
        f.flush().unwrap();
        let hash = compute_file_hash(f.path()).unwrap();
        assert_eq!(hash, blake3::hash(b"hello world").to_hex().to_string());
    }

    #[test]
    fn test_large_content() {
        // 128 KB （バッファ 2 枚分）のデータでストリーミングが正しく動作するか検証
        let data: Vec<u8> = (0u8..=255).cycle().take(128 * 1024).collect();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&data).unwrap();
        f.flush().unwrap();
        let hash = compute_file_hash(f.path()).unwrap();
        assert_eq!(hash, blake3::hash(&data).to_hex().to_string());
    }

    #[test]
    fn test_different_content_different_hash() {
        let mut f1 = NamedTempFile::new().unwrap();
        f1.write_all(b"content A").unwrap();
        f1.flush().unwrap();

        let mut f2 = NamedTempFile::new().unwrap();
        f2.write_all(b"content B").unwrap();
        f2.flush().unwrap();

        let h1 = compute_file_hash(f1.path()).unwrap();
        let h2 = compute_file_hash(f2.path()).unwrap();
        assert_ne!(h1, h2);
    }
}
