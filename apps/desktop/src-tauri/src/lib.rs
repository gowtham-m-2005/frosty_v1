mod commands;
mod shell;
mod state;
mod windows;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            shell::summon(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shell::on_shortcut)
                .build(),
        )
        .setup(shell::setup)
        .on_window_event(|window, event| {
            // Closing the main window returns to compact mode instead of
            // quitting; Quit lives in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == windows::MAIN
            {
                api.prevent_close();
                let _ = window.hide();
                shell::publish(
                    window.app_handle(),
                    "core.fullscreen_exited",
                    serde_json::json!({}),
                    None,
                );
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config_value,
            commands::fire_event,
            commands::report_hit_rects,
            commands::summon_painted,
            commands::overlay_hidden,
            commands::toggle_summon,
            commands::toggle_fullscreen,
            commands::open_main,
            commands::list_themes,
            commands::resolve_theme,
            commands::get_notices,
            commands::get_fox_position,
            commands::save_fox_position,
            commands::get_paths,
            commands::reveal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
