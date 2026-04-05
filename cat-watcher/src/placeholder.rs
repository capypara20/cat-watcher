use regex::Regex;
use crate::error::AppError;

pub fn validate_placeholders(text: &str, rule_name: &str, field_name: &str) -> Result<(), AppError> {
	// 有効なブレースホルダー
	 let valid = [
        "FullName", "DirectoryName", "Name", "BaseName", "Extension",
        "RelativePath", "WatchPath", "Destination",
        "Date", "Time", "DateTime",
    ];

	let re = Regex::new(r"\{\{|\}\}|\{([A-Za-z]+)\}").unwrap();
	for caps in re.captures_iter(text){
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