# MoleSweep: a Tauri/Rust Mac cleaner modeled on Mole.app

## Goal
Build **MoleSweep**, a personal macOS cleaner app (Tauri 2 + Rust) that copies four tabs of Mole.app (tw93): **Clean**, **Apps** (Uninstall + Startup), **Optimize**, **Analyze**. Do not build Mole's Status tab, its Updates section, or any of its menu bar features.

## Context
- Machine: macOS 27, Apple Silicon. Tools: `rustc 1.99`, `cargo`, `bun 1.3`, Xcode CLT. The user's shell is fish.
- `~/src/mole` is empty except for this file and is **not** a git repo yet (task 1 creates it).
- Stack (same as `~/src/pi-desktop`, use it as the reference project for structure, `vite.config.ts`, `tsconfig.json`, `src-tauri/Cargo.toml` release profile and `scripts/deploy.sh`):
  - Tauri 2, Rust edition 2024, frontend = **vanilla TypeScript + Vite** (no framework), package manager **bun**, icons from the `lucide` npm package (`import { createElement, Trash2 } from "lucide"`).
  - Theme: copy `/Users/bryan/.pi/agent/skills/dark-theme/theme.css` to `src/theme.css` and import it first. Read `/Users/bryan/.pi/agent/skills/dark-theme/SKILL.md` and follow it (Open Sans via `@fontsource-variable/open-sans`, 13px density, tangerine accent). Set `<html data-theme="dark">`.
- Commands:
  - Dev: `bun run tauri dev`
  - Frontend typecheck/build: `bun run build`
  - Rust tests: `cd src-tauri && cargo test`
  - Lint: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
  - Install: `bun run deploy` (builds the release `.app` and copies it to `/Applications/MoleSweep.app`)
- Reference source (read-only, never vendored): the open-source Mole CLI, GPL-3.0. Clone it with `git clone --depth 1 https://github.com/tw93/Mole /tmp/mole-src`. Useful files:
  - `lib/clean/user.sh`, `lib/clean/dev.sh`, `lib/clean/app_caches.sh`: cache/log paths per app/tool (`safe_clean <glob> "<label>"` lines).
  - `lib/clean/apps.sh` (`is_bundle_orphaned`, `ORPHAN_NEVER_DELETE_PATTERNS`): leftover detection.
  - `lib/core/app_protection.sh`: protected paths.
  - `lib/optimize/tasks.sh`: optimize task commands.
  - `/Library/Mole/mole-admin` (installed by Mole.app) has extra safety guards (`mole_protected_cache_path`).
- Planned layout:
  ```
  index.html
  src/main.ts            tab bar + router
  src/api.ts             typed invoke() wrappers + shared TS types
  src/ui.ts              el(), formatBytes(), confirmDialog(), toast(), fdaBanner()
  src/views/clean.ts | apps.ts | optimize.ts | analyze.ts
  src/theme.css, src/styles.css
  src-tauri/src/lib.rs   command registration, app state
  src-tauri/src/safety.rs  path guard (every deletion goes through it)
  src-tauri/src/fsutil.rs  allocated size, Trash, FDA check, reveal, admin shell, running apps
  src-tauri/src/clean/{mod.rs,rules.rs,leftovers.rs,ai_versions.rs}
  src-tauri/src/apps.rs  installed apps + uninstall
  src-tauri/src/startup.rs launch agents/daemons + login items
  src-tauri/src/optimize.rs
  src-tauri/src/analyze.rs
  scripts/deploy.sh
  ```
- Conventions: Rust commands that do heavy work are `async` and run their work in `tauri::async_runtime::spawn_blocking`. All structs sent to the frontend use `#[derive(Serialize)] #[serde(rename_all = "camelCase")]`. Every scanner takes `home: &Path` as a parameter (never calls `dirs::home_dir()` itself) so tests can pass a temp dir. Commands pass `dirs::home_dir()`.

