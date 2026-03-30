/// グローバル設定
#[derive(Debug, Clone)]
pub struct GlobalSettings {
    pub retry_count: u32,
    pub retry_interval_ms: u64,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            retry_count: 3,
            retry_interval_ms: 1000,
        }
    }
}

/// copy アクションの設定
#[derive(Debug, Clone)]
pub struct CopyAction {
    pub destination: String,
    pub overwrite: bool,
    pub preserve_structure: bool,
}
