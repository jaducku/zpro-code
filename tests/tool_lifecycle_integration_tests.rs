use std::time::SystemTime;
use zpro_code::types::{MainLoopState, MainLoopStatus, ToolCallState, ToolResult, ToolStatus};

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

// Test scenario 1: Full tool execution lifecycle
#[test]
fn full_tool_lifecycle_from_start_to_completion() {
    // Simulate: User sends message → tool starts → params ready → tool completes

    // Step 1: User initiates (create main loop)
    let mut loop_state = MainLoopState {
        loop_id: "loop_1".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Step 2: Tool starts
    let tool = create_test_tool("read_1", "Read");
    loop_state.tools.push(tool);

    // Verify tool is in loop and running
    assert_eq!(loop_state.tools.len(), 1);
    assert!(matches!(loop_state.tools[0].status, ToolStatus::Running));

    // Step 3: Params become ready
    if let Some(tool) = loop_state
        .tools
        .iter_mut()
        .find(|t| t.tool_use_id == "read_1")
    {
        tool.params = Some(serde_json::json!({"path": "/src/main.rs"}));
    }

    // Verify params set
    assert!(loop_state.tools[0].params.is_some());

    // Step 4: Tool completes
    if let Some(tool) = loop_state
        .tools
        .iter_mut()
        .find(|t| t.tool_use_id == "read_1")
    {
        tool.status = ToolStatus::Completed(234);
        tool.result = Some(ToolResult {
            content: "File content here".to_string(),
            full_content_available: true,
        });
    }

    // Verify completion
    assert!(matches!(loop_state.tools[0].status, ToolStatus::Completed(234)));
    assert!(loop_state.tools[0].result.is_some());

    // Step 5: Check if loop should be idle (no running tools)
    let all_tools_done = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(all_tools_done);
}

// Test scenario 2: Multiple tools in sequence
#[test]
fn main_loop_with_multiple_tools_in_sequence() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_2".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Tool 1: Read file
    let mut tool1 = create_test_tool("read_1", "Read");
    tool1.status = ToolStatus::Completed(100);
    tool1.params = Some(serde_json::json!({"path": "/src/main.rs"}));
    tool1.result = Some(ToolResult {
        content: "Code...".to_string(),
        full_content_available: true,
    });

    // Tool 2: Edit file
    let mut tool2 = create_test_tool("edit_1", "Edit");
    tool2.status = ToolStatus::Completed(150);
    tool2.params = Some(serde_json::json!({"path": "/src/main.rs"}));
    tool2.result = Some(ToolResult {
        content: "Edit successful".to_string(),
        full_content_available: true,
    });

    // Tool 3: Bash (still running)
    let tool3 = create_test_tool("bash_1", "Bash");
    // Tool3 is Running by default from create_test_tool

    loop_state.tools.push(tool1);
    loop_state.tools.push(tool2);
    loop_state.tools.push(tool3);

    // Verify 3 tools, 1 running
    assert_eq!(loop_state.tools.len(), 3);
    let running_count = loop_state
        .tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Running))
        .count();
    assert_eq!(running_count, 1);

    // Loop should NOT be idle (tool3 running)
    let all_done = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(!all_done);
}

// Test scenario 3: Tool failure handling
#[test]
fn tool_failure_is_tracked_correctly() {
    let mut tool = create_test_tool("bash_1", "Bash");
    tool.params = Some(serde_json::json!({"command": "invalid_command"}));

    // Tool fails
    tool.status = ToolStatus::Failed("Command not found".to_string());
    tool.result = Some(ToolResult {
        content: "Error: invalid_command not found".to_string(),
        full_content_available: true,
    });

    // Verify failure state
    assert!(matches!(tool.status, ToolStatus::Failed(ref msg) if msg == "Command not found"));
    assert!(tool.result.is_some());

    // Failed tool is not "running" (for idle detection)
    assert!(!matches!(tool.status, ToolStatus::Running));
}