## Decisions
- Name: **MoleSweep**. `productName` = `MoleSweep`, identifier = `com.bryan.molesweep`, Cargo package `molesweep`, lib `molesweep_lib`. Window title "MoleSweep", 1040×700, minimum 860×560.
- Implemented natively in Rust. Do **not** shell out to `mo`. Port only paths and rules from the Mole CLI, not its code structure. The project license is **GPL-3.0** because it ports rules from a GPL-3.0 project: copy `/tmp/mole-src/LICENSE` to `LICENSE`.
- Tabs, in order: **Clean**, **Apps** (segmented control: Uninstall | Startup), **Optimize**, **Analyze**. No Status tab and no Updates section.
- **Every deletion moves items to the Trash** (the `trash` crate with the `NsFileManager` delete method). The only permanent deletion is the explicit **Empty Trash** action. Every destructive action first shows a review list with checkboxes, then a confirm dialog that shows the item count and total size.
- Clean categories in v1: App caches, Logs, Browsers, Developer tools, AI tools (caches + old CLI versions), Leftovers of uninstalled apps, Trash. **AI chat/session/worktree deletion is excluded.**
- Optimize v1 tasks: Flush DNS cache, Rebuild QuickLook cache, Clear old saved window states, Remove broken launch agents, Remove broken login items. Run admin tasks through `osascript … with administrator privileges`, which prompts for the password each run. "Run periodic maintenance" is **dropped** because `/usr/sbin/periodic` no longer exists on macOS 26+ (confirmed missing on this Mac).
- Analyze is a drill-down **list** with size bars, not a treemap. The default root is `$HOME`, and "Choose folder…" picks another root. Actions: Reveal in Finder, Move to Trash.
- Out of scope: menu bar icon, Awake/keep screen on, Clean Screen, fan control, battery care, licensing/trial, localization (English only), auto-update, code signing/notarization, a whitelist/"never clean" feature, settings screen, Homebrew uninstall integration.
- Tests: Rust unit tests for the safety guard and every scanner, using `tempfile` temp dirs as fake `$HOME`. No UI/e2e tests.
- Full Disk Access is not required. If it is missing, show a banner with a button that opens System Settings. Items that fail with a permission error show up as failures; they must not crash the app.

## Tasks

**Progress tracking (required):** As soon as you finish a task, tick its box (`- [x]`) in this file, before you start the next task. Do not save the ticks for the end. The user watches this file to follow your progress.

**Git workflow (required):** Before you start a task, create a new feature branch for it. When the task is done, commit it to that branch. Whether to ask first depends on what the user asked for:
- If the user asked for one task (e.g. "do m3"), ask for approval before committing, and commit only after they approve.
- If the user asked you to implement the whole `todo.md`, commit each task as you finish it without asking.

Sub-steps: tick each sub-step box (e.g. `3.2`) as soon as it is done, and tick the task box once all of its sub-steps are ticked. The user may refer to a sub-step by its number (e.g. `t3.2`).

