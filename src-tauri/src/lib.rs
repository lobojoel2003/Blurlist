//! Blurlist desktop shell: wires the engine in `blurlist-core` to the React UI.

mod commands;
mod media;
mod state;

use std::path::{Path, PathBuf};

use blurlist_core::Store;
use tauri::Manager;

use crate::state::AppState;

fn model_dirs(app: &tauri::App, user_models_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("models"));
    }
    dirs.push(user_models_dir.to_path_buf());
    // Development builds read the models fetched into the repository.
    if cfg!(debug_assertions) {
        dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models"));
        dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models/research"));
    }
    if let Ok(extra) = std::env::var("BLURLIST_MODELS") {
        dirs.push(PathBuf::from(extra));
    }
    dirs
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("media", media::handle)
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let user_models_dir = data_dir.join("models");
            std::fs::create_dir_all(&user_models_dir)?;
            let store = Store::open(&data_dir.join("blurlist.sqlite3"))?;
            store.recover_interrupted_scans()?;
            let dirs = model_dirs(app, &user_models_dir);
            app.manage(AppState::new(store, data_dir, dirs, user_models_dir));

            // Load the models in the background so the first scan starts instantly.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = handle.state::<AppState>().engine();
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_models,
            commands::import_model,
            commands::get_settings,
            commands::save_settings,
            commands::reveal_path,
            commands::open_external,
            commands::cancel_task,
            commands::list_people,
            commands::create_person,
            commands::update_person,
            commands::delete_person,
            commands::delete_reference,
            commands::inspect_photo,
            commands::add_reference,
            commands::import_roster,
            commands::start_scan,
            commands::list_scans,
            commands::get_scan,
            commands::delete_scan,
            commands::rematch,
            commands::review_items,
            commands::set_decision,
            commands::restore_decision,
            commands::bulk_decision,
            commands::photo_detail,
            commands::add_manual_region,
            commands::delete_manual_region,
            commands::export_scan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Blurlist");
}