// Test scenario 4: Subagent tool hierarchy
#[test]
fn subagent_tool_hierarchy_tracked_correctly() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_3".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Main agent calls tool
    let mut main_tool = create_test_tool("main_read", "Read");
    main_tool.status = ToolStatus::Completed(100);

    // Subagent (planner) calls tool (child of subagent)
    let mut subagent_tool = create_test_tool("planner_grep", "Grep");
    subagent_tool.parent_agent_id = Some("planner".to_string());
    // subagent_tool.status is Running by default

    loop_state.tools.push(main_tool);
    loop_state.tools.push(subagent_tool);

    // Verify hierarchy
    assert_eq!(loop_state.tools.len(), 2);
    assert!(loop_state.tools[0].parent_agent_id.is_none()); // Main
    assert_eq!(
        loop_state.tools[1].parent_agent_id,
        Some("planner".to_string())
    ); // Subagent

    // Verify main tool completed, subagent tool still running
    assert!(matches!(loop_state.tools[0].status, ToolStatus::Completed(_)));
    assert!(matches!(loop_state.tools[1].status, ToolStatus::Running));
}

// Test scenario 5: Idle detection with mixed states
#[test]
fn idle_detection_requires_all_tools_not_running() {
    // Scenario: Some completed, some failed, but none running
    let mut tool1 = create_test_tool("tool_1", "Tool1");
    tool1.status = ToolStatus::Completed(100);

    let mut tool2 = create_test_tool("tool_2", "Tool2");
    tool2.status = ToolStatus::Failed("error".to_string());

    let loop_state = MainLoopState {
        loop_id: "loop_4".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![tool1, tool2],
    };

    // All tools are not running (completed OR failed)
    let can_be_idle = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(can_be_idle);
}

// Test scenario 6: Parent tool relationship
#[test]
fn nested_tool_relationship_via_parent_tool_id() {
    let mut parent_tool = create_test_tool("parent_1", "AgentTool");
    parent_tool.status = ToolStatus::Completed(500);

    let mut child_tool = create_test_tool("child_1", "Read");
    child_tool.parent_tool_use_id = Some("parent_1".to_string());
    child_tool.status = ToolStatus::Completed(100);

    let loop_state = MainLoopState {
        loop_id: "loop_5".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![parent_tool, child_tool],
    };

    // Verify parent-child relationship
    assert_eq!(loop_state.tools.len(), 2);
    let child = &loop_state.tools[1];
    assert_eq!(
        child.parent_tool_use_id,
        Some("parent_1".to_string()),
        "Child tool should track parent"
    );
    assert_eq!(
        loop_state.tools[0].parent_tool_use_id, None,
        "Parent tool has no parent"
    );
}

// Test scenario 7: Main loop status transitions
#[test]
fn main_loop_transitions_through_states() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_6".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Verify starts Active
    assert!(matches!(loop_state.status, MainLoopStatus::Active));

    // Add a tool
    loop_state.tools.push(create_test_tool("tool_1", "Read"));
    assert_eq!(loop_state.tools.len(), 1);
    assert!(matches!(loop_state.status, MainLoopStatus::Active));

    // Complete the tool
    loop_state.tools[0].status = ToolStatus::Completed(100);

    // Transition to Idle
    loop_state.status = MainLoopStatus::Idle;
    assert!(matches!(loop_state.status, MainLoopStatus::Idle));

    // Transition to AwaitingHitl
    loop_state.status = MainLoopStatus::AwaitingHitl;
    assert!(matches!(loop_state.status, MainLoopStatus::AwaitingHitl));
}

