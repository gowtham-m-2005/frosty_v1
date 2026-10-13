mod caps;
mod commands;
mod fullscreen;
mod notify;
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .register_uri_scheme_protocol("frosty-cap", |ctx, request| {
            caps::serve(ctx.app_handle(), request.uri().path())
        })
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
            caps::list_capabilities,
            caps::enable_capability,
            caps::disable_capability,
            caps::capability_activated,
            caps::capability_failed,
            caps::rescan_capabilities,
            caps::remove_capability,
            caps::install_capability_from_folder,
            caps::capability_reveal,
            caps::get_capability_schema,
            caps::get_capability_settings,
            caps::set_capability_settings,
            caps::call_tool,
            caps::tool_result,
            caps::cap_db,
            caps::cap_emit,
            caps::cap_schedule,
            caps::cap_notify,
            caps::cap_badge,
            caps::cap_log,
            notify::list_notifications,
            notify::notifications_mark_read,
            notify::dismiss_notification,
            notify::run_notification_action,
            notify::get_dnd,
            notify::set_dnd_manual,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
