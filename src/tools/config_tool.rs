use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct ConfigTool;

// --- Settings registry ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy)]
pub enum SettingType {
    String,
    Bool,
    Enum(&'static [&'static str]),
}

pub struct SettingDef {
    pub key: &'static str,
    pub source: SettingSource,
    pub value_type: SettingType,
    pub description: &'static str,
}

pub static SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "theme",
        source: SettingSource::Global,
        value_type: SettingType::Enum(&["dark", "light", "light-daltonized", "dark-daltonized"]),
        description: "UI color theme",
    },
    SettingDef {
        key: "verbose",
        source: SettingSource::Global,
        value_type: SettingType::Bool,
        description: "Enable verbose logging",
    },
    SettingDef {
        key: "autoCompactEnabled",
        source: SettingSource::Global,
        value_type: SettingType::Bool,
        description: "Enable automatic context compaction",
    },
    SettingDef {
        key: "model",
        source: SettingSource::Project,
        value_type: SettingType::String,
        description: "Default model for this project",
    },
    SettingDef {
        key: "permissions.defaultMode",
        source: SettingSource::Project,
        value_type: SettingType::Enum(&[
            "default",
            "plan",
            "acceptEdits",
            "bypassPermissions",
        ]),
        description: "Default permission mode",
    },
    SettingDef {
        key: "language",
        source: SettingSource::Project,
        value_type: SettingType::String,
        description: "Preferred language for responses",
    },
];

pub fn find_setting(key: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|s| s.key == key)
}

/// Validate and coerce a value for a setting type.
/// Returns the coerced JSON value or an error message.
pub fn validate_value(setting: &SettingDef, value: &Value) -> Result<Value, String> {
    match setting.value_type {
        SettingType::String => match value {
            Value::String(_) => Ok(value.clone()),
            other => Ok(Value::String(other.to_string())),
        },
        SettingType::Bool => match value {
            Value::Bool(_) => Ok(value.clone()),
            Value::String(s) => match s.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(format!(
                    "Invalid boolean value '{}'. Use true or false.",
                    s
                )),
            },
            _ => Err(format!(
                "Invalid boolean value '{}'. Use true or false.",
                value
            )),
        },
        SettingType::Enum(options) => {
            let s = match value {
                Value::String(s) => s.clone(),
                other => other.to_string().trim_matches('"').to_string(),
            };
            if options.contains(&s.as_str()) {
                Ok(Value::String(s))
            } else {
                Err(format!(
                    "Invalid value '{}'. Must be one of: {}",
                    s,
                    options.join(", ")
                ))
            }
        }
    }
}

// --- Dot-path helpers ---

pub fn get_nested(value: &Value, path: &str) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for part in parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return None,
        }
    }
    Some(current.clone())
}

pub fn set_nested(value: &mut Value, path: &str, new_val: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current[*part] = new_val;
            return;
        }
        if !current.get(*part).map_or(false, |v| v.is_object()) {
            current[*part] = Value::Object(serde_json::Map::new());
        }
        current = current.get_mut(*part).unwrap();
    }
}

// --- File-based project settings ---

fn project_settings_path() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    cwd.join(".agents").join("settings.json")
}

fn load_project_settings() -> Value {
    let path = project_settings_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(Value::Object(serde_json::Map::new())),
        Err(_) => Value::Object(serde_json::Map::new()),
    }
}

