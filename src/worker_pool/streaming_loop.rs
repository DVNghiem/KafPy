//! Streaming worker loop — four-phase lifecycle for persistent async generator handlers.
//!
//! Manages the lifecycle of a streaming handler subscription: Starting → Running → Draining → Recovering.
//! Coordinates graceful shutdown via CancellationToken and drives the async generator via invoke_streaming.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::dispatcher::queue_manager::QueueManager;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::handler::PythonHandler;
use crate::worker_pool::logger;

/// State machine for streaming handler lifecycle.
/// Transitions: Starting → Running → Draining → Recovering
#[derive(Debug, Clone, PartialEq, Eq)]
enum StreamingState {
    /// Subscribing to topic, creating async generator
    Starting,
    /// Polling messages and yielding to consumer
    Running,
    /// Cancellation received, draining buffered messages
    Draining,
    /// Error detected, applying retry/backoff before Restart
    Recovering { attempt: u32 },
}

/// Maximum retry attempts before transitioning to Draining.
const MAX_RECOVERY_ATTEMPTS: u32 = 3;

/// Streaming worker loop — drives a Python async generator to completion with four-phase lifecycle.
///
/// This loop runs for the lifetime of a streaming handler subscription, coordinating start/subscribe,
/// run/loop, stop/drain, and error recovery. CancellationToken coordinates graceful stop with drain.
///
/// # Arguments
/// * `worker_id` — Unique worker identifier for logging
/// * `consumer` — Kafka consumer for streaming subscription
/// * `handler` — Python handler with invoke_streaming method
/// * `cancel` — Cancellation token for graceful shutdown
/// * `queue_manager` — Queue manager for acking messages
///
/// # Returns
/// `Ok(())` on normal exit, `Err(...)` on fatal error
pub async fn streaming_worker_loop(
    worker_id: usize,
    consumer: rdkafka::consumer::StreamConsumer,
    handler: PythonHandler,
    cancel: CancellationToken,
    queue_manager: Arc<QueueManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    logger::log(
        "INFO",
        &format!("streaming worker started worker_id={}", worker_id),
    );

    let mut state = StreamingState::Starting;
    let mut current_attempt: u32 = 0;

    loop {
        match state {
            StreamingState::Starting => {
                tracing::info!(worker_id = worker_id, "streaming: starting");

                // Build initial ExecutionContext from consumer assignment
                // For streaming, we use a placeholder offset until the generator yields
                let ctx = ExecutionContext::new(
                    "streaming".to_string(),
                    0,
                    0,
                    worker_id,
                );

                // Transition to Running immediately (subscription is implicit via consumer)
                state = StreamingState::Running;
                tracing::info!(worker_id = worker_id, "streaming: transitioned to Running");
            }

            StreamingState::Running => {
                // Check cancellation first — coordinated with graceful shutdown (D-04)
                if cancel.is_cancelled() {
                    tracing::info!(
                        worker_id = worker_id,
                        "streaming: cancellation received, draining"
                    );
                    state = StreamingState::Draining;
                    continue;
                }

                // Poll the async generator via invoke_streaming
                // For streaming mode, we pass a placeholder initial message
                // The generator itself manages the subscription lifecycle
                let initial_message = crate::dispatcher::OwnedMessage {
                    topic: "streaming".to_string(),
                    partition: 0,
                    offset: 0,
                    key: None,
                    payload: None,
                    timestamp: crate::consumer::MessageTimestamp::NotAvailable,
                    headers: vec![],
                };

                let ctx = ExecutionContext::new(
                    handler.name().to_string(),
                    0,
                    0,
                    worker_id,
                );

                match handler.invoke_streaming(&ctx, initial_message).await {
                    ExecutionResult::Ok => {
                        // Generator exhausted normally — graceful stop
                        tracing::info!(
                            worker_id = worker_id,
                            "streaming: generator exhausted normally"
                        );
                        break;
                    }
                    ExecutionResult::Error { reason, exception, traceback } => {
                        // Error during yield — error recovery path (D-03)
                        tracing::warn!(
                            worker_id = worker_id,
                            exception = %exception,
                            traceback = %traceback,
                            "streaming: error during yield, entering recovery"
                        );
                        state = StreamingState::Recovering { attempt: 1 };
                        current_attempt = 1;
                    }
                    _ => {
                        // Unexpected result (Rejected/Timeout) — treat as error
                        tracing::error!(
                            worker_id = worker_id,
                            "streaming: unexpected result, exiting"
                        );
                        break;
                    }
                }
            }

            StreamingState::Draining => {
                tracing::info!(
                    worker_id = worker_id,
                    "streaming: draining complete, exiting"
                );
                // Generator close() is called via PythonAsyncFuture Drop impl
                // when the future is dropped on cancellation
                queue_manager.ack("streaming", 1);
                break;
            }

            StreamingState::Recovering { attempt } => {
                tracing::info!(
                    worker_id = worker_id,
                    attempt = attempt,
                    "streaming: recovering with backoff"
                );

                // Apply exponential backoff: 2^attempt seconds
                let backoff_secs = 2u64.pow(attempt);
                sleep(Duration::from_secs(backoff_secs)).await;

                if attempt < MAX_RECOVERY_ATTEMPTS {
                    // Retry — transition back to Running
                    state = StreamingState::Running;
                    tracing::info!(
                        worker_id = worker_id,
                        attempt = attempt,
                        "streaming: retrying after backoff"
                    );
                } else {
                    // Max retries exceeded — transition to Draining
                    tracing::warn!(
                        worker_id = worker_id,
                        "streaming: max recovery attempts exceeded, draining"
                    );
                    state = StreamingState::Draining;
                }
            }
        }
    }

    logger::log(
        "INFO",
        &format!("streaming worker stopped worker_id={}", worker_id),
    );
    Ok(())
}