use color_eyre::eyre::{Context, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::LazyLock};
use tracing::{error, info};

pub static VAULTS_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    let obsidian_dir_name = {
        #[cfg(target_os = "windows")]
        {
            "Obsdian"
        }
        #[cfg(not(target_os = "windows"))]
        {
            "obsidian"
        }
    };
    if let Some(base_dir) = directories::BaseDirs::new() {
        base_dir
            .config_dir()
            .join(obsidian_dir_name)
            .join("obsidian.json")
    } else {
        error!("Could not get config directory");
        std::process::exit(libc::EXIT_FAILURE);
    }
});

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Vault {
    pub path: PathBuf,
    pub ts: u64,
    pub open: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Vaults {
    vaults: HashMap<String, Vault>,
}

impl Vaults {
    pub fn new() -> Self {
        info!(file = ?VAULTS_FILE.clone(), "Reading vaults file");
        let Ok(file_contents) = std::fs::read_to_string(VAULTS_FILE.clone()) else {
            error!(
                "Could not read vaults file. Check if the file exists and that obsidian is properly installed"
            );
            std::process::exit(libc::EXIT_FAILURE);
        };
        let vaults: Result<Vaults> = serde_json::from_str(&file_contents)
            .context("Could not parse vaults file. Check if obsidian is properly installed");
        if let Ok(vaults) = vaults {
            return vaults;
        }
        error!("Could not parse vaults file. Check if obsidian is properly installed");
        std::process::exit(libc::EXIT_FAILURE);
    }
    pub fn get_open_vaults(&self) -> Vec<Vault> {
        self.vaults
            .iter()
            .flat_map(|(_, v)| {
                if v.open.unwrap_or(false) {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect_vec()
    }
    pub fn get_vaults(&self) -> Vec<Vault> {
        self.vaults.values().cloned().collect_vec()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Action {
    ChangeOpenVaults(Vec<Vault>),
    TerminateVaultListeners,
    VaultPluginChanged(PathBuf),
    UpdatePlugins(PathBuf),
    StartedSync,
    FinishedSync,
}
