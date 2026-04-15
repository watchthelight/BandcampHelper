#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod bandcamp;
mod download;
mod models;
mod state;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct BandcampState {
    pub cookies: Mutex<Option<Arc<reqwest::cookie::Jar>>>,
    pub client: Mutex<Option<reqwest::Client>>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for BandcampState {
    fn default() -> Self {
        Self {
            cookies: Mutex::new(None),
            client: Mutex::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(BandcampState::default())
        .invoke_handler(tauri::generate_handler![
            auth::try_restore_session,
            auth::login,
            auth::check_auth,
            auth::fetch_collection,
            download::start_downloads,
            download::cancel_downloads,
            download::get_default_output_directory,
            download::check_local_albums,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
