use crate::error::{LgtvError, Result};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/etc/lgtv/config.json")];

    // XDG_CONFIG_HOME or ~/.config
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(xdg).join("lgtv/config.json"));
    } else if let Some(home) = home_dir() {
        paths.push(home.join(".config/lgtv/config.json"));
    }

    // Legacy ~/.lgtv
    if let Some(home) = home_dir() {
        paths.push(home.join(".lgtv/config.json"));
    }

    paths.push(PathBuf::from("/opt/venvs/lgtv/config/config.json"));

    paths
}

pub fn find_config() -> Result<PathBuf> {
    let search_paths = config_search_paths();
    let mut writable_path = None;

    // First, try to find an existing config file
    for path in &search_paths {
        if path.exists() && path.is_file() {
            if let Ok(meta) = fs::metadata(path) {
                if !meta.permissions().readonly() {
                    return Ok(path.clone());
                }
            }
        }
    }

    // If no existing config, find a writable location
    for path in &search_paths {
        let dir = match path.parent() {
            Some(d) => d,
            None => continue,
        };

        if dir.exists() {
            if let Ok(meta) = fs::metadata(dir) {
                if !meta.permissions().readonly() {
                    writable_path = Some(path.clone());
                    break;
                }
            }
        } else if writable_path.is_none() {
            if let Some(parent) = dir.parent() {
                if parent.exists() {
                    match fs::create_dir_all(dir) {
                        Ok(_) => {
                            writable_path = Some(path.clone());
                            break;
                        }
                        Err(e) => {
                            log::debug!("Failed to create directory {}: {}", dir.display(), e)
                        }
                    }
                }
            }
        }
    }

    writable_path.ok_or_else(|| {
        let paths_str: Vec<String> = search_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        LgtvError::ConfigError(format!(
            "Cannot find suitable config path to write, create one in {}",
            paths_str.join(" or ")
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
