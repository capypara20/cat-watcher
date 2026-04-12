use std::path::Path;
use std::sync::LazyLock;

use crate::error::AppError;
use chrono::Local;
use regex::Regex;

pub struct PlaceholderContext {
    pub full_name: String,      //{FullName}		= 絶対パス
    pub directory_name: String, //{DirectoryName}	= 絶対パスからファイル名を除いた部分
    pub name: String,           //{Name}			= ファイル名
    pub base_name: String,      //{BaseName}		= 拡張子を除いたファイル名
    pub extension: String,      //{Extension}		= ファイルの拡張子
    pub relative_path: String,  //{RelativePath}	= 相対パス
    pub watch_path: String,     //{WatchPath}		= 監視対象のパス
    pub destination: String,    //{Destination}		= コピー先/移動先のパス
    pub date: String,           //{Date}			= 日付
    pub time: String,           //{Time}			= 時刻
    pub datetime: String,       //{DateTime}		= 日付と時刻
}

impl PlaceholderContext {
    pub fn new(full_path: &Path, watch_path: &Path, destination: &str) -> Self {
        let now = Local::now();
        Self {
            full_name: full_path.to_string_lossy().replace('\\', "/"),
            directory_name: full_path
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default(),
            name: full_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            base_name: full_path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            extension: full_path
                .extension()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            relative_path: full_path
                .strip_prefix(watch_path)
                .map(|n| n.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default(),
            watch_path: watch_path.to_string_lossy().replace('\\', "/"),
            destination: destination.replace('\\', "/"),
            date: now.format("%Y%m%d").to_string(),
            time: now.format("%H%M%S").to_string(),
            datetime: now.format("%Y%m%d_%H%M%S").to_string(),
        }
    }
}

static PLACEHOLDER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{|\}\}|\{([A-Za-z]+)\}").unwrap());

pub fn expand_placeholders(template: &str, ctx: &PlaceholderContext) -> Result<String, AppError> {
    let result = PLACEHOLDER_REGEX.replace_all(template, |caps: &regex::Captures| {
        if let Some(name) = caps.get(1) {
            match name.as_str() {
                "FullName" => ctx.full_name.clone(),
                "DirectoryName" => ctx.directory_name.clone(),
                "Name" => ctx.name.clone(),
                "BaseName" => ctx.base_name.clone(),
                "Extension" => ctx.extension.clone(),
                "RelativePath" => ctx.relative_path.clone(),
                "WatchPath" => ctx.watch_path.clone(),
                "Destination" => ctx.destination.clone(),
                "Date" => ctx.date.clone(),
                "Time" => ctx.time.clone(),
                "DateTime" => ctx.datetime.clone(),
                _ => caps.get(0).unwrap().as_str().to_string(), // 不明なプレースホルダーはそのまま
            }
        } else {
            match caps.get(0).unwrap().as_str() {
                "{{" => "{".to_string(),
                "}}" => "}".to_string(),
                other => other.to_string(), // それ以外はそのまま
            }
        }
    });
    Ok(result.to_string())
}

