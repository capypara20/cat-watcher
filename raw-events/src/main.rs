// notify の生イベント観察ツール
//
// 使い方:
//   cargo run -p raw-events -- <監視したいパス>
//
// 何をするか:
//   指定パスを再帰的に監視し、notify から届く Event をデバウンスせず
//   そのまま標準出力に書き出す。1 操作で何個イベントが来るか、
//   どんな EventKind が来るかを目で確認するための実験用コード。

use std::path::PathBuf;
use std::sync::mpsc;

use chrono::Local;
use notify::{recommended_watcher, Event, RecursiveMode, Watcher};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("引数に監視パスを指定してください: cargo run -p raw-events -- <path>")?;
    let path = PathBuf::from(path);

    println!("[start] 監視開始: {}", path.display());
    println!("[start] 終了は Ctrl+C");
    println!();

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;

    watcher.watch(&path, RecursiveMode::Recursive)?;

    let mut seq: u64 = 0;
    for res in rx {
        seq += 1;
        let ts = Local::now().format("%H:%M:%S%.3f");
        match res {
            Ok(ev) => {
                // kind / paths / attrs を一行にまとめて出す
                println!(
                    "[{ts}] #{seq:04} kind={:?} paths={:?} attrs={:?}",
                    ev.kind, ev.paths, ev.attrs
                );
            }
            Err(e) => {
                println!("[{ts}] #{seq:04} ERROR {e}");
            }
        }
    }

    Ok(())
}
