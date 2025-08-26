pub mod structs;
use color_eyre::eyre::Result;
use fast_rsync::{Signature, SignatureOptions, apply, diff};
use ignore::Walk;
use notify::event::ModifyKind;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use notify::{Event, EventKind};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, channel};
use std::sync::{Arc, Mutex};
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
    tx: tokio::sync::broadcast::Sender<Action>,
    rx: &mut tokio::sync::broadcast::Receiver<Action>,
) -> Result<()> {
    let (watcher_tx, watcher_rx) = channel();
    let watcher_paths: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut watcher_global = RecommendedWatcher::new(watcher_tx, Config::default())?;
    info!("Init watcher");
    let rx2 = tx.subscribe();
    let _thread_vault_listeners: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
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
                    info!("Vault plugin changed: {}", vault_path.display());
                    tx.send(Action::UpdatePlugins(vault_path))?;
                }
                _ => {}
            }
        }
        Ok(())
    });
    info!("Init watcher logging");
    let watcher_thread = tokio::spawn(async move {
        for event in watcher_rx {
            match event {
                Ok(event) => {
                    println!("Event: {:?}", event);
                    info!("Event: {:?}", event);
                }
                Err(e) => println!("Error: {:?}", e),
            }
        }
    });
    watcher_thread.await?;
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
    let options = SignatureOptions {
        block_size: 1024,    // split into 1KB blocks
        crypto_hash_size: 8, // store 8 bytes of crypto hash
    };

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

            let sig = Signature::calculate(&dst_bytes, options);
            let indexed_sig = sig.index();

            let mut delta: Vec<u8> = Vec::new();
            diff(&indexed_sig, &src_bytes, &mut delta)?;
            dbg!(&delta);

            let mut new_bytes = Vec::new();
            apply(&dst_bytes, &delta, &mut new_bytes)?;
            write_file(&dst, &new_bytes)?;
            let new_bytes_str = String::from_utf8(new_bytes)?;
            println!("File {} updated", dst.display());
            println!("{}", new_bytes_str);
        } else {
            write_file(&dst, &src_bytes)?;
        }
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
