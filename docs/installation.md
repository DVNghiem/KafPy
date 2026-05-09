# Installation

## Requirements

- Python 3.11 or later
- Apache Kafka broker (any version compatible with Kafka protocol 0.11+)
- `librdkafka` (system library, see platform-specific instructions below)

## Install librdkafka

KafPy uses `librdkafka` via PyO3/Rust. Install the development package for your platform before installing KafPy.

```bash
# macOS (Homebrew)
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora / Red Hat
dnf install librdkafka-devel

# Alpine
apk add librdkafka-dev
```

## Install KafPy

### From PyPI

```bash
pip install kafpy
```

### From source

```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
pip install maturin
maturin develop --release
```

## Verify installation

```python
import kafpy

print(kafpy.__version__)
```

Expected output:

```
0.1.0
```

## Optional dependencies

KafPy has no required external dependencies beyond `librdkafka`. Optional integrations include:

- `uvicorn` — for running the consumer as an async server
- `prometheus-client` — for Prometheus metrics export
- `opentelemetry-api`, `opentelemetry-sdk` — for OTLP tracing

Install optional dependencies as needed with `pip install`.
