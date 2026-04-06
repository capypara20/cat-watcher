use std::sync::LazyLock;

use regex::Regex;
use crate::error::AppError;

static PLACEHOLDER_REGEX: LazyLock<Regex> = LazyLock::new(|| 
	Regex::new(r"\{\{|\}\}|\{([A-Za-z]+)\}").unwrap()
);

pub fn validate_placeholders(text: &str, rule_name: &str, field_name: &str) -> Result<(), AppError> {
	// 有効なブレースホルダー
	 let valid = [
        "FullName", "DirectoryName", "Name", "BaseName", "Extension",
        "RelativePath", "WatchPath", "Destination",
        "Date", "Time", "DateTime",
    ];

	for caps in PLACEHOLDER_REGEX.captures_iter(text){
		if let Some(name) = caps.get(1){
			let placeholder = name.as_str();
			if !valid.contains(&placeholder){
				return Err(AppError::Validation(
					format!("監視ルール名 {} の {} に未知のブレースホルダーがあります {{{}}}", rule_name, field_name, placeholder)
				));
			}
		}
	}
	Ok(())
}