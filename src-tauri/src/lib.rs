#![recursion_limit = "256"]
mod commands;
mod store;
mod ytdlp;

use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let store = store::AppStore::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::parse_media,
            commands::parse_cancel,
            commands::start_download,
            commands::pause_task,
            commands::resume_task,
            commands::cancel_task,
            commands::remove_task,
            commands::redownload_task,
            commands::get_tasks,
            commands::open_save_dir,
            commands::choose_dir,
            commands::get_settings,
            commands::set_settings,
            commands::get_app_info,
            commands::read_clipboard,
            commands::open_builtin_browser,
            commands::check_builtin_cookies,
            commands::sniffer_report,
        ])
        // 与 commands.rs 中 State<'_, Mutex<AppStore>> 的类型保持一致
        .manage(Mutex::new(store))
        .run(tauri::generate_context!())
}
