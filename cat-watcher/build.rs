fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set("FileDescription", "ファイル監視・自動処理ツール");
        res.set("ProductName", "cat-watcher");
        res.set("LegalCopyright", "Copyright \u{00a9} 2026 capypara20");
        // Linux クロスコンパイル時は mingw の windres を使う
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
            && std::env::consts::OS != "windows"
        {
            res.set_windres_path("x86_64-w64-mingw32-windres");
            res.set_ar_path("x86_64-w64-mingw32-ar");
        }
        res.compile().expect("Windows リソースのコンパイルに失敗");
    }
}
