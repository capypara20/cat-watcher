#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("設定ファイルエラー: {0}")]
    Config(String),

    #[error("バリデーションエラー: {0}")]
    Validation(String),

    #[error("I/O エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML パースエラー: {0}")]
    TomlParse(String),

    #[error("監視エラー: {0}")]
    Watch(String),

    #[error("アクション実行エラー: {0}")]
    Action(String),
}
