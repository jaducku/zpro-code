use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::jsonrpc::{JsonRpcMessage, JsonRpcRequest};
use super::types::McpServerConfig;
use crate::types::Result;

/// Transport abstraction for MCP communication
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<()>;
    async fn receive(&mut self) -> Result<JsonRpcMessage>;
    async fn close(&mut self) -> Result<()>;
}

/// Stdio transport — communicates via child process stdin/stdout
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl StdioTransport {
    pub async fn spawn(config: &McpServerConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| crate::types::AtomError::Mcp {
            server_name: config.command.clone(),
            message: format!("Failed to spawn: {}", e),
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| crate::types::AtomError::Mcp {
                server_name: config.command.clone(),
                message: "Failed to capture stdin".into(),
            })?;

        let stdout =
            child
                .stdout
                .take()
                .ok_or_else(|| crate::types::AtomError::Mcp {
                    server_name: config.command.clone(),
                    message: "Failed to capture stdout".into(),
                })?;

        Ok(Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<()> {
        let json = serde_json::to_string(request)?;
        self.stdin
            .write_all(json.as_bytes())
            .await
            .map_err(crate::types::AtomError::Io)?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(crate::types::AtomError::Io)?;
        self.stdin
            .flush()
            .await
            .map_err(crate::types::AtomError::Io)?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<JsonRpcMessage> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .await
            .map_err(crate::types::AtomError::Io)?;

        if line.is_empty() {
            return Err(crate::types::AtomError::Mcp {
                server_name: "stdio".into(),
                message: "Connection closed".into(),
            });
        }

        let msg: JsonRpcMessage = serde_json::from_str(line.trim())?;
        Ok(msg)
    }

    async fn close(&mut self) -> Result<()> {
        let _ = self.child.kill().await;
        Ok(())
    }
}
