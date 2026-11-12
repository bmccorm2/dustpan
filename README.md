# MoleSweep

A personal macOS cleaner (Tauri 2 + Rust) modeled on [Mole.app](https://github.com/tw93/Mole). Four tabs:

- **Clean**: app caches, browser caches, developer and AI tool caches, old AI CLI versions, logs, leftovers of removed apps, and the Trash.
- **Apps**: uninstall apps together with their data in `~/Library`, and manage startup items (login items, launch agents and daemons).
- **Optimize**: flush DNS, rebuild the QuickLook cache, clear old saved window states, remove broken background services and login items.
- **Analyze**: drill down through a folder (default: your home folder) sorted by size, then reveal or trash what you find.

## Safety

- Every deletion **moves items to the Trash**. The only permanent deletion is the explicit **Empty Trash** action.
- Every path passes a safety guard first (`src-tauri/src/safety.rs`): only your home folder, `/Applications` and `/Library/{Caches,LaunchAgents,LaunchDaemons}` are allowed; key folders, keychains, iCloud, SSH/GPG keys, TCC data and password-manager data are always refused.
- Destructive actions always show a review list and a confirm dialog with the item count and size.

## Full Disk Access

MoleSweep works without it, but macOS hides some data (Safari, Mail, app containers, the Trash) until you grant it.
A banner offers to open **System Settings › Privacy & Security › Full Disk Access**. Add `/Applications/MoleSweep.app` there.
During development the terminal running `bun run tauri dev` needs the grant instead.

## Develop

Requires Rust, bun and the Xcode command line tools.

```fish
bun install
bun run tauri dev            # run the app
bun run build                # typecheck + build the frontend
cd src-tauri; and cargo test # Rust tests
cd src-tauri; and cargo clippy --all-targets -- -D warnings
```

## Install

```fish
bun run deploy   # builds the release app, installs /Applications/MoleSweep.app and opens it
```

## License

GPL-3.0. MoleSweep ports cleaning paths and rules from [tw93/Mole](https://github.com/tw93/Mole), which is GPL-3.0. See `LICENSE`.
