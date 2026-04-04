pub mod agent_tool;
pub mod ask_user_question_tool;
pub mod bash_tool;
pub mod file_edit_tool;
pub mod file_read_tool;
pub mod file_write_tool;
pub mod glob_tool;
pub mod grep_tool;
pub mod notebook_edit_tool;
pub mod send_message_tool;
pub mod skill_tool;
pub mod sleep_tool;
pub mod synthetic_output_tool;
pub mod task_create_tool;
pub mod task_get_tool;
pub mod task_list_tool;
pub mod task_output_tool;
pub mod task_stop_tool;
pub mod task_update_tool;
pub mod todo_write_tool;
pub mod tool_search_tool;
pub mod web_fetch_tool;
pub mod web_search_tool;

use crate::tool::Tool;

/// Collect all built-in tools
pub fn get_builtin_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(agent_tool::AgentTool),
        Box::new(ask_user_question_tool::AskUserQuestionTool),
        Box::new(bash_tool::BashTool),
        Box::new(file_read_tool::FileReadTool),
        Box::new(file_edit_tool::FileEditTool),
        Box::new(file_write_tool::FileWriteTool),
        Box::new(glob_tool::GlobTool),
        Box::new(grep_tool::GrepTool),
        Box::new(notebook_edit_tool::NotebookEditTool),
        Box::new(send_message_tool::SendMessageTool),
        Box::new(skill_tool::SkillTool),
        Box::new(sleep_tool::SleepTool),
        Box::new(task_create_tool::TaskCreateTool),
        Box::new(task_get_tool::TaskGetTool),
        Box::new(task_list_tool::TaskListTool),
        Box::new(task_output_tool::TaskOutputTool),
        Box::new(task_stop_tool::TaskStopTool),
        Box::new(task_update_tool::TaskUpdateTool),
        Box::new(todo_write_tool::TodoWriteTool),
        Box::new(tool_search_tool::ToolSearchTool),
        Box::new(web_fetch_tool::WebFetchTool),
        Box::new(web_search_tool::WebSearchTool),
    ]
}

/// Find a tool by name or alias from a list
pub fn find_tool_by_name<'a>(tools: &'a [Box<dyn Tool>], name: &str) -> Option<&'a dyn Tool> {
    tools
        .iter()
        .find(|t| t.name() == name || t.aliases().contains(&name))
        .map(|t| t.as_ref())
}

/// Check if a tool should be deferred (schema not sent upfront)
pub fn is_deferred_tool(tool: &dyn Tool) -> bool {
    if tool.always_load() {
        return false;
    }
    if tool.is_mcp() {
        return true;
    }
    if tool.name() == "ToolSearch" {
        return false;
    }
    tool.should_defer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_tools() {
        let tools = get_builtin_tools();
        assert!(tools.len() >= 7);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Edit"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"Grep"));
    }

    #[test]
    fn test_find_tool_by_name() {
        let tools = get_builtin_tools();
        assert!(find_tool_by_name(&tools, "Bash").is_some());
        assert!(find_tool_by_name(&tools, "NonExistent").is_none());
    }

    #[test]
    fn test_deferred_tools() {
        let tools = get_builtin_tools();
        // Tools that opt into deferral via should_defer()
        let expected_deferred = &["SendMessage"];
        for tool in &tools {
            let deferred = is_deferred_tool(tool.as_ref());
            if expected_deferred.contains(&tool.name()) {
                assert!(
                    deferred,
                    "Tool '{}' should be deferred",
                    tool.name()
                );
            } else {
                assert!(
                    !deferred,
                    "Tool '{}' should not be deferred",
                    tool.name()
                );
            }
        }
    }

    #[test]
    fn test_is_deferred_tool_mcp() {
        use async_trait::async_trait;
        use crate::tool::{ToolResult, ToolUseContext};

        struct MockMcpTool;

        #[async_trait]
        impl Tool for MockMcpTool {
            fn name(&self) -> &str { "mcp__test" }
            fn description(&self) -> &str { "Mock MCP tool" }
            fn input_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object"})
            }
            async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
                unimplemented!()
            }
            fn is_mcp(&self) -> bool { true }
        }

        assert!(is_deferred_tool(&MockMcpTool));
    }

    #[test]
    fn test_is_deferred_tool_always_load_overrides() {
        use async_trait::async_trait;
        use crate::tool::{ToolResult, ToolUseContext};

        struct AlwaysLoadMcpTool;

        #[async_trait]
        impl Tool for AlwaysLoadMcpTool {
            fn name(&self) -> &str { "mcp__important" }
            fn description(&self) -> &str { "Important MCP tool" }
            fn input_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object"})
            }
            async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
                unimplemented!()
            }
            fn is_mcp(&self) -> bool { true }
            fn always_load(&self) -> bool { true }
        }

        assert!(!is_deferred_tool(&AlwaysLoadMcpTool));
    }
}
