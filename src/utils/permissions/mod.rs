pub mod auto_classify;
pub mod checker;

pub use auto_classify::auto_classify;
pub use checker::check_permission;

/// Result of a permission check
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Allow { reason: String },
    Ask { tool_name: String, description: String },
    Deny { reason: String },
}
