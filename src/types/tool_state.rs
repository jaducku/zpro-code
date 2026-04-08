// src/types/tool_state.rs
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Tool call execution state (real-time tracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub tool_use_id: String,
    pub parent_agent_id: Option<String>,    // Which agent called this tool
    pub parent_tool_use_id: Option<String>, // For nested tool relationships
    pub tool_name: String,
    pub status: ToolStatus,
    pub created_at: SystemTime,
    pub params: Option<serde_json::Value>,  // Set on InputReady
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
    Idle,        // All agents idle, no pending tools
    AwaitingHitl, // HITL input required
}
