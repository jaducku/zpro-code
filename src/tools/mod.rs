pub mod agent_tool;
pub mod ask_user_question_tool;
pub mod bash_tool;
pub mod file_edit_tool;
pub mod file_read_tool;
pub mod file_write_tool;
pub mod glob_tool;
pub mod grep_tool;
pub mod notebook_edit_tool;
pub mod skill_tool;
pub mod sleep_tool;
pub mod task_create_tool;
pub mod task_get_tool;
pub mod task_list_tool;
pub mod task_update_tool;
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
        Box::new(skill_tool::SkillTool),
        Box::new(sleep_tool::SleepTool),
        Box::new(task_create_tool::TaskCreateTool),
        Box::new(task_get_tool::TaskGetTool),
        Box::new(task_list_tool::TaskListTool),
        Box::new(task_update_tool::TaskUpdateTool),
        Box::new(web_fetch_tool::WebFetchTool),
        Box::new(web_search_tool::WebSearchTool),
    ]
}

/// Find a tool by name from a list
pub fn find_tool_by_name<'a>(tools: &'a [Box<dyn Tool>], name: &str) -> Option<&'a dyn Tool> {
    tools.iter().find(|t| t.name() == name).map(|t| t.as_ref())
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
}
