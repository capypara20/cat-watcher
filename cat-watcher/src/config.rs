use std::path::{self, Path};
use crate::error::AppError;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
	Daily,
	Never,
}


#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchTarget {
    File,      //ファイルのみか
    Directory, //ディレクトリのみか
    Both,      //両方か
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    Create,
    Modify,
    Delete,
    Rename,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    Copy,
    Move,
    Command,
    Execute,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalConfig {
    pub global: Global,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulesConfig {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Global {
    pub log_level: LogLevel,
    pub log_file: String,
    pub log_rotation: LogRotation,
    pub retry_count: u32,
    pub retry_interval_ms: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub enabled: bool,
    pub name: String,
    pub watch: Watch,
    pub actions: Vec<ActionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Watch {
    pub path: String,
    pub recursive: bool,
    pub target: WatchTarget,
    pub include_hidden: bool,
    pub patterns: Option<Vec<String>>,
    pub regex: Option<String>,
    pub exclude_patterns: Vec<String>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionConfig {
    #[serde(rename = "type")]
    pub type_: ActionType,

    // typeがCopy / Move のとき
    pub destination: Option<String>,
    pub overwrite: Option<bool>,
    pub verify_integrity: Option<bool>,
    pub preserve_structure: Option<bool>,

    // typeがCommand / Executeのとき
    pub working_dir: Option<String>,

    // typeがCommandのとき
    pub shell: Option<String>,
    pub command: Option<String>,

    // typeがExecuteのとき
    pub program: Option<String>,
    pub args: Option<Vec<String>>,
}

pub fn load_global_config(path: &Path) -> Result<GlobalConfig, AppError> {
	let content = std::fs::read_to_string(path)?;
	let config: GlobalConfig = toml::from_str(&content)
							.map_err(|e| AppError::TomlParse(e.to_string()))?;
	Ok(config)
}

pub fn load_rules_config(path: &Path) -> Result<RulesConfig, AppError> {
	let content = std::fs::read_to_string(path)?;
	let config: RulesConfig = toml::from_str(&content)
							.map_err(|e| AppError::TomlParse(e.to_string()))?;
	Ok(config)
}

pub fn validate_global_config(config: &GlobalConfig) -> Result<(), AppError> {
	// ここでグローバル設定のバリデーションを行う
	let log_file = &config.global.log_file;
	if log_file.trim().is_empty() {
		return Err(AppError::Validation("log_file が空文字列です。ファイル名を含む有効なパスを定義してください".to_string()));
	}
	let log_path = Path::new(log_file);
	if log_path.extension().is_none() {
		return Err(AppError::Validation(format!("log_file にファイル拡張子が指定されていません: {}", log_path.display())));
	}

	if log_path.is_dir() {
		return Err(AppError::Validation(format!("log_file にディレクトリが指定されています: {}", log_path.display())));
	}

	if let Some(parent) = log_path.parent() {
		if !parent.exists() {
			return Err(AppError::Validation(format!("log_file の親ディレクトリが存在しません。{}", parent.display())));
		}
	}

	Ok(())
}

pub fn validate_rules_config(config: &RulesConfig) -> Result<(), AppError> {
	// ここでルール設定のバリデーションを行う
	let rules = &config.rules;
	if rules.is_empty() {
		return Err(AppError::Validation("ルールが1つも定義されていません。少なくとも1つのルールを定義してください".to_string()));
	}
	rules.iter().enumerate().try_for_each(|(index, rule)| {
		if rule.name.trim().is_empty() {
			return Err(AppError::Validation(format!("{} 番目の name が空文字列です。ルールにわかりやすい名前を定義してください", index)));
		}
		if rule.actions.is_empty() {
			return Err(AppError::Validation(format!("監視ルール名 {} の actions(処理) が1つも定義されていません。少なくとも1つのアクションを定義してください", rule.name)));
		}
		if rule.watch.events.is_empty() {
			return Err(AppError::Validation(format!("監視ルール名 {} の watch.events(検知イベント) が1つも定義されていません。少なくとも1つのイベントを定義してください", rule.name)));
		}
		if (rule.watch.patterns.is_some() && rule.watch.regex.is_some()) || (rule.watch.patterns.is_none() && rule.watch.regex.is_none()) {
			return Err(AppError::Validation(format!("監視ルール名 {} の watch.patterns と watch.regex は片方のみ定義できます。どちらか一方を定義してください", rule.name)));
		}
		
		for action in &rule.actions{
			validate_action(action, &rule.name)?;
		}

		let watch_path = Path::new(&rule.watch.path);
		if !watch_path.is_dir() {
			return Err(AppError::Validation(format!("監視ルール名 {} の watch.path が存在しません: {}", rule.name, watch_path.display())));
		}
		Ok(())
	})?;

	Ok(())
}

fn validate_action(action: &ActionConfig, rule_name: &str) -> Result<(), AppError> {
	match action.type_ {
		ActionType::Copy | ActionType::Move => {
			if action.destination.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Copy / Move のとき、destination(コピー先/移動先) を定義してください", rule_name)));
			}

			if action.overwrite.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Copy / Move のとき、overwrite(上書きの有無) を定義してください", rule_name)));
			}

			if action.preserve_structure.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Copy / Move のとき、preserve_structure(ディレクトリ構造を保持するか) を定義してください", rule_name)));
			}
			if action.verify_integrity.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Copy / Move のとき、verify_integrity(コピー後にファイルの完全性を検証するか) を定義してください", rule_name)));
			}
			if let Some(dest) = &action.destination {
				if !Path::new(dest).is_dir(){
					return Err(AppError::Validation(format!("監視ルール名 {} のアクションの destination(コピー先/移動先) が存在しません", rule_name)));
				}
			} 
		}

		ActionType::Command => {
			if action.shell.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Command のとき、shell(コマンドを実行するシェル) を定義してください", rule_name)));
			}
			
			if action.command.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Command のとき、command(実行するコマンド) を定義してください", rule_name)));
			}

			if action.working_dir.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Command のとき、working_dir(コマンド/プログラムを実行するディレクトリ) を定義してください", rule_name)));
			}
		}
		ActionType::Execute => {
			if action.program.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Execute のとき、program(実行するプログラム) を定義してください", rule_name)));
			}
			if action.args.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Execute のとき、args(プログラムに渡す引数) を定義してください。引数がない場合は空の配列を指定してください", rule_name)));
			}
			if action.working_dir.is_none() {
				return Err(AppError::Validation(format!("監視ルール名 {} のアクションの type が Execute のとき、working_dir(コマンド/プログラムを実行するディレクトリ) を定義してください", rule_name)));
			}
		}
	}
	Ok(())
}
