pub mod loader;

pub use loader::OutputStyleConfig;

use std::collections::HashMap;
use std::path::Path;

use crate::utils::paths::global_config_dir;
use loader::{get_builtin_styles, load_styles_from_dir};

/// Collect all output styles, layered: built-in -> user -> project.
/// Later sources override earlier ones by name.
pub fn get_all_output_styles(cwd: &Path) -> HashMap<String, OutputStyleConfig> {
    let mut map = HashMap::new();

    // Built-in styles
    for style in get_builtin_styles() {
        map.insert(style.name.clone(), style);
    }

    // User-level: ~/.atom/output-styles/
    let user_dir = global_config_dir().join("output-styles");
    for style in load_styles_from_dir(&user_dir, "userSettings") {
        map.insert(style.name.clone(), style);
    }

    // Project-level: <cwd>/.agents/output-styles/
    let project_dir = cwd.join(".agents").join("output-styles");
    for style in load_styles_from_dir(&project_dir, "projectSettings") {
        map.insert(style.name.clone(), style);
    }

    map
}

/// Get the output style config for a selected style name.
///
/// - If any plugin style has `force_for_plugin`, return it regardless of `selected`.
/// - If `selected` is "default", return None (no injection needed).
/// - Otherwise, look up the selected style.
pub fn get_output_style_config(cwd: &Path, selected: &str) -> Option<OutputStyleConfig> {
    let all = get_all_output_styles(cwd);

    // Check for forced plugin styles
    if let Some(forced) = all.values().find(|s| s.force_for_plugin && s.source == "plugin") {
        return Some(forced.clone());
    }

    if selected == "default" {
        return None;
    }

    all.get(selected).cloned()
}