// Test scenario 8: Multiple agents working on same loop
#[test]
fn multiple_agents_tools_in_same_loop() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_7".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Agent Alice's tool
    let mut alice_tool = create_test_tool("alice_read", "Read");
    alice_tool.parent_agent_id = Some("agent_alice".to_string());
    alice_tool.status = ToolStatus::Completed(100);

    // Agent Bob's tool
    let mut bob_tool = create_test_tool("bob_bash", "Bash");
    bob_tool.parent_agent_id = Some("agent_bob".to_string());
    bob_tool.status = ToolStatus::Running;

    // Main agent's tool
    let mut main_tool = create_test_tool("main_edit", "Edit");
    main_tool.parent_agent_id = None;
    main_tool.status = ToolStatus::Completed(150);

    loop_state.tools.push(alice_tool);
    loop_state.tools.push(bob_tool);
    loop_state.tools.push(main_tool);

    // Verify all tools in loop
    assert_eq!(loop_state.tools.len(), 3);

    // Verify agent ownership
    assert_eq!(
        loop_state.tools[0].parent_agent_id,
        Some("agent_alice".to_string())
    );
    assert_eq!(
        loop_state.tools[1].parent_agent_id,
        Some("agent_bob".to_string())
    );
    assert!(loop_state.tools[2].parent_agent_id.is_none());

    // Verify only Bob's tool running
    let running_count = loop_state
        .tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Running))
        .count();
    assert_eq!(running_count, 1);

    // Loop not idle because Bob's tool still running
    let all_done = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(!all_done);
}

// Test scenario 9: Tool params set before completion
#[test]
fn tool_receives_params_then_completes() {
    let mut tool = create_test_tool("read_1", "Read");
    assert!(tool.params.is_none());

    // Simulate params being set during execution
    tool.params = Some(serde_json::json!({
        "file_path": "/tmp/test.txt"
    }));
    assert!(tool.params.is_some());

    // Tool executes and completes with result
    tool.status = ToolStatus::Completed(50);
    tool.result = Some(ToolResult {
        content: "file contents".to_string(),
        full_content_available: true,
    });

    // Verify full lifecycle
    assert_eq!(tool.tool_use_id, "read_1");
    assert!(tool.params.is_some());
    assert!(matches!(tool.status, ToolStatus::Completed(50)));
    assert!(tool.result.is_some());
}

// Test scenario 10: Truncated results flag
#[test]
fn tool_result_truncation_tracking() {
    let mut tool = create_test_tool("bash_1", "Bash");
    tool.status = ToolStatus::Completed(200);

    // Long output, truncated
    tool.result = Some(ToolResult {
        content: "Very long output truncated...".to_string(),
        full_content_available: false,
    });

    // Verify truncation flag
    assert!(!tool.result.as_ref().unwrap().full_content_available);

    // Same tool with full content available
    let mut tool2 = create_test_tool("bash_2", "Bash");
    tool2.status = ToolStatus::Completed(100);
    tool2.result = Some(ToolResult {
        content: "Short output".to_string(),
        full_content_available: true,
    });

    assert!(tool2.result.as_ref().unwrap().full_content_available);
}

