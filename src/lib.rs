pub mod structs;
use color_eyre::eyre::Result;
use notify::event::{DataChange, ModifyKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use notify::{Event, EventKind};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use structs::{Action, VAULTS_FILE, Vault, Vaults};
use tracing::{error, info};

pub fn watch_vault_list(tx: mpsc::Sender<Event>) -> Result<()> {
    let (watcher_tx, watcher_rx) = channel();
    let mut watcher = RecommendedWatcher::new(watcher_tx, Config::default())?;
    watcher.watch(Path::new(&VAULTS_FILE.clone()), RecursiveMode::NonRecursive)?;
    for event in watcher_rx {
        match event {
            Ok(event) => {
                if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
                    tx.send(event)?;
                }
            }
            Err(e) => {
                error!("Error watching vaults file: {}", e);
            }
        }
    }
    Ok(())
}

pub fn watch_vault_plugins(tx: mpsc::Sender<Action>, vault_path: PathBuf) -> Result<()> {
    let (watcher_tx, watcher_rx) = channel();
    let mut watcher = RecommendedWatcher::new(watcher_tx, Config::default())?;
    watcher.watch(
        &vault_path.join(".obsidian").join("plugins"),
        RecursiveMode::Recursive,
    )?;
    for event in watcher_rx {
        match event {
            Ok(event) => {
                if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
                    tx.send(Action::VaultPluginChanged(vault_path.clone()))?;
                }
            }
            Err(e) => {
                error!("Error watching vault: {}", e);
            }
        }
    }
    Ok(())
}

pub async fn setup_vault_listeners(
    tx: mpsc::Sender<Action>,
    rx: mpsc::Receiver<Action>,
) -> Result<()> {
    let (watcher_tx, watcher_rx) = channel();
    let watcher_paths: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut watcher_global = RecommendedWatcher::new(watcher_tx, Config::default())?;
    info!("Init watcher");
    let _thread_vault_listeners: JoinHandle<Result<()>> = std::thread::spawn(move || {
        info!("Init vault watchers");

        let vaults = Vaults::new();
        tx.send(Action::ChangeOpenVaults(vaults.get_open_vaults()))
            .unwrap();
        for action in rx {
            match action {
                Action::ChangeOpenVaults(open_vaults) => {
                    let mut watcher_paths = watcher_paths.lock().unwrap();
                    for path in watcher_paths.clone() {
                        let _ = watcher_global.unwatch(&path);
                    }
                    watcher_paths.clear();
                    for path in open_vaults.iter().map(|v| v.path.clone()) {
                        watcher_global.watch(&path, RecursiveMode::Recursive)?;
                        watcher_paths.insert(path.clone());
                        info!(vault = ?path, "Adding vault");
                    }
                }
                Action::TerminateVaultListeners => {
                    let mut watcher_paths = watcher_paths.lock().unwrap();
                    for path in watcher_paths.clone() {
                        let _ = watcher_global.unwatch(&path);
                    }
                    watcher_paths.clear();
                }
                Action::VaultPluginChanged(vault_path) => {
                    tx.send(Action::UpdatePlugins(vault_path))?;
                }
                _ => {}
            }
        }
        Ok(())
    });
    info!("Init watcher logging");
    for event in watcher_rx {
        match event {
            Ok(event) => {
                println!("Event: {:?}", event);
                info!("Event: {:?}", event);
            }
            Err(e) => println!("Error: {:?}", e),
        }
    }
    _thread_vault_listeners.join().unwrap()?;

    Ok(())
}
