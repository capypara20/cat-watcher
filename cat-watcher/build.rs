fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set("FileDescription", "ファイル監視・自動処理ツール");
        res.set("ProductName", "cat-watcher");
        res.set("LegalCopyright", "Copyright \u{00a9} 2026 capypara20");
        res.compile().expect("Windows リソースのコンパイルに失敗");
    }
}