pub fn validate_placeholders(
    text: &str,
    rule_name: &str,
    field_name: &str,
) -> Result<(), AppError> {
    // 有効なブレースホルダー
    let valid = [
        "FullName",
        "DirectoryName",
        "Name",
        "BaseName",
        "Extension",
        "RelativePath",
        "WatchPath",
        "Destination",
        "Date",
        "Time",
        "DateTime",
    ];

    for caps in PLACEHOLDER_REGEX.captures_iter(text) {
        if let Some(name) = caps.get(1) {
            let placeholder = name.as_str();
            if !valid.contains(&placeholder) {
                return Err(AppError::Validation(format!(
                    "監視ルール名 {} の {} に未知のブレースホルダーがあります {{{}}}",
                    rule_name, field_name, placeholder
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// テスト用の固定値コンテキストを生成する
    fn make_ctx() -> PlaceholderContext {
        PlaceholderContext {
            full_name: "C:/data/incoming/report.csv".to_string(),
            directory_name: "C:/data/incoming".to_string(),
            name: "report.csv".to_string(),
            base_name: "report".to_string(),
            extension: "csv".to_string(),
            relative_path: "report.csv".to_string(),
            watch_path: "C:/data/incoming".to_string(),
            destination: "C:/data/outgoing".to_string(),
            date: "20260412".to_string(),
            time: "153000".to_string(),
            datetime: "20260412_153000".to_string(),
        }
    }

    // =========================================================
    // expand_placeholders: 各プレースホルダ単体の展開
    // =========================================================

    #[test]
    fn test_expand_name() {
        let ctx = make_ctx();
        let result = expand_placeholders("{Name}", &ctx).unwrap();
        assert_eq!(result, "report.csv");
    }

    #[test]
    fn test_expand_base_name() {
        let ctx = make_ctx();
        let result = expand_placeholders("{BaseName}", &ctx).unwrap();
        assert_eq!(result, "report");
    }

    #[test]
    fn test_expand_extension() {
        let ctx = make_ctx();
        let result = expand_placeholders("{Extension}", &ctx).unwrap();
        assert_eq!(result, "csv");
    }

    #[test]
    fn test_expand_full_name() {
        let ctx = make_ctx();
        let result = expand_placeholders("{FullName}", &ctx).unwrap();
        assert_eq!(result, "C:/data/incoming/report.csv");
    }

    #[test]
    fn test_expand_directory_name() {
        let ctx = make_ctx();
        let result = expand_placeholders("{DirectoryName}", &ctx).unwrap();
        assert_eq!(result, "C:/data/incoming");
    }

    #[test]
    fn test_expand_relative_path() {
        let ctx = make_ctx();
        let result = expand_placeholders("{RelativePath}", &ctx).unwrap();
        assert_eq!(result, "report.csv");
    }

    #[test]
    fn test_expand_watch_path() {
        let ctx = make_ctx();
        let result = expand_placeholders("{WatchPath}", &ctx).unwrap();
        assert_eq!(result, "C:/data/incoming");
    }

    #[test]
    fn test_expand_destination() {
        let ctx = make_ctx();
        let result = expand_placeholders("{Destination}", &ctx).unwrap();
        assert_eq!(result, "C:/data/outgoing");
    }

    #[test]
    fn test_expand_date() {
        let ctx = make_ctx();
        let result = expand_placeholders("{Date}", &ctx).unwrap();
        assert_eq!(result, "20260412");
    }

    #[test]
    fn test_expand_time() {
        let ctx = make_ctx();
        let result = expand_placeholders("{Time}", &ctx).unwrap();
        assert_eq!(result, "153000");
    }

    #[test]
    fn test_expand_datetime() {
        let ctx = make_ctx();
        let result = expand_placeholders("{DateTime}", &ctx).unwrap();
        assert_eq!(result, "20260412_153000");
    }

    // =========================================================
    // expand_placeholders: 複合・特殊ケース
    // =========================================================

    #[test]
    fn test_expand_multiple_placeholders() {
        let ctx = make_ctx();
        let result =
            expand_placeholders("{DirectoryName}/{BaseName}_{DateTime}.{Extension}", &ctx).unwrap();
        assert_eq!(result, "C:/data/incoming/report_20260412_153000.csv");
    }

    #[test]
    fn test_expand_escape_double_braces() {
        let ctx = make_ctx();
        let result = expand_placeholders("{{literal}}", &ctx).unwrap();
        assert_eq!(result, "{literal}");
    }

    #[test]
    fn test_expand_escape_mixed_with_placeholder() {
        let ctx = make_ctx();
        let result = expand_placeholders("{{prefix}}_{Name}", &ctx).unwrap();
        assert_eq!(result, "{prefix}_report.csv");
    }

    #[test]
    fn test_expand_no_placeholders() {
        let ctx = make_ctx();
        let result = expand_placeholders("plain text without placeholders", &ctx).unwrap();
        assert_eq!(result, "plain text without placeholders");
    }

    #[test]
    fn test_expand_extension_empty_for_no_extension_file() {
        let mut ctx = make_ctx();
        ctx.extension = "".to_string();
        let result = expand_placeholders("{BaseName}.{Extension}", &ctx).unwrap();
        assert_eq!(result, "report.");
    }

    // =========================================================
    // validate_placeholders
    // =========================================================

    #[test]
    fn test_validate_known_placeholder_ok() {
        assert!(validate_placeholders("{Name}", "rule1", "destination").is_ok());
    }

    #[test]
    fn test_validate_all_known_placeholders_ok() {
        let all = "{FullName}{DirectoryName}{Name}{BaseName}{Extension}{RelativePath}{WatchPath}{Destination}{Date}{Time}{DateTime}";
        assert!(validate_placeholders(all, "rule1", "destination").is_ok());
    }

    #[test]
    fn test_validate_unknown_placeholder_error() {
        let result = validate_placeholders("{Unknown}", "rule1", "destination");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_escaped_braces_ok() {
        assert!(validate_placeholders("{{escaped}}", "rule1", "destination").is_ok());
    }

    #[test]
    fn test_validate_no_placeholders_ok() {
        assert!(validate_placeholders("just plain text", "rule1", "destination").is_ok());
    }

    #[test]
    fn test_validate_error_message_contains_rule_and_field() {
        let result = validate_placeholders("{Bad}", "my-rule", "action.command");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("my-rule"));
        assert!(err_msg.contains("action.command"));
        assert!(err_msg.contains("{Bad}"));
    }

    // =========================================================
    // PlaceholderContext::new
    // =========================================================

    #[test]
    fn test_new_context_basic() {
        let full_path = PathBuf::from("C:/data/incoming/report.csv");
        let watch_path = PathBuf::from("C:/data/incoming");
        let ctx = PlaceholderContext::new(&full_path, &watch_path, "C:/dest");

        assert_eq!(ctx.full_name, "C:/data/incoming/report.csv");
        assert_eq!(ctx.directory_name, "C:/data/incoming");
        assert_eq!(ctx.name, "report.csv");
        assert_eq!(ctx.base_name, "report");
        assert_eq!(ctx.extension, "csv");
        assert_eq!(ctx.relative_path, "report.csv");
        assert_eq!(ctx.watch_path, "C:/data/incoming");
        assert_eq!(ctx.destination, "C:/dest");
    }

    #[test]
    fn test_new_context_no_extension() {
        let full_path = PathBuf::from("C:/data/incoming/Makefile");
        let watch_path = PathBuf::from("C:/data/incoming");
        let ctx = PlaceholderContext::new(&full_path, &watch_path, "C:/dest");

        assert_eq!(ctx.name, "Makefile");
        assert_eq!(ctx.base_name, "Makefile");
        assert_eq!(ctx.extension, "");
    }

    #[test]
    fn test_new_context_date_format() {
        let full_path = PathBuf::from("C:/data/file.txt");
        let watch_path = PathBuf::from("C:/data");
        let ctx = PlaceholderContext::new(&full_path, &watch_path, "");

        // YYYYMMDD = 8桁
        assert_eq!(ctx.date.len(), 8);
        // HHmmss = 6桁
        assert_eq!(ctx.time.len(), 6);
        // YYYYMMDD_HHmmss = 15桁
        assert_eq!(ctx.datetime.len(), 15);
        assert!(ctx.datetime.contains('_'));
    }

    #[test]
    fn test_new_context_subdirectory() {
        let full_path = PathBuf::from("C:/data/incoming/sub/deep/file.txt");
        let watch_path = PathBuf::from("C:/data/incoming");
        let ctx = PlaceholderContext::new(&full_path, &watch_path, "C:/dest");

        assert_eq!(ctx.relative_path, "sub/deep/file.txt");
        assert_eq!(ctx.name, "file.txt");
    }
}