- [x] 1. **Scaffold + repo + shell UI.**
  - [x] 1.1 Run `git init -b main` in `~/src/mole`. Make the first commit on `main` with only `todo.md` and a `.gitignore` (`node_modules/`, `dist/`, `src-tauri/target/`, `src-tauri/gen/`). Feature branches start from there.
  - [x] 1.2 Scaffold into a temp dir, **never into `~/src/mole` and never with `--force`**: `cd /tmp && bun create tauri-app molesweep --template vanilla-ts --manager bun --yes` (if the flags differ, run `bun create tauri-app --help`). Copy the result into `~/src/mole` without overwriting `todo.md`.
  - [x] 1.3 Apply the names/identifier/window size from Decisions in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`. Copy the `[profile.release]` block from `~/src/pi-desktop/src-tauri/Cargo.toml`.
  - [x] 1.4 Add deps: `bun add lucide @fontsource-variable/open-sans @tauri-apps/plugin-dialog`. Add these Cargo deps: `serde` (derive), `serde_json`, `tauri-plugin-dialog = "2"`, `trash = "5"`, `plist = "1"`, `glob = "0.3"`, `jwalk = "0.8"`, `rayon = "1"`, `dirs = "6"`, `libc = "0.2"`, `base64 = "0.22"`. Dev-dep: `tempfile = "3"`. Register the dialog plugin and add `dialog:default` to `src-tauri/capabilities/default.json`.
  - [x] 1.5 Copy the theme as described in Context. Build `src/main.ts` with a top tab bar (Lucide icons: `Sparkles` Clean, `Package` Apps, `Gauge` Optimize, `HardDrive` Analyze) that swaps between 4 empty view modules, each exporting `mount(root: HTMLElement): void`. Remember the active tab in `localStorage` under `molesweep.tab`.
  - [x] 1.6 Create `scripts/deploy.sh` by copying pi-desktop's and adapting it: `APP=MoleSweep.app`, quit via `osascript -e 'quit app "MoleSweep"'`, wait, then `pkill -f /Applications/MoleSweep.app/` as a fallback. Add `"deploy": "scripts/deploy.sh"` to `package.json`.
  - [x] 1.7 Copy `LICENSE` (GPL-3.0) from `/tmp/mole-src/LICENSE`.
  - [x] 1.8 Verify: `bun run tauri dev` opens a window with 4 switchable tabs.

- [x] 2. **Rust core: safety guard + fs utilities** (`safety.rs`, `fsutil.rs`).
  - [x] 2.1 `safety::check(path: &Path, home: &Path) -> Result<(), String>` returns `Err(reason)` when any of these is true:
    - The path is not absolute, contains a `..` component, or contains a newline.
    - The path does not exist (`symlink_metadata` fails).
    - It is not strictly under `home`, `/Applications/` or `/Library/` (only `/Library/LaunchAgents`, `/Library/LaunchDaemons` and `/Library/Caches` are allowed under `/Library`).
    - It is equal to (not inside) one of these roots: `home`, `home/Library`, `home/Library/Caches`, `home/Library/Logs`, `home/Library/Application Support`, `home/Library/Containers`, `home/Library/Group Containers`, `home/Library/Preferences`, `home/Documents`, `home/Desktop`, `home/Downloads`, `home/Pictures`, `home/Movies`, `home/Music`, `home/.Trash`, `/Applications`.
    - It is inside a protected subtree: `home/Library/Keychains`, `home/Library/Mobile Documents`, `home/Library/CloudStorage`, `home/.ssh`, `home/.gnupg`, any path containing `/TCC/`, `com.apple.TCC`, `backgroundtaskmanagement`, `com.apple.sharedfilelist`, `/System/`.
    - Its lowercased path contains one of: `1password`, `bitwarden`, `lastpass`, `keepass`, `dashlane`, `enpass`, `keychain`.
    - It is an app bundle under `/Applications` or `/System/Applications` whose bundle id starts with `com.apple.`.
  - [x] 2.2 `fsutil::allocated_size(path) -> u64`: recursive sum of `metadata.blocks() * 512`. It does not follow symlinks, counts each `(dev, ino)` only once when `nlink > 1`, and silently skips entries it cannot read. Use `jwalk` for directories.
  - [x] 2.3 `fsutil::trash_paths(paths: &[PathBuf], home: &Path) -> ActionResult`. For each path: run `safety::check`, compute `allocated_size`, then trash it with `trash::TrashContext` set to `trash::macos::DeleteMethod::NsFileManager`. Trash paths **one at a time** and collect each failure.
    `ActionResult { done: Vec<String>, failed: Vec<Failure { path: String, reason: String }>, bytes: u64 }`.
  - [x] 2.4 `fsutil::has_full_disk_access(home) -> bool`: true if `fs::read_dir(home/Library/Safari)` succeeds. If it fails with `PermissionDenied`, return false. If it fails with `NotFound`, retry the same check with `home/Library/Containers/com.apple.stocks`; if that is also missing, return true.
  - [x] 2.5 `fsutil::running_bundle_ids() -> HashSet<String>`: parse every `bundleID="…"` in the output of `/usr/bin/lsappinfo list`.
  - [x] 2.6 `fsutil::admin_shell(script: &str) -> Result<String, String>`: run `osascript -e 'do shell script "<escaped>" with administrator privileges'`. Escape `\` and `"`. If the user cancels (osascript error `-128`), return `Err("Cancelled")`.
  - [x] 2.7 `fsutil::reveal(path)` runs `open -R <path>`.
  - [x] 2.8 Commands: `fda_status() -> bool`, `open_fda_settings()` (runs `open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"`), `reveal_in_finder(path)`, `trash_paths(paths: Vec<String>) -> ActionResult`.
  - [x] 2.9 Frontend `src/ui.ts`: `fdaBanner()`, which is shown at the top of every tab while `fda_status()` is false and has an "Open System Settings" button; `confirmDialog({title, message, confirmLabel}) -> Promise<boolean>`, a custom modal (do not use `window.confirm`); `formatBytes` (base 1000, matching Finder: `1.2 GB`); `toast()`.
  - [x] 2.10 Tests: one test for each rejection rule, a test that allows `home/Library/Caches/com.foo/x`, and a hard-link dedupe test for `allocated_size`.

