use anyhow::Result;
use serde::Deserialize;
use std::{fs::File, path::Path};

#[derive(Clone, Deserialize)]
pub struct Config {
    pub shell: String,
    pub shell_args: Vec<String>,
    pub read_buf_size: usize,
    pub channel_buf_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: "/usr/bin/bash".into(),
            shell_args: vec!["-i".into()],
            read_buf_size: 1024,
            channel_buf_size: 100,
        }
    }
}

impl Config {
    pub fn from_file(file: File) -> Result<Self> {
        let config: Self = serde_json::from_reader(file)?;
        Ok(config)
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let config = Self::from_file(file)?;
        Ok(config)
    }
}
