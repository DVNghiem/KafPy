# Installation

## Prerequisites

- Python 3.11 or higher
- Rust toolchain (for building from source)
- librdkafka library

## Install librdkafka

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
sudo apt install librdkafka-dev

# Fedora / RHEL
sudo dnf install librdkafka-devel
```

## Install KafPy

### From PyPI (when available)

```bash
pip install kafpy
```

### Build from Source

```bash
# Install maturin
pip install maturin

# Clone and build
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
maturin develop --release
```

## Verify Installation

```python
import kafpy
print(kafpy.__version__)
```