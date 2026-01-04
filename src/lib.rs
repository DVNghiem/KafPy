use pyo3::prelude::*;

mod config;
mod consume;
mod errors;
mod kafka_message;
mod logging;
mod message_processor;
mod produce;

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
