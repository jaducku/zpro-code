// Agent orchestration system
//
// Supports both builtin agents (hardcoded) and custom agents (loaded from files).
// Custom agents can come from plugins, user settings, or project configs.
//
// Model resolution priority:
//   1. config.builtin_models.{name} override
//   2. Code default (Explore: haiku, others: None/inherit)
//   3. None (inherit from parent at runtime)

pub mod types;
pub mod registry;
pub mod builtins;
pub mod loader;

pub use types::{AgentDefinition, AgentSource, AgentToolFilter};
pub use registry::AgentRegistry;
pub use builtins::{builtin_agents, BuiltinModels};
pub use loader::{parse_agent_md, load_agents};
