use std::sync::{Arc, OnceLock};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortReason {
    /// Escape key — cancel current query, all tools immediately cancelled
    UserCancel,
    /// New message queued while tools running — tools check interruptBehavior
    Interrupt,
    /// Foreground task moved to background
    Background,
    /// Bash tool error propagated to sibling parallel tools
    SiblingError,
}

#[derive(Clone, Debug)]
pub struct AbortSignal {
    token: CancellationToken,
    reason: Arc<OnceLock<AbortReason>>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            reason: Arc::new(OnceLock::new()),
        }
    }

    pub fn cancel(&self, reason: AbortReason) {
        self.reason.set(reason).ok();
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await
    }

    pub fn reason(&self) -> Option<&AbortReason> {
        self.reason.get()
    }

    pub fn child_signal(&self) -> AbortSignal {
        AbortSignal {
            token: self.token.child_token(),
            reason: self.reason.clone(),
        }
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_signal_not_cancelled() {
        let signal = AbortSignal::new();
        assert!(!signal.is_cancelled());
        assert!(signal.reason().is_none());
    }

    #[test]
    fn test_cancel_sets_reason() {
        let signal = AbortSignal::new();
        signal.cancel(AbortReason::UserCancel);
        assert!(signal.is_cancelled());
        assert_eq!(signal.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_first_cancel_wins() {
        let signal = AbortSignal::new();
        signal.cancel(AbortReason::UserCancel);
        signal.cancel(AbortReason::Interrupt);
        assert_eq!(signal.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_child_cancelled_by_parent() {
        let parent = AbortSignal::new();
        let child = parent.child_signal();
        parent.cancel(AbortReason::UserCancel);
        assert!(child.is_cancelled());
        assert_eq!(child.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_child_cancel_does_not_affect_parent() {
        let parent = AbortSignal::new();
        let child = parent.child_signal();
        child.cancel(AbortReason::SiblingError);
        assert!(child.is_cancelled());
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancelled_future_resolves() {
        let signal = AbortSignal::new();
        let signal2 = signal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            signal2.cancel(AbortReason::UserCancel);
        });
        signal.cancelled().await;
        assert!(signal.is_cancelled());
    }
}
