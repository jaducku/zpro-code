//! Tauri IPC commands for split-screen frontend integration
//!
//! These commands are only compiled when the "tauri" feature is enabled.
//! They provide a request-response interface for the frontend to query application state.

#![allow(unexpected_cfgs)]

#[cfg(feature = "tauri")]
pub mod ipc {
    use crate::types::tool_state::MainLoopState;
    use std::sync::Mutex;

    /// Tauri application state wrapper
    /// This holds references to the current app state for IPC access
    pub struct TauriAppState {
        pub app: Mutex<crate::tui::app::App>,
    }

    /// Get the current main loop state
    /// Returns None if no loop is currently active
    #[tauri::command]
    pub fn get_main_loop_state(state: tauri::State<'_, TauriAppState>) -> Option<MainLoopState> {
        state
            .app
            .lock()
            .ok()
            .and_then(|app| app.current_main_loop.clone())
    }
}

#[cfg(not(feature = "tauri"))]
pub mod ipc {
    // Stub for when Tauri feature is disabled
}
