use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::permissions::PermissionMeta;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The glob pattern to match files against" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        todo!("Implement in next task")
    }

    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }
}
