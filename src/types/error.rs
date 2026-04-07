use thiserror::Error;

#[derive(Error, Debug)]
pub enum AtomError {
    #[error("API error: {message}")]
    Api {
        message: String,
        status: Option<u16>,
    },

    #[error("Tool error: {tool_name}: {message}")]
    Tool { tool_name: String, message: String },

    #[error("MCP error: {server_name}: {message}")]
    Mcp {
        server_name: String,
        message: String,
    },

    #[error("Permission denied: {tool_name}")]
    PermissionDenied { tool_name: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Token budget exceeded: {used}/{limit}")]
    TokenBudget { used: u64, limit: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Operation cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, AtomError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_display() {
        let err = AtomError::Api {
            message: "rate limited".into(),
            status: Some(429),
        };
        assert_eq!(err.to_string(), "API error: rate limited");
    }

    #[test]
    fn test_tool_error_display() {
        let err = AtomError::Tool {
            tool_name: "BashTool".into(),
            message: "command failed".into(),
        };
        assert_eq!(err.to_string(), "Tool error: BashTool: command failed");
    }

    #[test]
    fn test_token_budget_display() {
        let err = AtomError::TokenBudget {
            used: 200000,
            limit: 128000,
        };
        assert_eq!(err.to_string(), "Token budget exceeded: 200000/128000");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: AtomError = io_err.into();
        assert!(matches!(err, AtomError::Io(_)));
    }
}
