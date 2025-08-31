# Obsidian Syncer

Obsidian Syncer is a command-line tool for synchronizing Obsidian vaults, with a focus on syncing plugins and their configurations. It actively monitors your vaults for any changes and automatically propagates them to your other vaults, ensuring a consistent user experience across all your devices.

## How it Works

The tool operates by monitoring the `.obsidian/plugins` directory and the `community-plugins.json` file within each of your specified vaults. When a change is detected in one vault, the tool intelligently syncs these changes to the other vaults. To optimize the synchronization process, it employs a delta-based algorithm, which means only the differences between files are transferred, not the entire files themselves. This approach significantly reduces data transfer and speeds up the syncing process.

Additionally, the tool is designed to be mindful of your system's resources. It includes a mechanism to prevent syncing conflicts by ensuring that a sync operation is not initiated while another is already in progress. You can also exclude specific vaults from being synced by creating a file named `no_sync` in the root of the vault's directory.

## Usage

Just run the program, it handles the rest. It finds the vaults from the obsidian.json file in your central obsidian folder, which differs from OS to OS.

## Building from Source

To build Obsidian Syncer from source, you will need to have the Rust programming language and its package manager, Cargo, installed on your system.

1.  Clone the repository:

```sh
git clone https://github.com/your-username/obsidian_syncer.git
cd obsidian_syncer
```

2.  Build the project:

```sh
cargo build --release
```

3.  Run the executable:

```sh
./target/release/obsidian_syncer
```

## Contributing

Contributions are welcome! If you have any ideas, suggestions, or bug reports, please open an issue or submit a pull request.

## License

This project is licensed under the MIT License.
