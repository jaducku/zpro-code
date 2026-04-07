use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

pub type HookEnv = HashMap<String, String>;

#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug)]
pub enum CommandHookResult {
    Success { output: CommandOutput },
    Blocking { reason: String, output: CommandOutput },
    NonBlockingError { message: String },
}

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const DEFAULT_SHELL: &str = "sh";

pub async fn execute_command_hook(
    command: &str,
    shell: Option<&str>,
    env: &HookEnv,
    cwd: &Path,
    timeout_secs: Option<u64>,
) -> CommandHookResult {
    let shell = shell.unwrap_or(DEFAULT_SHELL);
    let duration = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let child = Command::new(shell)
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .envs(env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let child = match child {
        Ok(c) => c,
        Err(e) => {
            return CommandHookResult::NonBlockingError {
                message: format!("Failed to spawn hook command: {}", e),
            };
        }
    };

    let result = timeout(duration, child.wait_with_output()).await;

    match result {
        Err(_) => {
            // Timeout — child is consumed by wait_with_output so we can't kill it,
            // but the future is dropped which will drop the child process handle.
            CommandHookResult::NonBlockingError {
                message: format!(
                    "Hook command timed out after {} seconds",
                    duration.as_secs()
                ),
            }
        }
        Ok(Err(e)) => CommandHookResult::NonBlockingError {
            message: format!("Hook command failed: {}", e),
        },
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            let cmd_output = CommandOutput {
                stdout,
                stderr,
                exit_code,
            };

            match exit_code {
                0 => CommandHookResult::Success { output: cmd_output },
                2 => {
                    let reason = if cmd_output.stderr.trim().is_empty() {
                        format!("Hook command exited with code 2")
                    } else {
                        cmd_output.stderr.trim().to_string()
                    };
                    CommandHookResult::Blocking {
                        reason,
                        output: cmd_output,
                    }
                }
                _ => {
                    let message = if cmd_output.stderr.trim().is_empty() {
                        format!("Hook command exited with code {}", exit_code)
                    } else {
                        cmd_output.stderr.trim().to_string()
                    };
                    CommandHookResult::NonBlockingError { message }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn empty_env() -> HookEnv {
        HashMap::new()
    }

    #[tokio::test]
    async fn test_execute_command_hook_success() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("true", None, &empty_env(), dir.path(), None).await;
        match result {
            CommandHookResult::Success { output } => {
                assert_eq!(output.exit_code, 0);
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_1_nonblocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("false", None, &empty_env(), dir.path(), None).await;
        match result {
            CommandHookResult::NonBlockingError { .. } => {}
            other => panic!("Expected NonBlockingError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_code_2_blocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("exit 2", None, &empty_env(), dir.path(), None).await;
        match result {
            CommandHookResult::Blocking { .. } => {}
            other => panic!("Expected Blocking, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_code_3_nonblocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("exit 3", None, &empty_env(), dir.path(), None).await;
        match result {
            CommandHookResult::NonBlockingError { .. } => {}
            other => panic!("Expected NonBlockingError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_json_output() {
        let dir = TempDir::new().unwrap();
        let cmd = r#"echo '{"decision":"block","reason":"policy"}'"#;
        let result = execute_command_hook(cmd, None, &empty_env(), dir.path(), None).await;
        match result {
            CommandHookResult::Success { output } => {
                assert!(output.stdout.contains(r#""decision":"block"#));
                assert!(output.stdout.contains(r#""reason":"policy"#));
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_timeout() {
        let dir = TempDir::new().unwrap();
        let result =
            execute_command_hook("sleep 60", None, &empty_env(), dir.path(), Some(1)).await;
        match result {
            CommandHookResult::NonBlockingError { message } => {
                assert!(message.contains("timed out"), "message was: {}", message);
            }
            other => panic!("Expected NonBlockingError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_env_vars() {
        let dir = TempDir::new().unwrap();
        let mut env = HookEnv::new();
        env.insert("ATOM_TOOL_NAME".to_string(), "Bash".to_string());
        let result =
            execute_command_hook("echo $ATOM_TOOL_NAME", None, &env, dir.path(), None).await;
        match result {
            CommandHookResult::Success { output } => {
                assert!(
                    output.stdout.contains("Bash"),
                    "stdout was: {}",
                    output.stdout
                );
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }
}
