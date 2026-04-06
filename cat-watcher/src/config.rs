use std::path::Path;
use regex::Regex;
use serde::Deserialize;
use globset::Glob;

use crate::error::AppError;
use crate::placeholder::validate_placeholders;

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

		// globパターンチェック
		if let Some(patterns) = &rule.watch.patterns{
			for pt in patterns {
				Glob::new(pt).map_err(|e| AppError::Validation(
					format!("監視ルール名 {} の patterns に無効な glob があります '{}': {}",rule.name, pt, e)
				))?;
			}
		}

		// 正規表現チェック
		if let Some(regex_str) = &rule.watch.regex{
			Regex::new(regex_str).map_err(|e| AppError::Validation(
				format!("監視ルール名 {} の regex に無効な正規表現があります '{}': {}", rule.name, regex_str, e)
			))?;
		}

		for glob in &rule.watch.exclude_patterns{
			Glob::new(glob).map_err(|e| AppError::Validation(
				format!("監視ルール名 {} の exclude_patterns に無効な glob があります '{}': {}", rule.name, glob, e)
			))?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	fn sanitize_path(path: &std::path::Path) -> String {
		path.to_str().unwrap().replace('\\', "/")
	}

	// =========================================================
	// ヘルパー: バリデーションで watch.path / destination に
	// 実在するディレクトリが必要なので、一時ディレクトリのパスを
	// テンプレートに埋め込む
	// =========================================================

	fn make_rules_toml(watch_path: &str, action_block: &str) -> String {
		let watch_path = watch_path.replace('\\', "/");
		format!(r#"
			[[rules]]
			enabled = true
			name = "test-rule"

			[rules.watch]
			path = "{watch_path}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = []
			events = ["create", "modify"]

			[[rules.actions]]
			{action_block}
		"#)
	}

	fn make_rules_toml_with_watch(watch_block: &str, watch_path: &str) -> String {
		let watch_path = watch_path.replace('\\', "/");
		format!(r#"
			[[rules]]
			enabled = true
			name = "test-rule"

			[rules.watch]
			path = "{watch_path}"
			{watch_block}

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hello"
			working_dir = ""
		"#)
	}

	// =========================================================
	// GlobalConfig パーステスト
	// =========================================================

	#[test]
	fn test_parse_global_config() {
		let toml_str = r#"
			[global]
			log_level = "info"
			log_file = "logs/app.log"
			log_rotation = "daily"
			retry_count = 3
			retry_interval_ms = 1000
			dry_run = false
		"#;
		let config: GlobalConfig = toml::from_str(toml_str).unwrap();
		assert_eq!(config.global.retry_count, 3);
		assert_eq!(config.global.retry_interval_ms, 1000);
		assert_eq!(config.global.log_file, "logs/app.log");
	}

	#[test]
	fn test_parse_global_config_all_log_levels() {
		for level in &["trace", "debug", "info", "warn", "error"] {
			let toml_str = format!(r#"
				[global]
				log_level = "{level}"
				log_file = "logs/app.log"
				log_rotation = "daily"
				retry_count = 1
				retry_interval_ms = 500
				dry_run = false
			"#);
			let result: Result<GlobalConfig, _> = toml::from_str(&toml_str);
			assert!(result.is_ok(), "log_level '{}' のパースに失敗", level);
		}
	}

	#[test]
	fn test_parse_global_config_invalid_log_level() {
		let toml_str = r#"
			[global]
			log_level = "verbose"
			log_file = "logs/app.log"
			log_rotation = "daily"
			retry_count = 1
			retry_interval_ms = 500
			dry_run = false
		"#;
		let result: Result<GlobalConfig, _> = toml::from_str(toml_str);
		assert!(result.is_err());
	}

	#[test]
	fn test_parse_global_config_missing_field() {
		let toml_str = r#"
			[global]
			log_level = "info"
			log_file = "logs/app.log"
		"#;
		let result: Result<GlobalConfig, _> = toml::from_str(toml_str);
		assert!(result.is_err());
	}

	// =========================================================
	// GlobalConfig バリデーションテスト
	// =========================================================

	#[test]
	fn test_validate_global_config_empty_log_file() {
		let config = GlobalConfig {
			global: Global {
				log_level: LogLevel::Info,
				log_file: "   ".to_string(),
				log_rotation: LogRotation::Daily,
				retry_count: 3,
				retry_interval_ms: 1000,
				dry_run: false,
			},
		};
		let result = validate_global_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_global_config_no_extension() {
		let config = GlobalConfig {
			global: Global {
				log_level: LogLevel::Info,
				log_file: "logs/app".to_string(),
				log_rotation: LogRotation::Daily,
				retry_count: 3,
				retry_interval_ms: 1000,
				dry_run: false,
			},
		};
		let result = validate_global_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_global_config_parent_dir_not_exist() {
		let config = GlobalConfig {
			global: Global {
				log_level: LogLevel::Info,
				log_file: "nonexistent_dir_xyz/app.log".to_string(),
				log_rotation: LogRotation::Daily,
				retry_count: 3,
				retry_interval_ms: 1000,
				dry_run: false,
			},
		};
		let result = validate_global_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_global_config_valid() {
		let dir = tempdir().unwrap();
		let log_file = dir.path().join("app.log");
		let config = GlobalConfig {
			global: Global {
				log_level: LogLevel::Info,
				log_file: log_file.to_str().unwrap().to_string(),
				log_rotation: LogRotation::Daily,
				retry_count: 3,
				retry_interval_ms: 1000,
				dry_run: false,
			},
		};
		let result = validate_global_config(&config);
		assert!(result.is_ok());
	}

	// =========================================================
	// RulesConfig パーステスト
	// =========================================================

	#[test]
	fn test_parse_rules_config_copy_action() {
		let dir = tempdir().unwrap();
		let dest = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(dir.path()),
			&format!(r#"
				type = "copy"
				destination = "{}"
				overwrite = true
				verify_integrity = true
				preserve_structure = false
			"#, sanitize_path(dest.path())),
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules.len(), 1);
		assert_eq!(config.rules[0].name, "test-rule");
	}

	#[test]
	fn test_parse_rules_config_move_action() {
		let dir = tempdir().unwrap();
		let dest = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(dir.path()),
			&format!(r#"
				type = "move"
				destination = "{}"
				overwrite = false
				verify_integrity = false
				preserve_structure = true
			"#, sanitize_path(dest.path())),
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules[0].actions[0].overwrite, Some(false));
	}

	#[test]
	fn test_parse_rules_config_command_action() {
		let dir = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(dir.path()),
			r#"
				type = "command"
				shell = "cmd"
				command = "echo hello"
				working_dir = ""
			"#,
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules[0].actions[0].command, Some("echo hello".to_string()));
	}

	#[test]
	fn test_parse_rules_config_execute_action() {
		let dir = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(dir.path()),
			r#"
				type = "execute"
				program = "notepad.exe"
				args = ["{FullName}"]
				working_dir = ""
			"#,
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules[0].actions[0].program, Some("notepad.exe".to_string()));
	}

	#[test]
	fn test_parse_rules_config_with_regex_instead_of_patterns() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "regex-rule"

			[rules.watch]
			path = "{}"
			recursive = false
			target = "directory"
			include_hidden = true
			regex = "^report_\\d+\\.csv$"
			exclude_patterns = ["*.tmp"]
			events = ["create"]

			[[rules.actions]]
			type = "command"
			shell = "powershell"
			command = "echo test"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert!(config.rules[0].watch.patterns.is_none());
		assert!(config.rules[0].watch.regex.is_some());
	}

	#[test]
	fn test_parse_rules_config_all_events() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = false
			name = "all-events"

			[rules.watch]
			path = "{}"
			recursive = true
			target = "both"
			include_hidden = false
			patterns = ["*"]
			exclude_patterns = []
			events = ["create", "modify", "delete", "rename"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo done"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules[0].watch.events.len(), 4);
	}

	#[test]
	fn test_parse_rules_config_invalid_action_type() {
		let dir = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(dir.path()),
			r#"type = "delete""#,
		);
		let result: Result<RulesConfig, _> = toml::from_str(&toml_str);
		assert!(result.is_err());
	}

	// =========================================================
	// RulesConfig バリデーションテスト
	// =========================================================

	#[test]
	fn test_validate_rules_empty_rules() {
		let config = RulesConfig { rules: vec![] };
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_empty_name() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "  "

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = []
			events = ["create"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hi"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_empty_actions() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "no-actions"
			actions = []

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = []
			events = ["create"]
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_empty_events() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "no-events"

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = []
			events = []

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hi"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	// =========================================================
	// patterns / regex 排他チェック
	// =========================================================

	#[test]
	fn test_validate_rules_patterns_and_regex_both_present() {
		let dir = tempdir().unwrap();
		let toml_str = make_rules_toml_with_watch(
			r#"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			regex = "^test"
			exclude_patterns = []
			events = ["create"]
			"#,
			dir.path().to_str().unwrap(),
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_patterns_and_regex_both_absent() {
		let dir = tempdir().unwrap();
		let toml_str = make_rules_toml_with_watch(
			r#"
			recursive = true
			target = "file"
			include_hidden = false
			exclude_patterns = []
			events = ["create"]
			"#,
			dir.path().to_str().unwrap(),
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	// =========================================================
	// watch.path 存在チェック
	// =========================================================

	#[test]
	fn test_validate_rules_watch_path_not_exist() {
		let toml_str = make_rules_toml(
			"C:/nonexistent_path_xyz_12345",
			r#"
				type = "command"
				shell = "cmd"
				command = "echo hi"
				working_dir = ""
			"#,
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	// =========================================================
	// glob / regex 構文チェック
	// =========================================================

	#[test]
	fn test_validate_rules_invalid_glob_pattern() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "bad-glob"

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["[invalid"]
			exclude_patterns = []
			events = ["create"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hi"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_invalid_regex() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "bad-regex"

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			regex = "(unclosed"
			exclude_patterns = []
			events = ["create"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hi"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	#[test]
	fn test_validate_rules_invalid_exclude_glob() {
		let dir = tempdir().unwrap();
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "bad-exclude"

			[rules.watch]
			path = "{}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = ["[bad"]
			events = ["create"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo hi"
			working_dir = ""
		"#, sanitize_path(dir.path()));
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_err());
	}

	// =========================================================
	// validate_action: Copy / Move
	// =========================================================

	#[test]
	fn test_validate_action_copy_valid() {
		let dest = tempdir().unwrap();
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: Some(dest.path().to_str().unwrap().to_string()),
			overwrite: Some(true),
			verify_integrity: Some(true),
			preserve_structure: Some(false),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_ok());
	}

	#[test]
	fn test_validate_action_copy_missing_destination() {
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: None,
			overwrite: Some(true),
			verify_integrity: Some(true),
			preserve_structure: Some(false),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_copy_missing_overwrite() {
		let dest = tempdir().unwrap();
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: Some(dest.path().to_str().unwrap().to_string()),
			overwrite: None,
			verify_integrity: Some(true),
			preserve_structure: Some(false),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_copy_missing_verify_integrity() {
		let dest = tempdir().unwrap();
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: Some(dest.path().to_str().unwrap().to_string()),
			overwrite: Some(true),
			verify_integrity: None,
			preserve_structure: Some(false),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_copy_missing_preserve_structure() {
		let dest = tempdir().unwrap();
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: Some(dest.path().to_str().unwrap().to_string()),
			overwrite: Some(true),
			verify_integrity: Some(true),
			preserve_structure: None,
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_copy_destination_not_exist() {
		let action = ActionConfig {
			type_: ActionType::Copy,
			destination: Some("C:/nonexistent_dest_xyz_99999".to_string()),
			overwrite: Some(true),
			verify_integrity: Some(true),
			preserve_structure: Some(false),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_move_valid() {
		let dest = tempdir().unwrap();
		let action = ActionConfig {
			type_: ActionType::Move,
			destination: Some(dest.path().to_str().unwrap().to_string()),
			overwrite: Some(false),
			verify_integrity: Some(false),
			preserve_structure: Some(true),
			working_dir: None,
			shell: None,
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_ok());
	}

	// =========================================================
	// validate_action: Command
	// =========================================================

	#[test]
	fn test_validate_action_command_valid() {
		let action = ActionConfig {
			type_: ActionType::Command,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: Some("cmd".to_string()),
			command: Some("echo hello".to_string()),
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_ok());
	}

	#[test]
	fn test_validate_action_command_missing_shell() {
		let action = ActionConfig {
			type_: ActionType::Command,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: None,
			command: Some("echo hello".to_string()),
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_command_missing_command() {
		let action = ActionConfig {
			type_: ActionType::Command,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: Some("cmd".to_string()),
			command: None,
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_command_missing_working_dir() {
		let action = ActionConfig {
			type_: ActionType::Command,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: None,
			shell: Some("cmd".to_string()),
			command: Some("echo hello".to_string()),
			program: None,
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	// =========================================================
	// validate_action: Execute
	// =========================================================

	#[test]
	fn test_validate_action_execute_valid() {
		let action = ActionConfig {
			type_: ActionType::Execute,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: None,
			command: None,
			program: Some("notepad.exe".to_string()),
			args: Some(vec![]),
		};
		assert!(validate_action(&action, "test").is_ok());
	}

	#[test]
	fn test_validate_action_execute_missing_program() {
		let action = ActionConfig {
			type_: ActionType::Execute,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: None,
			command: None,
			program: None,
			args: Some(vec![]),
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_execute_missing_args() {
		let action = ActionConfig {
			type_: ActionType::Execute,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: Some("".to_string()),
			shell: None,
			command: None,
			program: Some("notepad.exe".to_string()),
			args: None,
		};
		assert!(validate_action(&action, "test").is_err());
	}

	#[test]
	fn test_validate_action_execute_missing_working_dir() {
		let action = ActionConfig {
			type_: ActionType::Execute,
			destination: None,
			overwrite: None,
			verify_integrity: None,
			preserve_structure: None,
			working_dir: None,
			shell: None,
			command: None,
			program: Some("notepad.exe".to_string()),
			args: Some(vec!["file.txt".to_string()]),
		};
		assert!(validate_action(&action, "test").is_err());
	}

	// =========================================================
	// 正常系: 全体を通したバリデーション
	// =========================================================

	#[test]
	fn test_validate_rules_config_valid_copy_rule() {
		let watch_dir = tempdir().unwrap();
		let dest_dir = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(watch_dir.path()),
			&format!(r#"
				type = "copy"
				destination = "{}"
				overwrite = true
				verify_integrity = false
				preserve_structure = true
			"#, sanitize_path(dest_dir.path())),
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_ok());
	}

	#[test]
	fn test_validate_rules_config_valid_command_rule() {
		let watch_dir = tempdir().unwrap();
		let toml_str = make_rules_toml(
			&sanitize_path(watch_dir.path()),
			r#"
				type = "command"
				shell = "powershell"
				command = "Get-Date"
				working_dir = ""
			"#,
		);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		let result = validate_rules_config(&config);
		assert!(result.is_ok());
	}

	#[test]
	fn test_validate_rules_config_multiple_rules() {
		let watch_dir = tempdir().unwrap();
		let dest_dir = tempdir().unwrap();
		let wp = sanitize_path(watch_dir.path());
		let dp = sanitize_path(dest_dir.path());
		let toml_str = format!(r#"
			[[rules]]
			enabled = true
			name = "rule-1"

			[rules.watch]
			path = "{wp}"
			recursive = true
			target = "file"
			include_hidden = false
			patterns = ["*.csv"]
			exclude_patterns = []
			events = ["create"]

			[[rules.actions]]
			type = "copy"
			destination = "{dp}"
			overwrite = true
			verify_integrity = true
			preserve_structure = false

			[[rules]]
			enabled = true
			name = "rule-2"

			[rules.watch]
			path = "{wp}"
			recursive = false
			target = "directory"
			include_hidden = true
			regex = "^backup"
			exclude_patterns = []
			events = ["create", "modify"]

			[[rules.actions]]
			type = "command"
			shell = "cmd"
			command = "echo done"
			working_dir = ""
		"#);
		let config: RulesConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config.rules.len(), 2);
		let result = validate_rules_config(&config);
		assert!(result.is_ok());
	}
}