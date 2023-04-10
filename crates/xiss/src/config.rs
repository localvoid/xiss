use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, WrapErr};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_output_path")]
    pub output: PathBuf,
    #[serde(default = "default_include_path")]
    pub include: PathBuf,
    #[serde(default)]
    pub map: ConfigMap,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> eyre::Result<Config> {
        let config = fs::read_to_string(path.as_ref())
            .wrap_err_with(|| format!("Failed to load config from '{:?}'", path.as_ref()))?;
        let config = serde_json::from_str(&config)
            .wrap_err_with(|| format!("Failed to deserialize config file '{:?}'", path.as_ref()))?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: "build/css/".into(),
            include: "css".into(),
            map: ConfigMap::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigMap {
    #[serde(default = "default_config_map_path")]
    pub path: PathBuf,
    #[serde(default = "default_config_map_lock_path")]
    pub lock: PathBuf,
    #[serde(default)]
    pub exclude: ConfigMapExclude,
}

impl Default for ConfigMap {
    fn default() -> Self {
        Self {
            path: "xiss-map.csv".into(),
            lock: "xiss-map.lock.csv".into(),
            exclude: ConfigMapExclude::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigMapExclude {
    #[serde(default)]
    pub class: Vec<String>,
    #[serde(default)]
    pub var: Vec<String>,
    #[serde(default)]
    pub keyframes: Vec<String>,
}

impl Default for ConfigMapExclude {
    fn default() -> Self {
        Self {
            class: Vec::default(),
            var: Vec::default(),
            keyframes: Vec::default(),
        }
    }
}

fn default_output_path() -> PathBuf {
    "build/css/".into()
}

fn default_include_path() -> PathBuf {
    "css/".into()
}

fn default_config_map_path() -> PathBuf {
    "xiss-map.csv".into()
}

fn default_config_map_lock_path() -> PathBuf {
    "xiss-map.lock.csv".into()
}
