#![forbid(unsafe_code)]

mod commands;
mod llm;
mod llm_config;
mod merchant_cache;
mod state;
mod upload;
mod user_rules;

use state::AppState;
use std::path::PathBuf;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Data root: env override for dev, else <app_local_data_dir>/data.
            let data_path = match std::env::var("FM_DATA_ROOT") {
                Ok(p) => PathBuf::from(p),
                Err(_) => app
                    .path()
                    .app_local_data_dir()
                    .map_err(|e| format!("could not resolve app_local_data_dir: {e}"))?
                    .join("data"),
            };
            std::fs::create_dir_all(&data_path)?;
            let state = AppState::new(&data_path).map_err(|e| e.to_string())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
            commands::unlock_profile,
            commands::unlock_with_recovery,
            commands::lock_profile,
            commands::current_profile,
            upload::upload_pdf,
            upload::list_imports,
            upload::get_import,
            upload::delete_import,
            upload::recategorize_transaction,
            user_rules::list_user_rules,
            user_rules::delete_user_rule,
            llm_config::get_llm_config,
            llm_config::set_llm_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
