use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::permissions::PermissionMeta;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacements in files"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "The absolute path to the file to modify" },
                "old_string": { "type": "string", "description": "The text to replace" },
                "new_string": { "type": "string", "description": "The text to replace it with" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        todo!("Implement in next task")
    }

    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: input
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(String::from),
            command: None,
            url: None,
        }
    }
}