// Test scenario 11: Loop with no tools is idle-ready
#[test]
fn empty_loop_is_idle_ready() {
    let loop_state = MainLoopState {
        loop_id: "loop_8".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // No running tools when list is empty
    let all_done = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(all_done);

    // Can transition to idle
    let mut idle_loop = loop_state.clone();
    idle_loop.status = MainLoopStatus::Idle;
    assert!(matches!(idle_loop.status, MainLoopStatus::Idle));
}

// Test scenario 12: Rapid tool status changes
#[test]
fn tool_rapid_state_transitions() {
    let mut tool = create_test_tool("rapid_1", "Bash");

    // Running -> add params
    assert!(matches!(tool.status, ToolStatus::Running));
    tool.params = Some(serde_json::json!({"cmd": "ls"}));

    // Still running -> add result (shouldn't happen but test handles it)
    tool.result = Some(ToolResult {
        content: "listing...".to_string(),
        full_content_available: true,
    });

    // Running -> Completed
    tool.status = ToolStatus::Completed(10);
    assert!(matches!(tool.status, ToolStatus::Completed(10)));

    // Verify final state has everything
    assert!(tool.params.is_some());
    assert!(tool.result.is_some());
    assert!(matches!(tool.status, ToolStatus::Completed(10)));
}

// Test scenario 13: Complex nested hierarchy with multiple agents
#[test]
fn complex_tool_hierarchy_with_multiple_nesting_levels() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_9".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Top-level agent tool (main agent)
    let mut top_level = create_test_tool("top_1", "AgentTool");
    top_level.parent_agent_id = None;
    top_level.status = ToolStatus::Completed(1000);

    // Second level: planner agent's tool
    let mut planner_tool = create_test_tool("planner_grep", "Grep");
    planner_tool.parent_agent_id = Some("planner".to_string());
    planner_tool.parent_tool_use_id = Some("top_1".to_string());
    planner_tool.status = ToolStatus::Completed(300);

    // Second level: code-reviewer agent's tool
    let mut reviewer_tool = create_test_tool("reviewer_read", "Read");
    reviewer_tool.parent_agent_id = Some("code_reviewer".to_string());
    reviewer_tool.parent_tool_use_id = Some("top_1".to_string());
    reviewer_tool.status = ToolStatus::Running;

    // Regular tool from main agent
    let mut main_tool = create_test_tool("main_edit", "Edit");
    main_tool.parent_agent_id = None;
    main_tool.status = ToolStatus::Completed(150);

    loop_state.tools.push(top_level);
    loop_state.tools.push(planner_tool);
    loop_state.tools.push(reviewer_tool);
    loop_state.tools.push(main_tool);

    // Verify structure
    assert_eq!(loop_state.tools.len(), 4);

    // Verify relationships
    assert!(loop_state.tools[0].parent_agent_id.is_none());
    assert_eq!(
        loop_state.tools[1].parent_agent_id,
        Some("planner".to_string())
    );
    assert_eq!(
        loop_state.tools[1].parent_tool_use_id,
        Some("top_1".to_string())
    );
    assert_eq!(
        loop_state.tools[2].parent_agent_id,
        Some("code_reviewer".to_string())
    );
    assert_eq!(
        loop_state.tools[2].parent_tool_use_id,
        Some("top_1".to_string())
    );

    // Verify only reviewer tool running
    let running = loop_state
        .tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Running))
        .count();
    assert_eq!(running, 1);
}

// Test scenario 14: Elapsed time tracking on completion
#[test]
fn tool_completion_tracks_elapsed_time() {
    let mut tool = create_test_tool("slow_1", "Bash");

    // Tool completes after 5 seconds
    tool.status = ToolStatus::Completed(5000);

    match tool.status {
        ToolStatus::Completed(ms) => {
            assert_eq!(ms, 5000);
            assert!(ms > 1000); // More than 1 second
        }
        _ => panic!("Expected Completed status"),
    }

    // Multiple tools with different durations
    let mut fast_tool = create_test_tool("fast_1", "Read");
    fast_tool.status = ToolStatus::Completed(50);

    let mut slow_tool = create_test_tool("slow_2", "Bash");
    slow_tool.status = ToolStatus::Completed(10000);

    let fast_ms = match fast_tool.status {
        ToolStatus::Completed(ms) => ms,
        _ => panic!("Expected Completed"),
    };
    let slow_ms = match slow_tool.status {
        ToolStatus::Completed(ms) => ms,
        _ => panic!("Expected Completed"),
    };

    assert!(fast_ms < slow_ms);
    assert_eq!(fast_ms, 50);
    assert_eq!(slow_ms, 10000);
}

