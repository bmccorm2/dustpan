mod fsutil;
mod safety;

use fsutil::ActionResult;
use std::path::PathBuf;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            fda_status,
            open_fda_settings,
            reveal_in_finder,
            trash_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
