use crate::error::AppError;
use clap::Parser;
use std::path::PathBuf;
mod config;
mod error;
mod placeholder;
mod watcher;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    global: PathBuf,
    #[arg(short, long)]
    rules: PathBuf,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    log_level: Option<String>,
    #[arg(long)]
    validate: bool,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let cli = Cli::parse();

    let result = run(&cli).await;
    match result {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("実行エラー: {}", e);
            std::process::exit(1);
        }
    };
}

async fn run(cli: &Cli) -> Result<(), AppError> {
    let global_config = config::load_global_config(&cli.global)?;
    let rules_conf = config::load_rules_config(&cli.rules)?;

    config::validate_global_config(&global_config)?;
    config::validate_rules_config(&rules_conf)?;

    if cli.validate {
        println!("バリデーション処理成功");
        return Ok(());
    }
	watcher::start_watching(&rules_conf.rules).await?;
    Ok(())	
}
