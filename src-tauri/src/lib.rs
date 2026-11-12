mod analyze;
mod apps;
mod clean;
mod fsutil;
mod optimize;
mod safety;
mod startup;

use fsutil::ActionResult;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

pub(crate) fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "No home folder".to_string())
}

/// Runs blocking work off the main thread.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn fda_status() -> Result<bool, String> {
    Ok(fsutil::has_full_disk_access(&home()?))
}

#[tauri::command]
fn open_fda_settings() -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    fsutil::reveal(&PathBuf::from(path))
}

#[tauri::command]
async fn trash_paths(paths: Vec<String>) -> Result<ActionResult, String> {
    let home = home()?;
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        Ok(fsutil::trash_paths(&paths, &home))
    })
    .await
}

#[derive(serde::Serialize, Clone)]
struct ProgressLabel {
    label: String,
}

#[tauri::command]
async fn clean_scan(app: tauri::AppHandle) -> Result<Vec<clean::CleanItem>, String> {
    let home = home()?;
    blocking(move || {
        let running = fsutil::running_bundle_ids();
        Ok(clean::scan(&home, &running, |label| {
            let _ = app.emit("clean://progress", ProgressLabel { label: label.to_string() });
        }))
    })
    .await
}

#[tauri::command]
async fn empty_trash() -> Result<ActionResult, String> {
    let home = home()?;
    blocking(move || Ok(clean::empty_trash(&home))).await
}

#[tauri::command]
async fn trash_size() -> Result<u64, String> {
    let home = home()?;
    blocking(move || Ok(clean::trash_size(&home))).await
}

#[tauri::command]
async fn apps_list(app: tauri::AppHandle) -> Result<Vec<apps::AppInfo>, String> {
    let home = home()?;
    let icons = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("icons");
    blocking(move || Ok(apps::installed_apps(&home, &icons))).await
}

#[tauri::command]
async fn app_leftovers(bundle_id: String, name: String) -> Result<Vec<apps::Leftover>, String> {
    let home = home()?;
    blocking(move || Ok(apps::app_leftovers(&home, &bundle_id, &name))).await
}

#[tauri::command]
async fn uninstall_app(
    bundle_id: String,
    name: String,
    app_path: Option<String>,
    leftover_paths: Vec<String>,
) -> Result<ActionResult, String> {
    let home = home()?;
    blocking(move || {
        let leftovers: Vec<PathBuf> = leftover_paths.into_iter().map(PathBuf::from).collect();
        let app = app_path.map(PathBuf::from);
        let running = fsutil::running_bundle_ids();
        Ok(apps::uninstall_app(&home, &bundle_id, &name, app.as_deref(), &leftovers, &running))
    })
    .await
}

#[tauri::command]
async fn startup_list() -> Result<startup::StartupList, String> {
    let home = home()?;
    blocking(move || Ok(startup::list(&home))).await
}

#[tauri::command]
async fn startup_set_enabled(id: String, enabled: bool) -> Result<(), String> {
    let home = home()?;
    blocking(move || startup::set_enabled(&home, &id, enabled)).await
}

#[tauri::command]
async fn startup_remove(id: String) -> Result<ActionResult, String> {
    let home = home()?;
    blocking(move || startup::remove(&home, &id)).await
}

#[tauri::command]
fn optimize_tasks() -> Vec<optimize::TaskInfo> {
    optimize::TASKS.to_vec()
}

#[tauri::command]
async fn optimize_run(app: tauri::AppHandle, ids: Vec<String>) -> Result<Vec<optimize::TaskResult>, String> {
    let home = home()?;
    blocking(move || {
        let mut results = Vec::new();
        for id in ids {
            let running = optimize::TaskResult { id: id.clone(), status: optimize::Status::Running, message: String::new() };
            let _ = app.emit("optimize://progress", &running);
            let r = optimize::run_task(&home, &id);
            let _ = app.emit("optimize://progress", &r);
            results.push(r);
        }
        Ok(results)
    })
    .await
}

#[tauri::command]
async fn analyze_scan(app: tauri::AppHandle, root: String) -> Result<(), String> {
    blocking(move || {
        let emitter = app.clone();
        let scan = analyze::scan(&PathBuf::from(root), |p| {
            let _ = emitter.emit("analyze://progress", p);
        })?;
        *app.state::<analyze::AnalyzeState>().0.lock().map_err(|e| e.to_string())? = Some(scan);
        Ok(())
    })
    .await
}

#[tauri::command]
fn analyze_list(state: tauri::State<'_, analyze::AnalyzeState>, path: String) -> Result<analyze::DirListing, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let scan = guard.as_ref().ok_or("Not scanned yet")?;
    analyze::list(scan, &PathBuf::from(path))
}

#[tauri::command]
async fn analyze_trash(app: tauri::AppHandle, path: String) -> Result<ActionResult, String> {
    let home = home()?;
    blocking(move || {
        let state = app.state::<analyze::AnalyzeState>();
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        let scan = guard.as_mut().ok_or("Not scanned yet")?;
        Ok(analyze::trash(scan, &PathBuf::from(path), &home))
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(analyze::AnalyzeState::default())
        .invoke_handler(tauri::generate_handler![
            fda_status,
            open_fda_settings,
            reveal_in_finder,
            trash_paths,
            clean_scan,
            empty_trash,
            trash_size,
            apps_list,
            app_leftovers,
            uninstall_app,
            startup_list,
            startup_set_enabled,
            startup_remove,
            optimize_tasks,
            optimize_run,
            analyze_scan,
            analyze_list,
            analyze_trash,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