- [x] 3. **Clean backend: rule engine** (`clean/mod.rs`, `clean/rules.rs`).
  - [x] 3.1 Types:
    ```rust
    pub struct Rule { pub id: &'static str, pub category: Cat, pub label: &'static str,
                      pub globs: &'static [&'static str],   // relative to home
                      pub recommended: bool, pub app_bundle: Option<&'static str> }
    pub enum Cat { AppCaches, Logs, Browsers, Developer, Ai, Leftovers, Trash }  // serialize as camelCase
    pub struct CleanItem { id: String, category: Cat, label: String, paths: Vec<String>,
                           size: u64, recommended: bool, blocked_reason: Option<String> }
    ```
  - [x] 3.2 `scan(home, running: &HashSet<String>, progress: impl Fn(&str)) -> Vec<CleanItem>`:
    - Expand each rule's globs under `home` with the `glob` crate. Drop matches that fail `safety::check` or have 0 bytes. Skip rules with no remaining matches.
    - If `app_bundle` is in `running`, set `blocked_reason = "Quit <AppName> first"` (AppName is the rule label's first word).
    - **Dedupe**: process rules in table order and keep a set of claimed paths. Drop a match if it equals a claimed path or is inside one, so specific rules win over the generic sweep.
    - Sizes are computed in parallel with rayon. Call `progress(label)` once per rule.
  - [x] 3.3 Rule table in `rules.rs`, specific rules first:
    - **Browsers**:
      - Safari: `Library/Caches/com.apple.Safari/*`, `app_bundle` `com.apple.Safari`.
      - Chrome: `Library/Application Support/Google/Chrome/*/Cache/*`, `…/*/Code Cache/*`, `…/*/GPUCache/*`, `…/*/Service Worker/CacheStorage/*`, plus `Library/Caches/Google/Chrome/*`. `app_bundle` `com.google.Chrome`.
      - Edge: `Library/Caches/com.microsoft.edgemac/*`, `com.microsoft.edgemac`.
      - Brave: `Library/Caches/BraveSoftware/*`, `com.brave.Browser`.
      - Arc: `Library/Caches/company.thebrowser.Browser/*` and the `Library/Application Support/Arc/User Data/*/{Code Cache,GPUCache,DawnCache,GrShaderCache}/*` globs (one glob each). `app_bundle` `company.thebrowser.Browser`.
      - Firefox: `Library/Caches/Firefox/Profiles/*/cache2/*`, `org.mozilla.firefox`.
    - **Developer**:
      - npm `.npm/_cacache/*`
      - Yarn `Library/Caches/Yarn/*`
      - Bun `.bun/install/cache/*`
      - uv `.cache/uv/*`
      - Cargo registry `.cargo/registry/cache/*`
      - Gradle `.gradle/caches/*`
      - Homebrew downloads `Library/Caches/Homebrew/downloads/*`
      - Xcode DerivedData `Library/Developer/Xcode/DerivedData/*`
      - CoreSimulator caches `Library/Developer/CoreSimulator/Caches/*`
      - CocoaPods `Library/Caches/CocoaPods/*`
      - Go build `Library/Caches/go-build/*`
      - pip `Library/Caches/pip/*`
      - node-gyp `.cache/node-gyp/*`
      - TypeScript `.cache/typescript/*`
      - Electron `.cache/electron/*`
      - Prisma `.cache/prisma/*`
      - pre-commit `.cache/pre-commit/*`
      - Ruff `.cache/ruff/*`
      - Turbo `.turbo/cache/*`
      - pyenv `.pyenv/cache/*`
      - rbenv `.rbenv/cache/*`
      - gem specs `.gem/specs/*`
      - Docker buildx `.docker/buildx/cache/*`
      - kube `.kube/cache/*`
      - AWS CLI `.aws/cli/cache/*`
      - JetBrains logs `Library/Logs/JetBrains/*`
      - All of these are recommended=true.
      - Optionally add more entries from `lib/clean/dev.sh` `safe_clean` lines that are plain user-level cache globs.
      - **Never** add the pnpm store (`Library/pnpm/store`), `~/.cargo/registry/src`, or Xcode Archives.
    - **AI**: Claude desktop `Library/Caches/com.anthropic.claudefordesktop/*` (`app_bundle` `com.anthropic.claudefordesktop`), Claude logs `Library/Logs/Claude/*`, LM Studio `Library/Caches/com.lmstudio.lmstudio/*`, Codex app cache `Library/Caches/Codex/Default/Cache/*` and `Library/Caches/Codex/Default/Code Cache/*`, OpenCode `.cache/opencode/*`, Copilot `Library/Caches/copilot/*`. All recommended=true. Old CLI versions come in task 5.
    - **Logs**: User logs `Library/Logs/*` (the dedupe step leaves out paths already claimed by earlier rules), Diagnostic reports `Library/Logs/DiagnosticReports/*` (place it before User logs). recommended=true.
    - **AppCaches (generic sweep, last)**: one `CleanItem` **per child** of `home/Library/Caches`.
      - Skip children whose name starts with `com.apple.` and skip `CloudKit`, `FamilyCircle`, `GeoServices`.
      - id = `appcache:<name>`. The label is the installed app's name when `<name>` matches an installed bundle id (use the bundle-id→name map from task 6's `apps::installed_bundle_map(home)`; until task 6 exists, create a stub that returns an empty map). Otherwise the label is `<name>`.
      - If `<name>` is in `running`, set blocked_reason `"In use"`.
      - recommended=true.
  - [x] 3.4 Commands: `clean_scan() -> Vec<CleanItem>`, which emits event `clean://progress` with payload `{ label: String }`. Deletion reuses `trash_paths`.
  - [x] 3.5 Tests (fake home in tempdir): globs match, dedupe (a Chrome cache path isn't also counted by the generic sweep), a running app gets blocked, `com.apple.*` is skipped in the sweep, zero-byte matches are dropped.

- [x] 4. **Clean UI** (`src/views/clean.ts`).
  - [x] 4.1 Idle state: big "Scan" button.
  - [x] 4.2 Scanning state: spinner in `--running` color plus the current progress label.
  - [x] 4.3 Results state:
    - Header shows the total selected size and a primary button "Move to Trash · <size>".
    - Preset links: Recommended (default) / All / None.
    - Collapsible category sections in this order: App caches, Browsers, Developer tools, AI tools, Logs, Leftovers, Trash. Each shows its category total.
    - Each item row: checkbox, label, size. Click the chevron to expand its paths, max 200 shown, each with a Reveal button.
    - Blocked items are greyed out with the reason, and their checkbox is disabled.
  - [x] 4.4 On confirm, call `trash_paths` with the union of the selected items' paths. Then show a result view: "Moved <bytes> to Trash", a failure count that expands into a list of path + reason, a "Scan again" button, and an "Empty Trash (<size>)" button. That button is wired up in task 5.

- [x] 5. **Clean: leftovers, AI old versions, Trash** (`clean/leftovers.rs`, `clean/ai_versions.rs`, Trash in `clean/mod.rs`).
  - [x] 5.1 **Leftovers** (`Cat::Leftovers`, recommended=**false**):
    - Scan these dirs one level deep: `home/Library/{Application Support, Caches, Containers, Preferences, Saved Application State, HTTPStorages, WebKit, Logs, Cookies}` and `home/Library/LaunchAgents`.
    - Take the candidate bundle id from the entry name: strip `.plist`, `.savedState`, `.binarycookies`. The name must look like reverse-DNS (`^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+){2,}$`).
    - An entry is an orphan only if **all** of these hold:
      - The id doesn't start with `com.apple.`.
      - It doesn't match the safety sensitive patterns.
      - It isn't in the installed bundle-id set. Build the set from `Info.plist` `CFBundleIdentifier` of `.app` bundles found up to 3 levels deep under `/Applications`, `home/Applications`, `/System/Applications`, `/System/Library/CoreServices`, `/Library/Application Support`, `home/Library/Application Support`.
      - `mdfind "kMDItemCFBundleIdentifier == '<id>'"` returns nothing. Cache the result per id. On timeout (5 s) or error, treat the app as installed.
      - The newest mtime anywhere inside the entry is older than 30 days. A walk error counts as "not stale".
      - It isn't `com.bryan.molesweep`.
    - Group results into one `CleanItem` per bundle id, with id `leftover:<bundle id>` and label `<bundle id>`.
    - Port the extra checks from `is_bundle_orphaned` in `/tmp/mole-src/lib/clean/apps.sh` only where they are simple.
  - [x] 5.2 **AI old versions** (`Cat::Ai`, recommended=true). For each `(versions_root, active_symlink)` in `[(".local/share/claude/versions", ".local/bin/claude"), (".local/share/cursor-agent/versions", ".local/bin/cursor-agent"), (".copilot/pkg/universal", ".local/bin/copilot")]`:
    - Resolve the symlink with `fs::canonicalize`. If it is missing or broken, skip the tool.
    - The active entry is the child of `versions_root` that contains the resolved target.
    - Keep the active entry and the newest other child by mtime. Every other child becomes a path of the item `aiver:<tool>` with label `<Tool> old versions`.
  - [x] 5.3 **Trash** (`Cat::Trash`, recommended=**false**): item `trash` with the children of `home/.Trash`. It is labeled "Trash (permanently deleted)". **Do not** send it to `trash_paths`: the frontend calls `empty_trash()`, which runs `fs::remove_dir_all`/`remove_file` on each child of `home/.Trash` and returns an `ActionResult`.
  - [x] 5.4 Commands: `empty_trash() -> ActionResult` and `trash_size() -> u64`. Wire the "Empty Trash" button from task 4 to these, behind a confirm dialog that says "This permanently deletes everything in the Trash."
  - [x] 5.5 Tests: an orphan is detected (fake installed set; backdate mtimes with `libc::utimes`, no extra crate); a recently modified entry is kept; `com.apple.x` is kept; an installed id is kept; AI versions keep the active entry plus 1 newest.

- [x] 6. **Apps → Uninstall** (`apps.rs`, `src/views/apps.ts`).
  - [x] 6.1 `installed_apps(home) -> Vec<AppInfo { name, bundle_id, version, path, size, icon: Option<String> }>`:
    - Find `*.app` directly in `/Applications`, in `/Applications/*/` (one level of subfolders), and in `home/Applications`.
    - Read `Contents/Info.plist` with the `plist` crate: `CFBundleIdentifier`, `CFBundleName`/`CFBundleDisplayName` (falling back to the file stem), `CFBundleShortVersionString`, `CFBundleIconFile`.
    - Exclude `com.apple.*` and `com.bryan.molesweep`.
    - Sizes come from `allocated_size`, computed in parallel.
    - Icon: run `sips -s format png --resampleHeightWidth 64 64 <icns> --out <app_cache_dir>/icons/<bundle_id>.png` if that PNG doesn't already exist, then return a `data:image/png;base64,…` URL. `None` on failure.
    - Sort by name.
  - [x] 6.2 `installed_bundle_map(home) -> HashMap<String, String>` (bundle id → name). Replace the task 3 stub with it.
  - [x] 6.3 `app_leftovers(home, bundle_id, name) -> Vec<Leftover { path, size, kind }>`. Look for entries matching the bundle id (exact, or prefixed by `<bundle_id>.`) or the exact app name in:
    - `Library/Application Support/<x>`, `Library/Caches/<x>`, `Library/Containers/<id>`, `Library/Group Containers/*<id>*`
    - `Library/Preferences/<id>*.plist`, `Library/Saved Application State/<id>.savedState`
    - `Library/HTTPStorages/<id>*`, `Library/WebKit/<id>`, `Library/Logs/<x>`, `Library/Cookies/<id>.binarycookies`, `Library/LaunchAgents/<id>*.plist`
    - `Library/Application Scripts/<id>`
    - `kind` = the parent folder name ("Caches", "Preferences", …). Skip anything that fails `safety::check`. Name matching is case-insensitive. The name match is skipped if the name is shorter than 4 characters.
  - [x] 6.4 `uninstall_app(app_path, leftover_paths) -> ActionResult`:
    - If the app's bundle id is in `running_bundle_ids()`, return an error result "Quit <name> first" and trash nothing.
    - For leftover LaunchAgent plists, first run `launchctl bootout gui/<uid> <plist>` and ignore errors.
    - Then call `trash_paths([app_path] + leftover_paths)`.
    - If trashing the `.app` fails with a permission error, report "Needs admin: remove it in Finder". No admin escalation.
  - [x] 6.5 Commands: `apps_list`, `app_leftovers`, `uninstall_app`.
  - [x] 6.6 UI:
    - Segmented control "Uninstall | Startup" at the top of the Apps tab (Startup is filled in by task 7).
    - Uninstall shows a search box, a sort toggle (Name / Size), and an app list (icon, name, version, size).
    - Selecting an app shows a detail pane with leftover rows that have checkboxes, all checked by default, plus the app row (checked, with an option to uncheck so only the data is removed).
    - Button "Uninstall · <total>" → confirm → result toast → refresh the list.

- [x] 7. **Apps → Startup** (`startup.rs`, Startup segment in `apps.ts`).
  - [x] 7.1 `StartupItem { id, kind: "userAgent"|"globalAgent"|"globalDaemon"|"loginItem", label, path, program: Option<String>, enabled: bool, broken: bool, editable: bool }`.
  - [x] 7.2 Launch items:
    - Read every `*.plist` in `home/Library/LaunchAgents` (userAgent, editable), `/Library/LaunchAgents` (globalAgent, read-only), and `/Library/LaunchDaemons` (globalDaemon, read-only).
    - `label` = `Label` key or the file stem. `program` = `Program` or `ProgramArguments[0]`.
    - `broken` = program is an absolute path that doesn't exist.
    - `enabled` = false if `launchctl print-disabled gui/<uid>` (for userAgent) lists the label as disabled. Accept both `"label" => disabled` and `"label" => true`. Otherwise true.
  - [x] 7.3 Login items: run `osascript -e 'tell application "System Events" to get {name, path} of every login item'` and parse the two comma-separated lists. kind loginItem, editable, enabled=true, broken = path doesn't exist. If osascript fails (Automation denied), return an empty list plus a `warning` string. The command returns `{ items, warning: Option<String> }`.
  - [x] 7.4 `uid` = `unsafe { libc::getuid() }`.
  - [x] 7.5 Actions (userAgent only, unless noted):
    - Disable: `launchctl disable gui/<uid>/<label>`, then `launchctl bootout gui/<uid>/<label>` (ignore bootout errors).
    - Enable: `launchctl enable gui/<uid>/<label>`, then `launchctl bootstrap gui/<uid> <path>` (ignore an "already loaded" error).
    - Remove: disable, then `trash_paths([path])`.
    - loginItem Remove: `osascript -e 'tell application "System Events" to delete login item "<name>"'`.
  - [x] 7.6 Commands: `startup_list`, `startup_set_enabled(id, enabled)`, `startup_remove(id)`. `id` = `<kind>:<path or name>`.
  - [x] 7.7 UI: a filter (All / Enabled / Disabled / Broken), grouped list (Login items, My background services, System agents, System daemons), and a "Broken" badge in `--error`. Editable rows get an enable toggle and a Remove button (with confirm). Read-only rows get only Reveal. Show the warning banner when it is present.
  - [x] 7.8 Tests: the `print-disabled` parser (both formats); plist parsing and broken detection with temp plists.

- [x] 8. **Optimize** (`optimize.rs`, `src/views/optimize.ts`).
  - [x] 8.1 `TaskInfo { id, title, description, needs_admin }`, `TaskResult { id, status: "done"|"skipped"|"failed", message }`. Tasks:
    1. `flush_dns` "Flush DNS cache", needs_admin: `admin_shell("dscacheutil -flushcache; killall -HUP mDNSResponder")`.
    2. `rebuild_quicklook` "Rebuild QuickLook cache": `qlmanage -r cache`, then `qlmanage -r`.
    3. `saved_state` "Clear old saved window states": trash children of `home/Library/Saved Application State` with mtime older than 30 days. Message: "Removed N". If there are none: skipped, "Nothing older than 30 days".
    4. `broken_agents` "Remove broken background services": userAgent items from `startup.rs` with `broken=true` → `startup_remove`. Message lists the labels.
    5. `broken_login_items` "Remove broken login items": login items with `broken=true` → remove.
  - [x] 8.2 Command `optimize_run(ids: Vec<String>) -> Vec<TaskResult>` runs the tasks sequentially, emitting `optimize://progress { id, status: "running"|"done"|"skipped"|"failed", message }`. Also add `optimize_tasks() -> Vec<TaskInfo>`.
  - [x] 8.3 UI: list of tasks with checkboxes (all checked by default), descriptions, a lock icon (`Lock`) on admin tasks, and a "Run selected" button. Each row shows live status (spinner in `--running`, ✓ in `--ok`, skipped in `--text-dim`, failed in `--error`) plus its message.
  - [x] 8.4 Tests: saved-state age filtering with a temp home.

- [x] 9. **Analyze** (`analyze.rs`, `src/views/analyze.ts`).
  - [x] 9.1 `AnalyzeState` (Tauri managed state, `Mutex`) stores `root: PathBuf` and `dir_sizes: HashMap<PathBuf, u64>`.
  - [x] 9.2 `analyze_scan(root: String)`:
    - Do one `jwalk` walk of `root` that does not follow symlinks and stays on `root`'s `dev` (skip entries on other devices).
    - Add each file's `blocks*512` to every ancestor directory up to `root`. Dedupe hard links by `(dev, ino)`.
    - Count unreadable dirs.
    - Emit `analyze://progress { files, bytes }` at most every 250 ms.
    - Return when done.
  - [x] 9.3 `analyze_list(path) -> DirListing { path, total, unreadable: u32, entries: Vec<Entry { name, path, size, is_dir, kind }> }`. List direct children; dir sizes come from `dir_sizes`, file sizes from metadata. Sort by size descending.
  - [x] 9.4 `kind` comes from the extension:
    - image: png jpg jpeg heic gif webp tiff raw
    - video: mp4 mov mkv avi m4v
    - audio: mp3 m4a wav flac aac
    - archive: zip tar gz tgz xz 7z rar
    - diskImage: dmg iso img sparseimage sparsebundle
    - app: `.app` directory
    - folder: other directories
    - other: everything else
  - [x] 9.5 Command `analyze_trash(path)` calls `trash_paths([path])`. On success, subtract the size from all cached ancestors.
  - [x] 9.6 UI:
    - Header: breadcrumb (clickable segments from root), a "Choose folder…" button (`@tauri-apps/plugin-dialog` `open({ directory: true })`), and a "Rescan" button.
    - Body: rows with a kind icon (Lucide `Folder`, `Image`, `Film`, `Music`, `Archive`, `Disc`, `AppWindow`, `File`), the name, a size bar (width = size / largest entry, filled with `--accent`), and the size. Double-click a folder to drill in; Backspace goes up.
    - Right-click a row to open a small context menu: Reveal in Finder / Move to Trash (with confirm).
    - Footer: "<n> items · <total> · <unreadable> folders unreadable", plus an FDA hint when unreadable > 0 and FDA is missing.
    - Default root is `$HOME`, scanned automatically on the first visit to the tab.
  - [x] 9.7 Tests: sizes of a temp tree add up; a hard link is counted once; a symlink is not followed; the kind classifier.

- [x] 10. **Polish + docs + install.**
  - [x] 10.1 The FDA banner is shown on all tabs.
  - [x] 10.2 Every async action disables its button while running.
  - [x] 10.3 Errors from `invoke` show as `toast` in `--error`.
  - [x] 10.4 Empty states for every list.
  - [x] 10.5 Write `README.md`: what the app is, the build/run/deploy commands (fish-compatible), why to grant Full Disk Access, that deletions go to the Trash, and that it is GPL-3.0 because rules are ported from tw93/Mole.
  - [x] 10.6 Run every check in Acceptance criteria, then `bun run deploy`.

## Acceptance criteria
- [x] `cd src-tauri && cargo test` passes. `cargo clippy --all-targets -- -D warnings` is clean. `bun run build` passes (tsc with no errors).
- [x] `bun run deploy` produces and launches `/Applications/MoleSweep.app`, and Mole.app is not touched.
- [x] Clean: Scan finishes in under 60 s on this Mac and shows categories with sizes. Moving a selection to the Trash puts the files in `~/.Trash`. Empty Trash requires confirmation. A running Safari blocks the Safari item.
- [x] Apps → Uninstall: create a throwaway app with `mkdir -p ~/Applications/Dummy.app/Contents` and an `Info.plist` with `CFBundleIdentifier` `com.example.dummy`, plus a fake leftover `~/Library/Caches/com.example.dummy`. Uninstall it and confirm both end up in the Trash.
- [x] Apps → Startup lists `~/Library/LaunchAgents/*.plist` (e.g. `com.bryan.keyboard-customizer.capsbackspace`). Toggling disable/enable changes `launchctl print-disabled gui/(id -u)` output.
- [ ] Optimize: running "Rebuild QuickLook cache" reports done. "Flush DNS cache" shows the macOS admin password prompt, and cancelling reports failed with "Cancelled".
- [x] Analyze: `$HOME` scan completes. Drill-down and back work instantly (no rescan). Reveal opens Finder.
- [x] No code path deletes anything without going through `safety::check`, except `empty_trash`, which only touches children of `home/.Trash`.

## Out of scope
Status tab, Updates (Homebrew/Sparkle/App Store), menu bar, Awake, Clean Screen, fans, battery, licensing, localization, auto-update, signing/notarization, whitelist/"never clean", settings screen, AI chat/session/worktree cleanup, `mo purge`/`installer`/`slim` features, treemap visualization, permanent deletion (other than Empty Trash), admin-level cleaning of `/Library` or `/private`. No dependencies beyond those listed in task 1 (plus `tempfile` for tests).

## Notes / risks
- **Data loss**: the safety guard is the most important code. Write task 2's tests before using `trash_paths` anywhere. Never widen the allowed roots to make a test pass.
- Moving caches to the Trash frees no space until the Trash is emptied. That is why the Clean result screen offers "Empty Trash".
- The `trash` crate's default macOS method drives Finder via AppleScript, which is slow, plays a sound and needs Automation permission. Always set `DeleteMethod::NsFileManager`.
- Without Full Disk Access, reading `~/.Trash`, `~/Library/Containers/*`, Safari and Mail data fails with `EPERM`. Treat these as skipped or failed items, never as panics. A dev build (`tauri dev`) gets FDA through the terminal app. The installed `.app` needs its own grant.
- Login item queries and removal through System Events trigger a one-time Automation prompt. Denial must degrade to a warning.
- Sizes use allocated blocks, so iCloud placeholder files count as 0, which is correct.
- `mdfind` can stall. Always use a timeout, and on failure treat the app as installed (keep its data).
- Do not use `--force` with any scaffolder, and do not scaffold into `~/src/mole` directly. `todo.md` must survive.