fn save_project_settings(value: &Value) -> Result<(), String> {
    let path = project_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write settings: {}", e))?;
    Ok(())
}

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "ConfigTool"
    }

    fn description(&self) -> &str {
        "Get or set configuration settings. Omit value to read a setting, provide value to write it."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "setting": {
                    "type": "string",
                    "description": "The setting key to get or set"
                },
                "value": {
                    "description": "The value to set. Omit to read the current value."
                }
            },
            "required": ["setting"]
        })
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> { Some("get or set configuration settings") }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let setting_key = ctx
            .input
            .get("setting")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "ConfigTool".into(),
                message: "Missing required 'setting' parameter".into(),
            })?;

        let def = find_setting(setting_key).ok_or_else(|| {
            let available: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
            crate::types::AtomError::Tool {
                tool_name: "ConfigTool".into(),
                message: format!(
                    "Unknown setting '{}'. Available settings: {}",
                    setting_key,
                    available.join(", ")
                ),
            }
        })?;

        let has_value = ctx.input.get("value").is_some();

        if has_value {
            // SET operation
            let raw_value = ctx.input.get("value").unwrap();
            let validated =
                validate_value(def, raw_value).map_err(|msg| crate::types::AtomError::Tool {
                    tool_name: "ConfigTool".into(),
                    message: msg,
                })?;

            match def.source {
                SettingSource::Global => {
                    let mut config_json =
                        serde_json::to_value(crate::services::config::AtomConfig::load())
                            .unwrap_or(Value::Object(serde_json::Map::new()));
                    let previous = get_nested(&config_json, setting_key);
                    set_nested(&mut config_json, setting_key, validated.clone());
                    let config: crate::services::config::AtomConfig =
                        serde_json::from_value(config_json).map_err(|e| {
                            crate::types::AtomError::Tool {
                                tool_name: "ConfigTool".into(),
                                message: format!("Failed to update config: {}", e),
                            }
                        })?;
                    config.save().map_err(|e| crate::types::AtomError::Tool {
                        tool_name: "ConfigTool".into(),
                        message: format!("Failed to save config: {}", e),
                    })?;

                    let result = serde_json::json!({
                        "success": true,
                        "operation": "set",
                        "setting": setting_key,
                        "previousValue": previous.unwrap_or(Value::Null),
                        "newValue": validated,
                    });
                    Ok(ToolResult {
                        content: serde_json::to_string_pretty(&result).unwrap(),
                        is_error: false,
                        content_blocks: None,
                    })
                }
                SettingSource::Project => {
                    let mut settings = load_project_settings();
                    let previous = get_nested(&settings, setting_key);
                    set_nested(&mut settings, setting_key, validated.clone());
                    save_project_settings(&settings).map_err(|msg| {
                        crate::types::AtomError::Tool {
                            tool_name: "ConfigTool".into(),
                            message: msg,
                        }
                    })?;

                    let result = serde_json::json!({
                        "success": true,
                        "operation": "set",
                        "setting": setting_key,
                        "previousValue": previous.unwrap_or(Value::Null),
                        "newValue": validated,
                    });
                    Ok(ToolResult {
                        content: serde_json::to_string_pretty(&result).unwrap(),
                        is_error: false,
                        content_blocks: None,
                    })
                }
            }
        } else {
            // GET operation
            let value = match def.source {
                SettingSource::Global => {
                    let config_json =
                        serde_json::to_value(crate::services::config::AtomConfig::load())
                            .unwrap_or(Value::Object(serde_json::Map::new()));
                    get_nested(&config_json, setting_key)
                }
                SettingSource::Project => {
                    let settings = load_project_settings();
                    get_nested(&settings, setting_key)
                }
            };

            let result = serde_json::json!({
                "success": true,
                "operation": "get",
                "setting": setting_key,
                "value": value.unwrap_or(Value::Null),
            });
            Ok(ToolResult {
                content: serde_json::to_string_pretty(&result).unwrap(),
                is_error: false,
                content_blocks: None,
            })
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(input: Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[test]
    fn test_name() {
        let tool = ConfigTool;
        assert_eq!(tool.name(), "ConfigTool");
    }

    #[test]
    fn test_schema() {
        let tool = ConfigTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "setting"));
        assert!(schema["properties"]["setting"].is_object());
    }

    #[test]
    fn test_find_setting() {
        assert!(find_setting("theme").is_some());
        assert!(find_setting("verbose").is_some());
        assert!(find_setting("model").is_some());
        assert!(find_setting("nonexistent").is_none());
    }

    #[test]
    fn test_validate_bool_coercion() {
        let def = find_setting("verbose").unwrap();
        assert_eq!(
            validate_value(def, &Value::String("true".into())).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            validate_value(def, &Value::String("false".into())).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            validate_value(def, &Value::Bool(true)).unwrap(),
            Value::Bool(true)
        );
        assert!(validate_value(def, &Value::String("maybe".into())).is_err());
    }

    #[test]
    fn test_validate_enum() {
        let def = find_setting("theme").unwrap();
        assert_eq!(
            validate_value(def, &Value::String("dark".into())).unwrap(),
            Value::String("dark".into())
        );
        assert!(validate_value(def, &Value::String("invalid".into())).is_err());
    }

    #[test]
    fn test_get_nested() {
        let val = serde_json::json!({"a": {"b": 42}});
        assert_eq!(get_nested(&val, "a.b"), Some(serde_json::json!(42)));
        assert_eq!(get_nested(&val, "a"), Some(serde_json::json!({"b": 42})));
        assert_eq!(get_nested(&val, "c"), None);
        assert_eq!(get_nested(&val, "a.c"), None);
    }

    #[test]
    fn test_set_nested() {
        let mut val = serde_json::json!({});
        set_nested(&mut val, "a.b", serde_json::json!(42));
        assert_eq!(val, serde_json::json!({"a": {"b": 42}}));

        set_nested(&mut val, "a.b", serde_json::json!(99));
        assert_eq!(val, serde_json::json!({"a": {"b": 99}}));

        set_nested(&mut val, "x", serde_json::json!("hello"));
        assert_eq!(val["x"], serde_json::json!("hello"));
    }

    #[test]
    fn test_unknown_setting() {
        // Verified via find_setting returning None and the error path in execute
        assert!(find_setting("nonexistent").is_none());
        let available: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        assert!(available.contains(&"theme"));
        assert!(available.contains(&"verbose"));
        assert!(available.contains(&"model"));
    }

    #[tokio::test]
    async fn test_get_setting() {
        // GET verbose — reads from global config (which may or may not exist)
        let tool = ConfigTool;
        let ctx = make_ctx(serde_json::json!({"setting": "verbose"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["operation"], "get");
        assert_eq!(parsed["setting"], "verbose");
        assert_eq!(parsed["success"], true);
    }

    #[tokio::test]
    async fn test_missing_setting() {
        let tool = ConfigTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