// Test scenario 15: Loop can detect when all subagent tools complete
#[test]
fn loop_detects_all_subagent_tools_complete() {
    let mut loop_state = MainLoopState {
        loop_id: "loop_10".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: Vec::new(),
    };

    // Subagent1 tools (both completed)
    let mut sub1_tool1 = create_test_tool("sub1_read", "Read");
    sub1_tool1.parent_agent_id = Some("subagent1".to_string());
    sub1_tool1.status = ToolStatus::Completed(100);

    let mut sub1_tool2 = create_test_tool("sub1_bash", "Bash");
    sub1_tool2.parent_agent_id = Some("subagent1".to_string());
    sub1_tool2.status = ToolStatus::Completed(200);

    // Subagent2 tools (both completed)
    let mut sub2_tool1 = create_test_tool("sub2_grep", "Grep");
    sub2_tool1.parent_agent_id = Some("subagent2".to_string());
    sub2_tool1.status = ToolStatus::Completed(150);

    let mut sub2_tool2 = create_test_tool("sub2_edit", "Edit");
    sub2_tool2.parent_agent_id = Some("subagent2".to_string());
    sub2_tool2.status = ToolStatus::Completed(120);

    loop_state.tools.push(sub1_tool1);
    loop_state.tools.push(sub1_tool2);
    loop_state.tools.push(sub2_tool1);
    loop_state.tools.push(sub2_tool2);

    // All tools complete, loop can transition to idle
    let all_done = loop_state
        .tools
        .iter()
        .all(|t| !matches!(t.status, ToolStatus::Running));
    assert!(all_done);

    // Can transition to idle
    loop_state.status = MainLoopStatus::Idle;
    assert!(matches!(loop_state.status, MainLoopStatus::Idle));
}

// Test scenario 16: /clear command resets current_main_loop to None
#[test]
fn clear_command_resets_current_main_loop() {
    // Setup: Create a main loop with active tools
    let mut loop_state = Some(MainLoopState {
        loop_id: "loop_clear_1".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![
            create_test_tool("tool_1", "Read"),
            create_test_tool("tool_2", "Bash"),
            create_test_tool("tool_3", "Edit"),
        ],
    });

    // Verify loop exists and has tools before clear
    assert!(loop_state.is_some());
    assert_eq!(loop_state.as_ref().unwrap().tools.len(), 3);
    assert!(matches!(
        loop_state.as_ref().unwrap().status,
        MainLoopStatus::Active
    ));

    // Simulate /clear: set loop_state to None
    loop_state = None;

    // Verify loop is cleared
    assert!(loop_state.is_none());
}

// Test scenario 17: /clear command clears active_agents HashSet
#[test]
fn clear_command_clears_active_agents() {
    // Setup: Create a set of active agents
    let mut active_agents = std::collections::HashSet::new();
    active_agents.insert("planner".to_string());
    active_agents.insert("code_reviewer".to_string());
    active_agents.insert("security_reviewer".to_string());

    // Verify agents are present
    assert_eq!(active_agents.len(), 3);
    assert!(active_agents.contains("planner"));
    assert!(active_agents.contains("code_reviewer"));
    assert!(active_agents.contains("security_reviewer"));

    // Simulate /clear: clear active_agents
    active_agents.clear();

    // Verify active_agents is empty
    assert_eq!(active_agents.len(), 0);
    assert!(!active_agents.contains("planner"));
    assert!(!active_agents.contains("code_reviewer"));
    assert!(!active_agents.contains("security_reviewer"));
}

// Test scenario 18: /clear fully resets main loop and agent state
#[test]
fn clear_fully_resets_main_loop_and_agent_state() {
    // Setup: Complex state with active loop and multiple agents
    let mut current_main_loop = Some(MainLoopState {
        loop_id: "loop_clear_2".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![
            {
                let mut tool = create_test_tool("agent_tool_1", "AgentTool");
                tool.parent_agent_id = Some("planner".to_string());
                tool.status = ToolStatus::Running;
                tool
            },
            {
                let mut tool = create_test_tool("read_1", "Read");
                tool.parent_agent_id = Some("code_reviewer".to_string());
                tool.status = ToolStatus::Completed(200);
                tool
            },
            {
                let mut tool = create_test_tool("bash_1", "Bash");
                tool.status = ToolStatus::Running;
                tool
            },
        ],
    });

    let mut active_agents = std::collections::HashSet::new();
    active_agents.insert("planner".to_string());
    active_agents.insert("code_reviewer".to_string());

    // Verify initial state
    assert!(current_main_loop.is_some());
    assert_eq!(current_main_loop.as_ref().unwrap().tools.len(), 3);
    assert_eq!(active_agents.len(), 2);

    // Verify there are running tools
    let running_tools = current_main_loop
        .as_ref()
        .unwrap()
        .tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Running))
        .count();
    assert_eq!(running_tools, 2);

    // Simulate /clear: reset both parts of state
    current_main_loop = None;
    active_agents.clear();

    // Verify both parts are reset
    assert!(current_main_loop.is_none());
    assert_eq!(active_agents.len(), 0);
}

