//! Execution context — metadata attached to each message during execution.

/// Context carried through the execution pipeline.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub worker_id: usize,
}

impl ExecutionContext {
    pub fn new(topic: String, partition: i32, offset: i64, worker_id: usize) -> Self {
        Self { topic, partition, offset, worker_id }
    }
}
