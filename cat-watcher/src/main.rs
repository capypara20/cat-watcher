mod error;
use error::AppError;

fn might_fail(ok: bool) -> Result<String, AppError> {
    if ok {
        Ok("成功！".to_string())
    } else {
        // Err(AppError::Config("global.toml が見つかりません".to_string()))
        Err(AppError::Validation("バリデーションチェックエラー".to_string()))
    }
}

fn main() {
    match might_fail(false) {
        Ok(msg) => println!("{}", msg),
        Err(e)  => println!("エラー発生: {}", e),
    }
}
