//! Async task scheduler with a cooperative shutdown token.
//!
//! Tasks are owned by a [`TaskScheduler`]'s `JoinSet`; dropping the scheduler
//! aborts them, and [`TaskScheduler::shutdown`] cancels the token first so
//! cooperative tasks can finish within the deadline. Any task still running at
//! the deadline is aborted and joined, so no task is left detached.

use std::time::Duration;

use tokio::sync::watch;
use tokio::task::{AbortHandle, Id, JoinSet};
use tokio::time::{timeout, Instant};

/// Cloneable cancellation signal shared by all runtime tasks.
#[derive(Clone)]
pub struct ShutdownToken {
    sender: watch::Sender<bool>,
}

impl ShutdownToken {
    /// Create a fresh, uncancelled token.
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self { sender }
    }

    /// Request shutdown. Repeated calls are harmless.
    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    /// Whether shutdown was requested.
    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    /// Resolve when [`Self::cancel`] is called. Safe to await after a cancel.
    pub async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        // `changed` only returns an error when the sender is dropped; the token
        // itself owns the sender, so that cannot happen while this future runs.
        let _ = receiver.changed().await;
    }
}

impl Default for ShutdownToken {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShutdownToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShutdownToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// A handle to one spawned task.
#[derive(Debug, Clone)]
pub struct TaskHandle {
    id: Id,
    name: String,
    abort: AbortHandle,
}

impl TaskHandle {
    /// Stable Tokio task id.
    pub fn id(&self) -> Id {
        self.id
    }

    /// Human-readable task name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Abort the task without waiting. Cooperative tasks should use the
    /// shutdown token instead.
    pub fn abort(&self) {
        self.abort.abort();
    }
}

/// Result of a graceful shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Tasks that finished successfully before the deadline.
    pub completed: usize,
    /// Tasks that observed cancellation and returned successfully.
    pub cancelled: usize,
    /// Tasks still running at the deadline and therefore force-aborted.
    pub aborted: usize,
    /// Tasks that panicked or were aborted externally.
    pub failed: usize,
}

/// Owns a set of named async tasks and one shared shutdown token.
pub struct TaskScheduler {
    token: ShutdownToken,
    tasks: JoinSet<()>,
}

impl TaskScheduler {
    /// Create a scheduler with a new shutdown token.
    pub fn new() -> Self {
        Self::with_token(ShutdownToken::new())
    }

    /// Create a scheduler that uses an existing token.
    pub fn with_token(token: ShutdownToken) -> Self {
        Self {
            token,
            tasks: JoinSet::new(),
        }
    }

    /// The shared cancellation token.
    pub fn token(&self) -> ShutdownToken {
        self.token.clone()
    }

    /// Spawn a task owned by this scheduler.
    pub fn spawn<F>(&mut self, name: impl Into<String>, future: F) -> TaskHandle
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let name = name.into();
        let abort = self.tasks.spawn(future);
        let id = abort.id();
        TaskHandle { id, name, abort }
    }

    /// Number of tasks that have not finished yet.
    pub fn pending(&self) -> usize {
        self.tasks.len()
    }

    /// Cancel the shared token and wait for every task until `deadline`, then
    /// abort and join the remainder.
    pub async fn shutdown(mut self, deadline: Duration) -> ShutdownReport {
        self.token.cancel();
        let deadline = Instant::now() + deadline;
        let mut report = ShutdownReport::default();

        loop {
            if self.tasks.is_empty() {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match timeout(remaining, self.tasks.join_next_with_id()).await {
                Ok(Some(Ok((_id, ())))) => {
                    if self.token.is_cancelled() {
                        report.cancelled += 1;
                    } else {
                        report.completed += 1;
                    }
                }
                Ok(Some(Err(_error))) => {
                    report.failed += 1;
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        report.aborted = self.tasks.len();
        if !self.tasks.is_empty() {
            self.tasks.abort_all();
            while self.tasks.join_next().await.is_some() {}
        }
        report
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TaskScheduler {
    fn drop(&mut self) {
        // A scheduler dropped without `shutdown` still cancels cooperative
        // tasks and aborts the rest; `JoinSet` is emptied on drop.
        self.token.cancel();
        self.tasks.abort_all();
    }
}

impl std::fmt::Debug for TaskScheduler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TaskScheduler")
            .field("pending", &self.tasks.len())
            .field("cancelled", &self.token.is_cancelled())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[test]
    fn token_cancel_is_visible_to_clones() {
        let token = ShutdownToken::new();
        let clone = token.clone();
        assert!(!token.is_cancelled());
        clone.cancel();
        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_resolves_after_prior_cancel() {
        let token = ShutdownToken::new();
        token.cancel();
        tokio::time::timeout(Duration::from_secs(1), token.cancelled())
            .await
            .expect("cancelled future resolves");
    }

    #[tokio::test]
    async fn shutdown_waits_for_cooperative_tasks() {
        let mut scheduler = TaskScheduler::new();
        let token = scheduler.token();
        let (sent, received) = oneshot::channel();
        let handle = scheduler.spawn("worker", async move {
            token.cancelled().await;
            let _ = sent.send(());
        });
        assert_eq!(handle.name(), "worker");
        assert_eq!(scheduler.pending(), 1);

        let report = scheduler.shutdown(Duration::from_secs(1)).await;
        assert_eq!(report.completed, 0);
        assert_eq!(report.cancelled, 1);
        assert_eq!(report.aborted, 0);
        assert_eq!(report.failed, 0);
        received.await.expect("worker observed cancellation");
    }

    #[tokio::test]
    async fn shutdown_aborts_tasks_at_the_deadline() {
        let mut scheduler = TaskScheduler::new();
        scheduler.spawn("stuck", async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        let report = scheduler.shutdown(Duration::from_millis(20)).await;
        assert_eq!(report.aborted, 1);
        assert_eq!(report.completed, 0);
        assert_eq!(report.cancelled, 0);
    }

    #[tokio::test]
    async fn externally_aborted_tasks_are_reported_as_failed() {
        let mut scheduler = TaskScheduler::new();
        let handle = scheduler.spawn("aborted", async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        handle.abort();
        let report = scheduler.shutdown(Duration::from_secs(1)).await;
        assert_eq!(report.failed, 1);
    }

    #[tokio::test]
    async fn dropping_scheduler_cancels_the_token_and_aborts_tasks() {
        let mut scheduler = TaskScheduler::new();
        let token = scheduler.token();
        scheduler.spawn("stuck", async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        drop(scheduler);
        assert!(token.is_cancelled());
    }
}
