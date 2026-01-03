use pyo3::prelude::*;

mod config;
mod consume;
mod errors;
mod kafka_message;
mod logging;
mod message_processor;

use consume::PyConsumer;
use kafka_message::KafkaMessage;
use logging::Logger;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize tracing
    Logger::init();

    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<config::KafkaConfig>()?;
    m.add_class::<config::AppConfig>()?;
    Ok(())
}
