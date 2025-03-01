use crate::error::{LgtvError, Result};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const SEARCH_CONFIG: &[&str] = &[
    "/etc/lgtv/config.json",
    "~/.lgtv/config.json",
    "/opt/venvs/lgtv/config/config.json",
];

pub fn find_config() -> Result<PathBuf> {
    let mut writable_path = None;
    
    // First, try to find an existing config file
    for &config_path in SEARCH_CONFIG {
        let path = expand_path(config_path)?;
        let dir = path.parent().ok_or_else(|| {
            LgtvError::ConfigError(format!("Invalid config path: {}", path.display()))
        })?;
        
        if path.exists() && path.is_file() && fs::metadata(&path)?.permissions().readonly() == false {
            return Ok(path);
        }
        
        if dir.exists() && fs::metadata(dir)?.permissions().readonly() == false {
            writable_path = Some(path);
        } else if writable_path.is_none() && dir.parent().map_or(false, |p| p.exists()) {
            // Try to create the directory
            match fs::create_dir_all(dir) {
                Ok(_) => writable_path = Some(path),
                Err(e) => log::debug!("Failed to create directory {}: {}", dir.display(), e),
            }
        }
    }
    
    // If no existing config is found, use the first writable path
    writable_path.ok_or_else(|| {
        LgtvError::ConfigError(format!(
            "Cannot find suitable config path to write, create one in {}",
            SEARCH_CONFIG.join(" or ")
        ))
    })
}

pub fn read_config(path: &Path) -> Result<Value> {
    let config_str = fs::read_to_string(path)?;
    let config: Value = serde_json::from_str(&config_str)?;
    Ok(config)
}

pub fn write_config(path: &Path, config: &Value) -> Result<()> {
    let config_str = serde_json::to_string_pretty(config)?;
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let mut file = File::create(path)?;
    file.write_all(config_str.as_bytes())?;
    Ok(())
}

fn expand_path(path: &str) -> Result<PathBuf> {
    let path_str = if path.starts_with("~") {
        match dirs::home_dir() {
            Some(home) => home.join(&path[2..]).to_string_lossy().into_owned(),
            None => {
                return Err(LgtvError::ConfigError(
                    "Could not determine home directory".to_string(),
                ))
            }
        }
    } else {
        path.to_string()
    };
    
    Ok(PathBuf::from(path_str))
}
