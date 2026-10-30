pub mod analyzer;
pub mod engine;
pub mod export;
pub mod library;
pub mod project;
pub mod session;
pub mod toolchain;

use tauri_plugin_sql::{Migration, MigrationKind};

use export::{export_finalize, export_start, export_stop};
use library::transform::{dirt_samples_groups, sample_reload, sample_transform};
use project::{project_open, project_save};
use session::{Session, session_eval, session_eval_file, session_restart, session_start};
use toolchain::{audio_devices, toolchain_detect};

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
        .invoke_handler(tauri::generate_handler![
            greet,
            session_start,
            session_eval,
            session_eval_file,
            session_restart,
            project_open,
            project_save,
            toolchain_detect,
            audio_devices,
            sample_transform,
            sample_reload,
            dirt_samples_groups,
            export_start,
            export_stop,
            export_finalize
        ])
        .build(tauri::generate_context!())
        .expect("error while building tIDE")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                use tauri::Manager;
                // Blocks until ghci *and* the engine are down — no orphaned synths.
                app_handle.state::<Session>().stop();
            }
        });
}
