// Agent orchestration system
//
// Supports both builtin agents (hardcoded) and custom agents (loaded from files).
// Custom agents can come from plugins, user settings, or project configs.
//
// Model resolution priority:
//   1. config.builtin_models.{name} override
//   2. Code default (Explore: haiku, others: None/inherit)
//   3. None (inherit from parent at runtime)

pub mod builtins;
pub mod loader;
pub mod registry;
pub mod types;

pub use builtins::{builtin_agents, BuiltinModels};
pub use loader::{load_agents, parse_agent_md};
pub use registry::AgentRegistry;
pub use types::{AgentDefinition, AgentSource, AgentToolFilter};
