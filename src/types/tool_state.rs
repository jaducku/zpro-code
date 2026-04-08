// src/types/tool_state.rs
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Tool call execution state (real-time tracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub tool_use_id: String,
    pub parent_agent_id: Option<String>, // Which agent called this tool
    pub parent_tool_use_id: Option<String>, // For nested tool relationships
    pub tool_name: String,
    pub status: ToolStatus,
    pub created_at: SystemTime,
    pub params: Option<serde_json::Value>, // Set on InputReady
    pub result: Option<ToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Completed(u64), // elapsed_ms
    Failed(String), // error message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String, // Truncated to ~500 chars
    pub full_content_available: bool,
}

/// Main loop state (user input → all agents idle)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainLoopState {
    pub loop_id: String,
    pub started_at: SystemTime,
    pub status: MainLoopStatus,
    pub tools: Vec<ToolCallState>, // Flat list; hierarchy via parent IDs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MainLoopStatus {
    Active,
    Idle,         // All agents idle, no pending tools
    AwaitingHitl, // HITL input required
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a test ToolCallState
    fn create_test_tool(id: &str, name: &str) -> ToolCallState {
        ToolCallState {
            tool_use_id: id.to_string(),
            parent_agent_id: None,
            parent_tool_use_id: None,
            tool_name: name.to_string(),
            status: ToolStatus::Running,
            created_at: SystemTime::now(),
            params: None,
            result: None,
        }
    }

    // ============= ToolCallState Tests =============

    #[test]
    fn tool_call_state_creation() {
        let tool = create_test_tool("tool_1", "Read");
        assert_eq!(tool.tool_use_id, "tool_1");
        assert_eq!(tool.tool_name, "Read");
        assert!(matches!(tool.status, ToolStatus::Running));
        assert!(tool.parent_agent_id.is_none());
        assert!(tool.parent_tool_use_id.is_none());
        assert!(tool.params.is_none());
        assert!(tool.result.is_none());
    }

    #[test]
    fn tool_state_starts_in_running_state() {
        let tool = create_test_tool("tool_1", "Bash");
        assert!(matches!(tool.status, ToolStatus::Running));
    }

    #[test]
    fn tool_transitions_from_running_to_completed() {
        let mut tool = create_test_tool("tool_1", "Read");
        tool.status = ToolStatus::Completed(150);
        assert!(matches!(tool.status, ToolStatus::Completed(150)));
    }

    #[test]
    fn tool_transitions_from_running_to_failed() {
        let mut tool = create_test_tool("tool_1", "Bash");
        tool.status = ToolStatus::Failed("timeout".to_string());
        match tool.status {
            ToolStatus::Failed(msg) => assert_eq!(msg, "timeout"),
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn tool_can_track_parent_agent() {
        let mut tool = create_test_tool("tool_1", "AgentTool");
        tool.parent_agent_id = Some("agent_xyz".to_string());
        assert_eq!(tool.parent_agent_id, Some("agent_xyz".to_string()));
    }

    #[test]
    fn tool_can_track_parent_tool() {
        let mut tool = create_test_tool("tool_1", "Read");
        tool.parent_tool_use_id = Some("parent_tool_id".to_string());
        assert_eq!(tool.parent_tool_use_id, Some("parent_tool_id".to_string()));
    }

    #[test]
    fn tool_can_store_params() {
        let mut tool = create_test_tool("tool_1", "Read");
        let params = serde_json::json!({"path": "/tmp/file.txt"});
        tool.params = Some(params.clone());
        assert_eq!(tool.params.unwrap(), params);
    }

    #[test]
    fn tool_result_tracks_content() {
        let result = ToolResult {
            content: "Output: success".to_string(),
            full_content_available: true,
        };
        assert_eq!(result.content, "Output: success");
        assert!(result.full_content_available);
    }

    #[test]
    fn tool_can_store_result() {
        let mut tool = create_test_tool("tool_1", "Read");
        let result = ToolResult {
            content: "File contents".to_string(),
            full_content_available: true,
        };
        tool.result = Some(result.clone());
        assert!(tool.result.is_some());
        assert_eq!(tool.result.unwrap().content, "File contents");
    }

    #[test]
    fn tool_result_truncated_flag() {
        let result = ToolResult {
            content: "Truncated output...".to_string(),
            full_content_available: false,
        };
        assert!(!result.full_content_available);
        assert_eq!(result.content, "Truncated output...");
    }

    // ============= ToolStatus Tests =============

    #[test]
    fn completed_status_includes_elapsed_time() {
        let status = ToolStatus::Completed(1234);
        match status {
            ToolStatus::Completed(ms) => assert_eq!(ms, 1234),
            _ => panic!("Expected Completed status"),
        }
    }

    #[test]
    fn completed_status_with_different_durations() {
        let test_cases = vec![0u64, 1, 100, 1000, 999999];
        for duration_ms in test_cases {
            let status = ToolStatus::Completed(duration_ms);
            match status {
                ToolStatus::Completed(ms) => assert_eq!(ms, duration_ms),
                _ => panic!("Expected Completed status"),
            }
        }
    }

    #[test]
    fn failed_status_includes_error_message() {
        let error_msg = "Connection timeout";
        let status = ToolStatus::Failed(error_msg.to_string());

        match status {
            ToolStatus::Failed(msg) => assert_eq!(msg, error_msg),
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn failed_status_with_different_errors() {
        let errors = vec![
            "Connection timeout",
            "File not found",
            "Invalid parameters",
            "Process killed",
        ];

        for error in errors {
            let status = ToolStatus::Failed(error.to_string());
            match status {
                ToolStatus::Failed(msg) => assert_eq!(msg, error),
                _ => panic!("Expected Failed status"),
            }
        }
    }

    // ============= MainLoopState Tests =============

    #[test]
    fn main_loop_state_creation() {
        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };
        assert_eq!(loop_state.loop_id, "loop_1");
        assert!(matches!(loop_state.status, MainLoopStatus::Active));
        assert!(loop_state.tools.is_empty());
    }

    #[test]
    fn main_loop_state_starts_active() {
        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };
        assert!(matches!(loop_state.status, MainLoopStatus::Active));
    }

    #[test]
    fn main_loop_status_transitions_to_idle() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };
        loop_state.status = MainLoopStatus::Idle;
        assert!(matches!(loop_state.status, MainLoopStatus::Idle));
    }

    #[test]
    fn main_loop_status_transitions_to_awaiting_hitl() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };
        loop_state.status = MainLoopStatus::AwaitingHitl;
        assert!(matches!(loop_state.status, MainLoopStatus::AwaitingHitl));
    }

    #[test]
    fn main_loop_collects_single_tool() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };

        let tool1 = create_test_tool("tool_1", "Read");
        loop_state.tools.push(tool1);

        assert_eq!(loop_state.tools.len(), 1);
        assert_eq!(loop_state.tools[0].tool_use_id, "tool_1");
    }

    #[test]
    fn main_loop_collects_multiple_tools() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };

        loop_state.tools.push(create_test_tool("tool_1", "Read"));
        loop_state.tools.push(create_test_tool("tool_2", "Bash"));
        loop_state.tools.push(create_test_tool("tool_3", "Edit"));

        assert_eq!(loop_state.tools.len(), 3);
    }

    #[test]
    fn main_loop_is_idle_when_all_tools_completed() {
        let mut tool1 = create_test_tool("tool_1", "Read");
        tool1.status = ToolStatus::Completed(100);

        let mut tool2 = create_test_tool("tool_2", "Bash");
        tool2.status = ToolStatus::Completed(200);

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![tool1, tool2],
        };

        // All tools completed - check idle detection
        let all_completed = loop_state
            .tools
            .iter()
            .all(|t| !matches!(t.status, ToolStatus::Running));
        assert!(all_completed);
    }

    #[test]
    fn main_loop_not_idle_when_tool_running() {
        let tool1 = create_test_tool("tool_1", "Read"); // Running
        let mut tool2 = create_test_tool("tool_2", "Bash");
        tool2.status = ToolStatus::Completed(200);

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![tool1, tool2],
        };

        // One tool still running - not idle
        let any_running = loop_state
            .tools
            .iter()
            .any(|t| matches!(t.status, ToolStatus::Running));
        assert!(any_running);
    }

    #[test]
    fn main_loop_is_idle_when_all_tools_failed() {
        let mut tool1 = create_test_tool("tool_1", "Read");
        tool1.status = ToolStatus::Failed("error 1".to_string());

        let mut tool2 = create_test_tool("tool_2", "Bash");
        tool2.status = ToolStatus::Failed("error 2".to_string());

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![tool1, tool2],
        };

        // All tools failed (not running) - should be idle
        let all_completed = loop_state
            .tools
            .iter()
            .all(|t| !matches!(t.status, ToolStatus::Running));
        assert!(all_completed);
    }

    #[test]
    fn main_loop_mixed_completed_and_failed() {
        let mut tool1 = create_test_tool("tool_1", "Read");
        tool1.status = ToolStatus::Completed(100);

        let mut tool2 = create_test_tool("tool_2", "Bash");
        tool2.status = ToolStatus::Failed("error".to_string());

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![tool1, tool2],
        };

        // All tools not running (completed or failed)
        let all_completed = loop_state
            .tools
            .iter()
            .all(|t| !matches!(t.status, ToolStatus::Running));
        assert!(all_completed);
    }

    #[test]
    fn main_loop_with_nested_tools() {
        let mut parent_tool = create_test_tool("parent_1", "AgentTool");
        parent_tool.status = ToolStatus::Completed(500);

        let mut child_tool = create_test_tool("child_1", "Read");
        child_tool.parent_tool_use_id = Some("parent_1".to_string());
        child_tool.status = ToolStatus::Completed(100);

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![parent_tool, child_tool],
        };

        // Check hierarchy tracking via parent IDs
        let child = &loop_state.tools[1];
        assert_eq!(
            child.parent_tool_use_id,
            Some("parent_1".to_string()),
            "Child tool should track parent"
        );
    }

    #[test]
    fn main_loop_with_agent_owned_tools() {
        let mut tool1 = create_test_tool("tool_1", "Read");
        tool1.parent_agent_id = Some("agent_alice".to_string());

        let mut tool2 = create_test_tool("tool_2", "Bash");
        tool2.parent_agent_id = Some("agent_bob".to_string());

        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: vec![tool1, tool2],
        };

        // Check agent ownership tracking
        assert_eq!(
            loop_state.tools[0].parent_agent_id,
            Some("agent_alice".to_string())
        );
        assert_eq!(
            loop_state.tools[1].parent_agent_id,
            Some("agent_bob".to_string())
        );
    }

    #[test]
    fn main_loop_empty_tools_list_is_idle() {
        let loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };

        // No running tools when list is empty
        let all_completed = loop_state
            .tools
            .iter()
            .all(|t| !matches!(t.status, ToolStatus::Running));
        assert!(all_completed);
    }

    #[test]
    fn main_loop_status_all_variants() {
        let variants = vec![
            MainLoopStatus::Active,
            MainLoopStatus::Idle,
            MainLoopStatus::AwaitingHitl,
        ];

        for status in variants {
            let loop_state = MainLoopState {
                loop_id: "loop_1".to_string(),
                started_at: SystemTime::now(),
                status: status.clone(),
                tools: Vec::new(),
            };

            // Should be assignable and retrievable
            assert!(matches!(
                loop_state.status,
                MainLoopStatus::Active | MainLoopStatus::Idle | MainLoopStatus::AwaitingHitl
            ));
        }
    }

    // ============= Integration Tests =============

    #[test]
    fn tool_life_cycle_running_to_completed() {
        let mut tool = create_test_tool("tool_1", "Read");

        // Initial state
        assert!(matches!(tool.status, ToolStatus::Running));
        assert!(tool.result.is_none());

        // Add params
        tool.params = Some(serde_json::json!({"path": "/tmp/file.txt"}));

        // Transition to completed
        tool.status = ToolStatus::Completed(150);
        tool.result = Some(ToolResult {
            content: "File contents".to_string(),
            full_content_available: true,
        });

        // Verify final state
        assert!(matches!(tool.status, ToolStatus::Completed(150)));
        assert!(tool.result.is_some());
        assert_eq!(tool.result.unwrap().content, "File contents");
    }

    #[test]
    fn tool_life_cycle_running_to_failed() {
        let mut tool = create_test_tool("tool_1", "Bash");

        // Initial state
        assert!(matches!(tool.status, ToolStatus::Running));

        // Add params
        tool.params = Some(serde_json::json!({"command": "ls /nonexistent"}));

        // Transition to failed
        tool.status = ToolStatus::Failed("File not found".to_string());

        // Verify final state
        assert!(matches!(tool.status, ToolStatus::Failed(ref msg) if msg == "File not found"));
    }

    #[test]
    fn main_loop_progresses_through_states() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };

        // Add tools
        loop_state.tools.push(create_test_tool("tool_1", "Read"));
        assert!(matches!(loop_state.status, MainLoopStatus::Active));
        assert_eq!(loop_state.tools.len(), 1);

        // Complete all tools
        loop_state.tools[0].status = ToolStatus::Completed(100);

        // Transition to idle
        loop_state.status = MainLoopStatus::Idle;
        assert!(matches!(loop_state.status, MainLoopStatus::Idle));
    }

    #[test]
    fn main_loop_can_request_hitl() {
        let mut loop_state = MainLoopState {
            loop_id: "loop_1".to_string(),
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        };

        // Transition to awaiting HITL
        loop_state.status = MainLoopStatus::AwaitingHitl;
        assert!(matches!(loop_state.status, MainLoopStatus::AwaitingHitl));
    }
}