// Test scenario 19: /clear after single tool completes
#[test]
fn clear_after_single_tool_completes() {
    // Setup: Simple completed loop
    let mut current_main_loop = Some(MainLoopState {
        loop_id: "loop_clear_3".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Idle,
        tools: vec![{
            let mut tool = create_test_tool("final_tool", "Read");
            tool.status = ToolStatus::Completed(1500);
            tool.result = Some(ToolResult {
                content: "Completed successfully".to_string(),
                full_content_available: true,
            });
            tool
        }],
    });

    let active_agents: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Verify completed state
    assert!(current_main_loop.is_some());
    assert_eq!(current_main_loop.as_ref().unwrap().tools.len(), 1);
    assert!(matches!(
        current_main_loop.as_ref().unwrap().status,
        MainLoopStatus::Idle
    ));
    assert!(active_agents.is_empty());

    // Simulate /clear
    current_main_loop = None;

    // Verify cleared
    assert!(current_main_loop.is_none());
}

// Test scenario 20: /clear idempotency (clearing already-empty state)
#[test]
fn clear_is_idempotent() {
    // Setup: Already-cleared state
    let mut current_main_loop: Option<MainLoopState> = None;
    let mut active_agents: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Verify initial state is already clear
    assert!(current_main_loop.is_none());
    assert!(active_agents.is_empty());

    // Simulate /clear on already-clear state
    current_main_loop = None;
    active_agents.clear();

    // Verify still clear (idempotent)
    assert!(current_main_loop.is_none());
    assert!(active_agents.is_empty());
}

// Test scenario 21: /clear with mixed tool states
#[test]
fn clear_with_mixed_tool_states() {
    // Setup: Mix of running, completed, and failed tools
    let mut current_main_loop = Some(MainLoopState {
        loop_id: "loop_clear_4".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![
            {
                let mut tool = create_test_tool("running_tool", "Bash");
                tool.status = ToolStatus::Running;
                tool
            },
            {
                let mut tool = create_test_tool("completed_tool", "Read");
                tool.status = ToolStatus::Completed(100);
                tool.result = Some(ToolResult {
                    content: "Done".to_string(),
                    full_content_available: true,
                });
                tool
            },
            {
                let mut tool = create_test_tool("failed_tool", "Edit");
                tool.status = ToolStatus::Failed("Error occurred".to_string());
                tool.result = Some(ToolResult {
                    content: "Error details".to_string(),
                    full_content_available: true,
                });
                tool
            },
        ],
    });

    let mut active_agents = std::collections::HashSet::new();
    active_agents.insert("executor".to_string());

    // Verify complex state
    assert_eq!(current_main_loop.as_ref().unwrap().tools.len(), 3);
    assert_eq!(active_agents.len(), 1);

    let tool_states: Vec<&str> = current_main_loop
        .as_ref()
        .unwrap()
        .tools
        .iter()
        .map(|t| match &t.status {
            ToolStatus::Running => "running",
            ToolStatus::Completed(_) => "completed",
            ToolStatus::Failed(_) => "failed",
        })
        .collect();
    assert_eq!(tool_states, vec!["running", "completed", "failed"]);

    // Simulate /clear
    current_main_loop = None;
    active_agents.clear();

    // Verify all state cleared regardless of mixed states
    assert!(current_main_loop.is_none());
    assert!(active_agents.is_empty());
}
