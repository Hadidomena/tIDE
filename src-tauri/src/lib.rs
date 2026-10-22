pub mod analyzer;
pub mod engine;
pub mod library;
pub mod project;
pub mod session;
pub mod toolchain;

use tauri_plugin_sql::{Migration, MigrationKind};

use engine::EngineLauncher;
use project::{project_open, project_save};
use session::{Session, session_eval, session_restart, session_start};
use toolchain::toolchain_detect;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "init v2 schema (plan-v2 §8)",
        sql: include_str!("../migrations/0001_init.sql"),
        kind: MigrationKind::Up,
    }];

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:tide.db", migrations)
                .build(),
        )
        .manage(Session::new())
        .manage(EngineLauncher::new())
        .invoke_handler(tauri::generate_handler![
            greet,
            session_start,
            session_eval,
            session_restart,
            project_open,
            project_save,
            toolchain_detect
        ])
        .build(tauri::generate_context!())
        .expect("error while building tIDE")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                use tauri::Manager;
                app_handle.state::<Session>().stop();
                app_handle.state::<EngineLauncher>().stop();
            }
        });
}
