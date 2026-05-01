//! Fan-in worker loop — merges messages from multiple topic streams via round-robin tokio::select!.
//!
//! For each topic source, spawns a background task that receives messages from that
//! topic's Kafka partition and forwards them into a shared mpsc channel.
//! The main loop then uses `tokio::select! biased` to interleave messages by arrival order.

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::dispatcher::OwnedMessage;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::handler::PythonHandler;
use crate::worker_pool::handle_execution_failure;
use crate::worker_pool::ExecutionAction;
use crate::coordinator::RetryCoordinator;
use crate::dlq::{DlqRouter, SharedDlqProducer};
use crate::dispatcher::queue_manager::QueueManager;
use crate::python::logger;

/// Fan-in worker loop — merges messages from multiple topic streams.
///
/// For each topic source, spawns a background task that receives messages from that
/// topic's Kafka partition and forwards them into a shared mpsc channel.
/// The main loop then uses `tokio::select! biased` to interleave messages by arrival order.
///
/// # Arguments
/// * `worker_id` — Unique worker identifier for logging
/// * `sources` — Vec of (topic, Kafka partition, message_stream) for each source
/// * `handler` — Python handler to invoke for each merged message
/// * `fan_in_id` — Fan-in group ID for ExecutionContext
/// * `queue_manager` — Queue manager for acking messages
/// * `cancel` — Cancellation token for graceful shutdown
pub async fn fan_in_worker_loop(
    worker_id: usize,
    sources: Vec<(String, i32, Box<dyn tokio_stream::Stream<Item = OwnedMessage> + Send + std::marker::Unpin>)>,
    handler: Arc<PythonHandler>,
    fan_in_id: u64,
    queue_manager: Arc<QueueManager>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    cancel: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    logger::log(
        "INFO",
        &format!(
            "fan-in worker started worker_id={} sources={} fan_in_id={}",
            worker_id,
            sources.len(),
            fan_in_id
        ),
    );

    // For each source, create a shared channel and spawn a forwarder task.
    let mut source_channels: Vec<(String, mpsc::Receiver<(OwnedMessage, String)>)> = Vec::new();

    for (topic, _partition, stream) in sources {
        let (tx, rx) = mpsc::channel(100);
        source_channels.push((topic.clone(), rx));

        let topic_clone = topic.clone();
        tokio::spawn(async move {
            let mut stream = stream;
            use tokio_stream::StreamExt;
            while let Some(msg) = stream.next().await {
                if tx.send((msg, topic_clone.clone())).await.is_err() {
                    break;
                }
            }
        });
    }

    // Main merge loop — biased select fair polling across all sources.
    loop {
        // Check cancellation first.
        if cancel.is_cancelled() {
            tracing::info!(worker_id = worker_id, "fan-in worker: cancellation received");
            break;
        }

        // Poll each source channel by iterating index — biased ensures fair rotation.
        // try_recv is non-blocking — if a channel is empty, skip to next.
        let mut made_progress = false;
        let mut idx = 0;
        while idx < source_channels.len() {
            let (_topic, rx) = &mut source_channels[idx];
            if let Ok((msg, source_topic)) = rx.try_recv() {
                made_progress = true;

                tracing::trace!(
                    worker_id = worker_id,
                    source_topic = %source_topic,
                    topic = %msg.topic,
                    partition = msg.partition,
                    offset = msg.offset,
                    "fan-in worker: received message from source"
                );

                // Extract W3C trace context from message headers.
                let header_map: std::collections::HashMap<String, String> = msg
                    .headers
                    .iter()
                    .filter_map(|(k, v)| {
                        v.as_ref()
                            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                            .map(|val| (k.clone(), val))
                    })
                    .collect();
                let mut trace_map = std::collections::HashMap::new();
                crate::observability::tracing::inject_trace_context(&header_map, &mut trace_map);

                let trace_id = trace_map.get("trace_id").cloned();
                let span_id = trace_map.get("span_id").cloned();
                let trace_flags = trace_map.get("trace_flags").cloned();

                // Build ExecutionContext with trace fields.
                // Note: source_topic and fan_in_id are set via with_trace args since
                // with_trace doesn't have dedicated builder-method variants.
                let ctx = ExecutionContext::with_trace(
                    msg.topic.clone(),
                    msg.partition,
                    msg.offset,
                    worker_id,
                    trace_id.clone(),
                    span_id.clone(),
                    trace_flags.clone(),
                    None,                           // branch_id
                    None,                           // fan_out_id
                    source_topic.clone(),           // source_topic
                    Some(fan_in_id),                // fan_in_id
                );

                let start = std::time::Instant::now();
                let result = handler.invoke_mode_with_timeout(&ctx, msg.clone()).await;
                let elapsed = start.elapsed();

                logger::log("INFO", &format!(
                    "fan-in handler invoke complete worker_id={} source_topic={} topic={} partition={} offset={} elapsed_ms={}",
                    worker_id, source_topic, msg.topic, msg.partition, msg.offset, elapsed.as_millis() as u64
                ));

                match result {
                    ExecutionResult::Ok => {
                        tracing::debug!(
                            worker_id = worker_id,
                            source_topic = %source_topic,
                            topic = %msg.topic,
                            partition = msg.partition,
                            offset = msg.offset,
                            "fan-in handler executed successfully"
                        );
                        retry_coordinator.record_success(&msg.topic, msg.partition, msg.offset);
                        queue_manager.ack(&msg.topic, 1);
                    }
                    ExecutionResult::Error { ref reason, ref exception, .. } => {
                        tracing::warn!(
                            worker_id = worker_id,
                            source_topic = %source_topic,
                            topic = %msg.topic,
                            partition = msg.partition,
                            offset = msg.offset,
                            exception = %exception,
                            "fan-in handler raised exception"
                        );
                        crate::failure::logging::log_failure(&ctx, reason, exception, false);

                        let action = handle_execution_failure(
                            &ctx,
                            &msg,
                            &result,
                            Arc::clone(&retry_coordinator),
                            Arc::clone(&dlq_producer),
                            Arc::clone(&dlq_router),
                            Arc::clone(&queue_manager),
                        )
                        .await;

                        match action {
                            ExecutionAction::Ack | ExecutionAction::Dlq | ExecutionAction::Retry { .. } => {
                                // Fan-in retry: message is dropped. Retries are handled by
                                // the dispatcher's redelivery mechanism, not re-enqueue here.
                            }
                        }
                    }
                    ExecutionResult::Rejected { ref reason, .. } => {
                        tracing::warn!(
                            worker_id = worker_id,
                            source_topic = %source_topic,
                            topic = %msg.topic,
                            partition = msg.partition,
                            offset = msg.offset,
                            reason = %reason,
                            "fan-in handler rejected message"
                        );
                        let exc_name = "Rejected";
                        crate::failure::logging::log_failure(&ctx, reason, exc_name, false);

                        let action = handle_execution_failure(
                            &ctx,
                            &msg,
                            &result,
                            Arc::clone(&retry_coordinator),
                            Arc::clone(&dlq_producer),
                            Arc::clone(&dlq_router),
                            Arc::clone(&queue_manager),
                        )
                        .await;

                        match action {
                            ExecutionAction::Ack | ExecutionAction::Dlq | ExecutionAction::Retry { .. } => {}
                        }
                    }
                    ExecutionResult::Timeout { ref info } => {
                        tracing::warn!(
                            worker_id = worker_id,
                            source_topic = %source_topic,
                            topic = %msg.topic,
                            partition = msg.partition,
                            offset = msg.offset,
                            timeout_ms = info.timeout_ms,
                            "fan-in handler timed out"
                        );
                        let reason = crate::failure::FailureReason::Terminal(
                            crate::failure::TerminalKind::HandlerPanic,
                        );
                        crate::failure::logging::log_failure(&ctx, &reason, "HandlerTimeout", false);

                        let action = handle_execution_failure(
                            &ctx,
                            &msg,
                            &result,
                            Arc::clone(&retry_coordinator),
                            Arc::clone(&dlq_producer),
                            Arc::clone(&dlq_router),
                            Arc::clone(&queue_manager),
                        )
                        .await;

                        match action {
                            ExecutionAction::Ack | ExecutionAction::Dlq | ExecutionAction::Retry { .. } => {}
                        }
                    }
                }
            }
            idx += 1;
        }

        // If no progress was made, yield to avoid busy-waiting.
        if !made_progress {
            tokio::task::yield_now().await;
        }
    }

    logger::log(
        "INFO",
        &format!("fan-in worker stopped worker_id={}", worker_id),
    );
    Ok(())
}