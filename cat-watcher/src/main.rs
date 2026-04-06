use crate::error::AppError;
use std::path::Path;
mod error;
mod config;
mod placeholder;

fn main() -> Result<(), AppError>{
	let global_conf_path = "config/global.toml";
	let global_conf = config::load_global_config(&Path::new(global_conf_path))?;
	// println!("{:#?}", global_conf);

	let rules_conf_path = "config/rules.toml";
	let rules_conf = config::load_rules_config(&Path::new(rules_conf_path))?;
	// println!("{:#?}", rules_conf);

	config::validate_global_config(&global_conf)?;
	config::validate_rules_config(&rules_conf)?;
	Ok(())	
}