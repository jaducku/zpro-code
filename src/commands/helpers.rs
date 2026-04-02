/// Creates a prompt telling the user a command has moved to a plugin.
pub fn create_moved_to_plugin_prompt(old_name: &str, plugin_name: &str) -> String {
    format!(
        "The /{old_name} command has been moved to the '{plugin_name}' plugin. \
         Please install it with: /install-plugin {plugin_name}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_moved_to_plugin_prompt() {
        let prompt = create_moved_to_plugin_prompt("security-review", "security");
        assert!(prompt.contains("/security-review"));
        assert!(prompt.contains("security"));
        assert!(prompt.contains("/install-plugin"));
    }
}
