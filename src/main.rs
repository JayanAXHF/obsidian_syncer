mod errors;
mod logging;
use std::fs::read_dir;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use color_eyre::eyre::Result;
use itertools::Itertools;
use obsidian_syncer::structs::*;
use obsidian_syncer::sync_vault;
use tokio::sync::broadcast;
use tracing::debug;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    errors::init()?;
    let (tx, mut rx1) = broadcast::channel(100);
    let rx2 = tx.subscribe();
    info!("Test logging");
    let vaults = Vaults::new();
    let is_free = Arc::new(AtomicBool::new(true));

    let (list_watcher_tx, list_watcher_rx) = std::sync::mpsc::channel();
    let _thread_vault_list = std::thread::spawn(move || {
        obsidian_syncer::watch_vault_list(list_watcher_tx).unwrap();
    });
    let tx_move = tx.clone();
    let is_not_syncing = Arc::clone(&is_free);
    let _thread_vault_listeners = tokio::spawn(async move {
        println!("Starting vault listeners");

        obsidian_syncer::setup_vault_listeners(
            tx_move.clone(),
            &mut rx1,
            Arc::clone(&is_not_syncing),
        )
        .await
        .unwrap();
    });
    let tx_move = tx.clone();
    let _thread_syncer: tokio::task::JoinHandle<std::result::Result<(), color_eyre::eyre::Error>> =
        tokio::spawn(async move {
            let is_free1 = Arc::clone(&is_free);
            info!("Starting syncer");
            let mut rx = rx2;
            loop {
                let event = if let Ok(e) = rx.recv().await {
                    e
                } else {
                    break;
                };
                info!("Syncer Event: {:?}", event);
                match event {
                    Action::UpdatePlugins(vault_path) => {
                        let vaults = Vaults::new();
                        let vaults = vaults.get_vaults();
                        let to_be_synced = vaults
                            .iter()
                            .filter(|v| v.path != vault_path)
                            .cloned()
                            .collect_vec();
                        debug!("TEST SYNCER");

                        let is_free2 = Arc::clone(&is_free1);
                        Arc::clone(&is_free2).store(false, std::sync::atomic::Ordering::SeqCst);
                        let _thread: tokio::task::JoinHandle<
                            std::result::Result<(), color_eyre::eyre::Error>,
                        > = tokio::spawn(async move {
                            debug!("Starting Syncing Operation");
                            for vault in to_be_synced {
                                let entries = read_dir(&vault.path)?;
                                let entries = entries
                                    .filter_map(Result::ok)
                                    .map(|e| {
                                        let path = e.path();
                                        let name = path.file_name().unwrap_or_default();
                                        name.to_owned()
                                    })
                                    .collect_vec();

                                info!("Syncing vault {}", vault.path.display());

                                if entries.contains(&"no_sync".to_owned().into()) {
                                    continue;
                                }
                                sync_vault(vault_path.clone(), vault.path.clone()).await?;
                            }

                            debug!("Finished Syncing Operation");

                            Ok(())
                        });
                        _thread.await??;
                        Arc::clone(&is_free2).store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    _ => {
                        // Not this one's job
                    }
                }
            }
            Ok(())
        });
    let mut rx3 = tx.subscribe();
    let logger_thread = tokio::spawn(async move {
        loop {
            let Ok(action) = rx3.recv().await else {
                break;
            };
            info!("Action: {:?}", action);
        }
    });
    let tx2 = tx.clone();
    let event_watcher_thread = tokio::spawn(async move {
        for event in list_watcher_rx {
            info!("Event: {:?}", event);
            let vaults = Vaults::new();
            let open_vaults = vaults.get_open_vaults();
            tx2.send(Action::ChangeOpenVaults(open_vaults)).unwrap();
        }
    });

    tx.send(Action::ChangeOpenVaults(vaults.get_open_vaults()))
        .unwrap();

    logger_thread.await?;
    _thread_syncer.await??;
    _thread_vault_listeners.await?;
    event_watcher_thread.await?;
    _thread_vault_list.join().unwrap();
    Ok(())
}
