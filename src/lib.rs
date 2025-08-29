pub mod cryptography;
pub mod structs;
use color_eyre::eyre::Result;
use cryptography::delta::Delta;
use ignore::Walk;
use notify::event::ModifyKind;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use notify::{Event, EventKind};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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

pub fn watch_vault_plugins(
    tx: mpsc::Sender<Action>,
    vault_path: PathBuf,
    not_syncing: Arc<AtomicBool>,
) -> Result<()> {
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
                    if not_syncing.load(std::sync::atomic::Ordering::Relaxed) {
                        tx.send(Action::VaultPluginChanged(vault_path.clone()))?;
                    }
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
    tx: tokio::sync::broadcast::Sender<Action>,
    rx: &mut tokio::sync::broadcast::Receiver<Action>,
    free: Arc<AtomicBool>,
) -> Result<()> {
    let (watcher_tx, watcher_rx) = channel();
    let watcher_paths: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut watcher_global = RecommendedWatcher::new(watcher_tx, Config::default())?;
    info!("Init watcher");
    let rx2 = tx.subscribe();
    let tx2 = tx.clone();
    let _thread_vault_listeners: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
        let tx = tx2;
        info!("Init vault watchers");

        let vaults = Vaults::new();
        tx.send(Action::ChangeOpenVaults(vaults.get_open_vaults()))
            .unwrap();
        let mut rx = rx2;
        loop {
            let Ok(action) = rx.recv().await else {
                break;
            };

            match action {
                Action::ChangeOpenVaults(open_vaults) => {
                    let mut watcher_paths = watcher_paths.lock().unwrap();
                    for path in watcher_paths.clone() {
                        let path = path.join(".obsidian").join("plugins");
                        let _ = watcher_global.unwatch(&path);
                    }
                    watcher_paths.clear();
                    for path in open_vaults.iter().map(|v| v.path.clone()) {
                        let path = path.join(".obsidian").join("plugins");
                        watcher_global.watch(&path, RecursiveMode::Recursive)?;
                        watcher_paths.insert(path.clone());
                        info!(vault = ?path, "Adding vault");
                    }
                }
                Action::TerminateVaultListeners => {
                    let mut watcher_paths = watcher_paths.lock().unwrap();
                    for path in watcher_paths.clone() {
                        let path = path.join(".obsidian").join("plugins");
                        let _ = watcher_global.unwatch(&path);
                    }
                    watcher_paths.clear();
                }
                Action::VaultPluginChanged(vault_path) => {
                    info!("Vault plugin changed: {}", vault_path.display());
                    //tx.send(Action::UpdatePlugins(vault_path))?;
                }
                _ => {}
            }
        }
        Ok(())
    });
    info!("Init watcher logging");

    let tx3 = tx.clone();
    let mut rx3 = tx.subscribe();
    let watcher_thread: tokio::task::JoinHandle<color_eyre::eyre::Result<()>> =
        tokio::spawn(async move {
            for event in watcher_rx {
                info!(free = ?free.load(std::sync::atomic::Ordering::Relaxed));
                if free.load(std::sync::atomic::Ordering::Relaxed) {
                    let vaults = Vaults::new();
                    let open_vaults = vaults.get_open_vaults();
                    match event {
                        Ok(event) => {
                            info!("Event: {:?}", event);
                            match event.kind {
                                EventKind::Modify(_)
                                | EventKind::Create(_)
                                | EventKind::Remove(_) => {
                                    for vault in open_vaults.iter() {
                                        if event.paths.iter().any(|p| p.starts_with(&vault.path)) {
                                            tx3.send(Action::UpdatePlugins(vault.path.clone()))?;
                                            break;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(e) => println!("Error: {:?}", e),
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            Ok(())
        });
    watcher_thread.await??;
    _thread_vault_listeners.await??;

    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(path)?.read_to_end(&mut buf)?;
    Ok(buf)
}

fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?; // create dirs if missing
    }
    fs::File::create(path)?.write_all(data)?;
    Ok(())
}

pub async fn sync_vault(from: PathBuf, to: PathBuf) -> Result<()> {
    let plugins_from = from.join(".obsidian").join("plugins");
    let plugins_to = to.join(".obsidian").join("plugins");
    dbg!(&plugins_from, &plugins_to);
    let walk_result = Walk::new(&plugins_from);
    dbg!("Finished Walk");

    let from_community_plugins = from.join(".obsidian").join("community-plugins.json");
    let to_community_plugins = to.join(".obsidian").join("community-plugins.json");

    sync_file(from_community_plugins, to_community_plugins)?;

    for entry in walk_result {
        let entry = entry?;
        dbg!(&entry);
        if entry.file_type().unwrap().is_dir() {
            continue;
        }
        let path = entry.path();
        dbg!(&path);
        let dst = plugins_to.join(path.strip_prefix(&plugins_from)?);
        let src_bytes = read_file(path)?;
        if dst.exists() {
            let dst_bytes = read_file(&dst)?;
            let del = Delta::new();
            let delta = del.generate_delta(&dst_bytes, &src_bytes);
            delta.apply(&src_bytes, dst.clone()).unwrap();
        } else {
            write_file(&dst, &src_bytes)?;
        }
    }
    // --- Delete files that no longer exist in "from" ---
    for entry in Walk::new(&plugins_to).filter_map(Result::ok) {
        if entry.file_type().expect("NO file type found").is_file() {
            let rel_path = entry.path().strip_prefix(&plugins_to)?;
            let src_path = plugins_from.join(rel_path);
            if !src_path.exists() {
                dbg!("Deleting", &entry.path());
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

pub fn sync_file(from: PathBuf, to: PathBuf) -> Result<()> {
    let src_bytes = read_file(&from)?;
    if to.exists() {
        let dst_bytes = read_file(&to)?;
        let del = Delta::new();
        let delta = del.generate_delta(&dst_bytes, &src_bytes);
        delta.apply(&src_bytes, to.clone()).unwrap();
    } else {
        write_file(&to, &src_bytes)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_transfer() {
        let from = PathBuf::from("/Users/jayansunil/Dev/rust/obsidian_syncer/test/from");
        let to = PathBuf::from("/Users/jayansunil/Dev/rust/obsidian_syncer/test/to");
        sync_vault(from, to).await.unwrap();
    }
}
