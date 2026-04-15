use pyo3::prelude::*;

pub mod config;
pub mod consume;
pub mod errors;
pub mod kafka_message;
pub mod logging;
pub mod message_processor;
pub mod produce;

// Pure Rust consumer core — no PyO3 dependencies
pub mod consumer;

use consume::PyConsumer;
use kafka_message::KafkaMessage;
use logging::Logger;
use produce::PyProducer;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize tracing
    Logger::init();

    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    Ok(())
}
