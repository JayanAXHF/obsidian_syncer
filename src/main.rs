mod errors;
mod logging;
use std::sync::mpsc;

use color_eyre::eyre::Ok;
use color_eyre::eyre::Result;
use obsidian_syncer::structs::*;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    errors::init()?;
    let (tx, rx) = mpsc::channel();
    info!("Test logging");
    let vaults = Vaults::new();

    let (list_watcher_tx, list_watcher_rx) = mpsc::channel();
    let _thread_vault_list = std::thread::spawn(move || {
        obsidian_syncer::watch_vault_list(list_watcher_tx).unwrap();
    });
    let tx_move = tx.clone();
    let _thread_vault_listeners = tokio::spawn(async move {
        println!("Starting vault listeners");
        obsidian_syncer::setup_vault_listeners(tx_move.clone(), rx)
            .await
            .unwrap();
    });

    tx.send(Action::ChangeOpenVaults(vaults.get_open_vaults()))
        .unwrap();
    for event in list_watcher_rx {
        info!("Event: {:?}", event);
        let vaults = Vaults::new();
        let open_vaults = vaults.get_open_vaults();
        tx.send(Action::ChangeOpenVaults(open_vaults)).unwrap();
    }
    let _thread_syncer = tokio::spawn(async move {
        println!("Starting syncer");
        for event in rx {
            info!("Event: {:?}", event);
            match event {
                Action::UpdatePlugins(_vault_path) => {
                    todo!()
                }
                _ => {
                    // Not this one's job
                }
            }
        }
    });
    _thread_vault_list.join().unwrap();
    _thread_vault_listeners.await?;
    Ok(())
}

