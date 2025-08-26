mod errors;
mod logging;
use color_eyre::eyre::Result;
use itertools::Itertools;
use obsidian_syncer::structs::*;
use obsidian_syncer::sync_vault;
use tokio::sync::broadcast;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    errors::init()?;
    let (tx, mut rx1) = broadcast::channel(100);
    let rx2 = tx.subscribe();
    info!("Test logging");
    let vaults = Vaults::new();

    let (list_watcher_tx, list_watcher_rx) = std::sync::mpsc::channel();
    let _thread_vault_list = std::thread::spawn(move || {
        obsidian_syncer::watch_vault_list(list_watcher_tx).unwrap();
    });
    let tx_move = tx.clone();
    let _thread_vault_listeners = tokio::spawn(async move {
        println!("Starting vault listeners");

        obsidian_syncer::setup_vault_listeners(tx_move.clone(), &mut rx1)
            .await
            .unwrap();
    });
    let _thread_syncer: tokio::task::JoinHandle<std::result::Result<(), color_eyre::eyre::Error>> =
        tokio::spawn(async move {
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
                        let _thread: tokio::task::JoinHandle<
                            std::result::Result<(), color_eyre::eyre::Error>,
                        > = tokio::spawn(async move {
                            for vault in to_be_synced {
                                info!("Syncing vault {}", vault.path.display());
                                sync_vault(vault_path.clone(), vault.path.clone()).await?
                            }
                            Ok(())
                        });
                        _thread.await??;
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

    tx.send(Action::ChangeOpenVaults(vaults.get_open_vaults()))
        .unwrap();

    logger_thread.await?;
    _thread_syncer.await??;
    _thread_vault_listeners.await?;
    _thread_vault_list.join().unwrap();
    for event in list_watcher_rx {
        info!("Event: {:?}", event);
        let vaults = Vaults::new();
        let open_vaults = vaults.get_open_vaults();
        tx.send(Action::ChangeOpenVaults(open_vaults)).unwrap();
    }
    Ok(())
}
