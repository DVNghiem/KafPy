# Phase 37: Documentation & Packaging - Research

**Researched:** 2026-04-20
**Domain:** Python package documentation, maturin/Rust-PyO3 packaging, docstring standards
**Confidence:** MEDIUM

## Summary

Phase 37 implements the final layer of KafPy's public API foundation: documentation and packaging. The core deliverables are (1) a cleaned-up README.md with a sub-30-line quickstart, (2) a `docs/` directory with guides and auto-generated API reference, (3) a `kafpy/guides/` Python package containing `.md` files, (4) full docstrings on all public classes/functions, and (5) a correctly configured `pyproject.toml` for maturin that passes `BUILD-01` through `BUILD-04`. Most of the infrastructure is already in place: the `pyproject.toml` uses maturin, `Cargo.toml` has `cdylib` + `rlib` and `package.metadata.maturin`, and the Python modules have `__all__` defined. The primary gaps are documentation content, the `kafpy/guides/` package, and minor `pyproject.toml` corrections to match the exact BUILD requirements.

**Primary recommendation:** Focus implementation on content creation (README rewrite, `.md` guides) and the new `kafpy/guides/` package structure. The maturin configuration is close to correct but needs two adjustments: the extension module name in `pyproject.toml` should match what Cargo.toml actually builds, and `project.dependencies` should note the rdkafka system library requirement.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| README.md | docs/ static | — | Top-level project documentation at repo root |
| docs/ guides | docs/ static | — | Auto-generated from docstrings via pdoc/mkdocs |
| kafpy/guides/ package | Python package | — | Runtime-accessible markdown via importlib.resources |
| Docstrings | Python modules | — | Inline API documentation in each module |
| pyproject.toml | project root | Cargo.toml | Build system configuration |
| maturin develop | dev environment | — | Local installation workflow |
| Extension module (_kafpy) | Rust/PyO3 | Cargo.toml | Rust library compiled to Python extension |

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **v1.8:** Instance-based config (no globals) -- all runtime state in instance objects
- **v1.8:** Decorator + explicit handler registration as primary Python API patterns
- **v1.8:** Rust internals private by default; only explicitly `pub` items accessible from Python
- **v1.8:** kafpy.exceptions as sole public import path for exceptions
- **v1.8:** Dual Consumer + KafPy wrapper: consumer = kafpy.Consumer(config) -> Rust Consumer; app = kafpy.KafPy(consumer) -> Python wrapper
- **v1.8:** HandlerContext/HandlerResult stub frozen dataclasses (full impl in Phase 36+)

### Claude's Discretion
- README structure and exact content (sections, length of each section)
- Docstring format choice (numpy vs google vs sphinx)
- kafpy/guides/ package internal structure
- Which docstring generation tool (pdoc, mkdocs, pydoc)
- Whether docs/ is auto-generated or hand-maintained

### Deferred Ideas (OUT OF SCOPE)
- Auto-generation pipeline from docstrings (DOC-02 mentions "auto-generated" but hand-written is acceptable for v1.8)
- API reference website with search (defer to v1.9)
- CHANGELOG management (defer to release process)

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-------------------|
| DOC-01 | README.md at repo root with install instructions, quickstart example (<30 lines), feature list | Section: README Structure |
| DOC-02 | docs/ directory with quickstart guide and API reference (auto-generated from docstrings) | Section: docs/ Directory |
| DOC-03 | kafpy/guides/ module as Python package with .md files: getting-started.md, handler-patterns.md, configuration.md, error-handling.md | Section: guides/ Package |
| DOC-04 | Every public class and function has a docstring: purpose, args, returns, raises | Section: Docstring Format |
| DOC-05 | Quickstart example demonstrates: install, config, handler decorator, app.run() | Section: Quickstart Example |
| DOC-06 | docs/guides/handler-patterns.md covers: sync handler, async handler, batch handler, error handling in handlers | Section: Handler Patterns Guide |
| BUILD-01 | pyproject.toml with build-system = { backend = "maturin" } and project.dependencies including rdkafka system library notes | Section: pyproject.toml Maturin Config |
| BUILD-02 | Cargo.toml in project root (for maturin) exposes kafpy-python as Python extension module name | Section: Cargo.toml Extension Module |
| BUILD-03 | kafpy/__init__.py does from .consumer import KafPy and from .config import * etc.; no star-import of internal modules | Section: Current State & Gaps |
| BUILD-04 | maturin develop works without additional configuration on a machine with librdkafka installed | Section: maturin develop Workflow |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| maturin | 1.12.4 | Build Rust-PyO3 extensions | Standard maturin packaging for PyO3 projects |
| pyo3 | 0.27.2 | Python-Rust bindings | Required for _kafpy extension |
| pyo3-async-runtimes | 0.27.0 | Async Python handler support | Enables async handler execution |
| rdkafka-sys | 0.38 | librdkafka Rust bindings | Kafka protocol implementation |

### Documentation
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| pdoc | latest | API reference generation from docstrings | For DOC-02 auto-generated API reference |
| importlib.resources | stdlib | Access .md guide files at runtime | For kafpy/guides/ package |

**Installation:**
```bash
pip install maturin pdoc
# rdkafka system library (handled separately - see BUILD-01)
```

## Architecture Patterns

### System Architecture Diagram

```
[Developer machine with librdkafka installed]
        |
        v
[ m a t u r i n   d e v e l o p ]
        |
        v
[Rust compilation: Cargo.toml -> target/debug/libkafpy.*.so]
        |
        v
[kafpy/ Python package]
  |-- __init__.py (public API re-exports)
  |-- config.py (ConsumerConfig, RoutingConfig, ...)
  |-- consumer.py (Consumer wrapper)
  |-- runtime.py (KafPy)
  |-- handlers.py (KafkaMessage, HandlerContext, HandlerResult)
  |-- exceptions.py (KafPyError hierarchy)
  |-- guides/ (markdown guide files as package)
  |     |-- __init__.py
  |     |-- getting-started.md
  |     |-- handler-patterns.md
  |     |-- configuration.md
  |     |-- error-handling.md
  |     |-- .static/ (if any images/css needed)
  |
[docs/ auto-generated API reference]
  |-- index.md (quickstart)
  |-- api/ (auto-generated from docstrings via pdoc)
```

### Recommended Project Structure

```
KafPy/
├── README.md                    # Root readme (DOC-01)
├── docs/
│   ├── index.md                # Quickstart guide (DOC-02)
│   ├── guides/
│   │   ├── handler-patterns.md # (DOC-06)
│   │   ├── configuration.md     # (DOC-03)
│   │   └── error-handling.md   # (DOC-03)
│   └── api/                     # Auto-generated via pdoc
├── kafpy/
│   ├── __init__.py             # BUILD-03
│   ├── __all__ defined in each module
│   ├── guides/                 # DOC-03: Python package with .md files
│   │   ├── __init__.py
│   │   ├── getting-started.md
│   │   ├── handler-patterns.md
│   │   ├── configuration.md
│   │   └── error-handling.md
│   └── (other modules)
├── pyproject.toml              # BUILD-01
└── Cargo.toml                  # BUILD-02
```

### Pattern 1: guides/ Package with importlib.resources

**What:** `kafpy/guides/` is a Python package (has `__init__.py`) that contains `.md` files as data resources. At runtime, code can read these guide files to display help text or serve them via a docs server.

**When to use:** For embedded help content that ships with the package and is accessible via `import kafpy.guides`.

**Example `__init__.py`:**
```python
"""Embedded guide documents for KafPy.

Access guide content via importlib.resources:

    from importlib.resources import files
    guide_text = (files("kafpy.guides") / "getting-started.md").read_text()
"""

from __future__ import annotations

__all__ = [
    "guides_available",
]

import importlib.resources as _resources

def guides_available() -> list[str]:
    """Return list of available guide names."""
    return ["getting-started", "handler-patterns", "configuration", "error-handling"]
```

**Example `getting-started.md`:**
```markdown
# Getting Started with KafPy

## Installation

KafPy requires Python 3.11+ and librdkafka.

```bash
pip install maturin
maturin develop --release
```

## Quick Start

...
```

### Pattern 2: Maturin pyproject.toml for PyO3 Extension Module

**What:** `pyproject.toml` configured with `maturin` build backend, Python source directory, extension module name, and maturin-specific settings.

**Example pyproject.toml (BUILD-01, BUILD-02):**
```toml
[project]
name = "kafpy"
version = "0.1.0"
description = "High-performance Kafka consumer for Python"
authors = [
    { name = "Dang Van Nghiem", email = "vannghiem848@gmail.com" }
]
license = { text = "BSD-3-Clause" }
readme = "README.md"
requires-python = ">=3.11"
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: BSD License",
    "Operating System :: OS Independent",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Rust",
    "Framework :: AsyncIO",
    "Typing :: Typed",
]
# BUILD-01: rdkafka is a SYSTEM LIBRARY, not a Python package.
# Install librdkafka before running maturin develop:
#   macOS: brew install librdkafka
#   Ubuntu/Debian: apt install librdkafka-dev
#   Fedora/RHEL: dnf install librdkafka-devel
# The Python package has zero pure-Python dependencies.
dependencies = []

[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[tool.maturin]
# BUILD-02: Python extension module name exposed by Cargo.toml [lib] name
module-name = "kafpy._kafpy"
# Location of Python source files for maturin to bundle
python-source = "kafpy"
```

### Pattern 3: Cargo.toml Extension Module Configuration

**What:** Cargo.toml `[lib]` section must define the extension module name that Python will import. The `[package.metadata.maturin]` must have `name = "kafpy"` (not the underscore variant) for the importable package name.

**Current state (Cargo.toml lines 8-10, 59-61):**
```toml
[lib]
name = "_kafpy"          # Actual .so file: lib_kafpy._kafpy.so
crate-type = ["cdylib", "rlib"]

[package.metadata.maturin]
name = "kafpy"           # Package name pip installs
python-source = "kafpy"
```

The `name = "_kafpy"` in `[lib]` is correct for BUILD-02 -- it creates the importable `_kafpy` extension that `kafpy/__init__.py` imports. The `package.metadata.maturin.name = "kafpy"` is also correct -- this is the pip-installable package name.

### Pattern 4: Google-Style Docstrings (Already in Use)

**What:** Docstrings using Google format with `Args:`, `Returns:`, `Raises:` sections. This is the format already used throughout KafPy (e.g., `kafpy/exceptions.py`, `kafpy/handlers.py`).

**Example:**
```python
def translate_rust_error(module: str, error_string: str) -> KafPyError:
    """Translate a Rust error string to the appropriate Python exception.

    Rust errors are logged with context-preserving messages (D-06/D-10).
    Parses the error string to extract error_code and topic/partition info.

    Args:
        module: The Rust module name that generated the error (e.g., "Consumer").
        error_string: The formatted error string from Rust.

    Returns:
        The appropriate KafPyError subclass instance with structured fields.

    Raises:
        No exceptions raised by this function directly.
    """
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Docstring generation for API reference | Write custom scripts to parse docstrings | [pdoc](https://pdoc.dev/) or [mkdocs](https://www.mkdocs.org/) with pydoc plugin | Battle-tested, handles all docstring formats, produces clean HTML |
| Accessing bundled .md files at runtime | Using `pkgutil` or manual file path construction | [importlib.resources](https://docs.python.org/3/library/importlib.resources.html) (stdlib, Python 3.9+) | Standard library, works with installed packages, no extra dependencies |
| Markdown rendering in docs/ | Custom HTML generation from .md | Serve raw .md files or use mkdocs material theme | Simpler for v1.8, defer complex rendering to v1.9 |

**Key insight:** For `kafpy/guides/`, use `importlib.resources.files("kafpy.guides").joinpath("getting-started.md").read_text()` to read guide content at runtime. Do not use `__file__` paths as those break when the package is installed as a zip.

## Common Pitfalls

### Pitfall 1: Wrong Extension Module Name in pyproject.toml
**What goes wrong:** `maturin develop` fails with "could not find extension module kafpy._kafpy" or import errors.
**Why it happens:** Mismatch between `module-name` in `[tool.maturin]` and the actual `[lib] name` in `Cargo.toml`. The Python `import kafpy._kafpy` must match the compiled `.so` filename.
**How to avoid:** The `module-name` in `pyproject.toml` MUST be `kafpy._kafpy` to match `Cargo.toml [lib] name = "_kafpy"`. Python imports `_kafpy`, not `kafpy`.
**Warning signs:** `ModuleNotFoundError: kafpy._kafpy` after `maturin develop` succeeds.

### Pitfall 2: Forgetting rdkafka System Library
**What goes wrong:** `maturin develop` compiles but fails at runtime with `Library not found: librdkafka`.
**Why it happens:** rdkafka is a system C library, not a Python package. maturin builds the Rust bindings but the dynamic linker can't find `librdkafka.so`.
**How to avoid:** Document in `pyproject.toml` that `librdkafka` must be installed separately. Verify with `ldconfig -librdkafka` or `pkg-config --libs rdkafka` before building.
**Warning signs:** `LD_LIBRARY_PATH` errors, `FileNotFoundError: librdkafka.so` on import.

### Pitfall 3: Star Imports Leaking Internal Modules
**What goes wrong:** `from kafpy import *` exposes `_private.py` internals or raw Rust names.
**Why it happens:** `__init__.py` using `from ._private import *` or similar.
**How to avoid:** BUILD-03 explicitly forbids star-imports from internal modules. Only use explicit re-exports.
**Warning signs:** Underscore-prefixed names in `kafpy.__all__`, raw `_kafpy` names in public API.

### Pitfall 4: Quickstart Example Too Long
**What goes wrong:** README quickstart exceeds 30 lines, violating DOC-01.
**Why it happens:** Including extensive configuration, error handling, or comments inflates the example.
**How to avoid:** The quickstart should show the minimal happy path: install -> config -> decorator -> app.run(). Everything else belongs in docs/guides.
**Warning signs:** Count lines in the code block -- every line including blank lines counts.

## Code Examples

### Quickstart Example (< 30 lines) — DOC-05

```python
# DOC-05: Under 30 lines — minimal happy path
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)

app = kafpy.KafPy(kafpy.Consumer(config))

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(msg.payload)
    return kafpy.HandlerResult(action="ack")

app.run()
```
Lines: 19 (including blank lines, counting is from "import" to "app.run()")

### pyproject.toml Full Configuration — BUILD-01, BUILD-02

```toml
[project]
name = "kafpy"
version = "0.1.0"
description = "High-performance Kafka consumer for Python"
authors = [
    { name = "Dang Van Nghiem", email = "vannghiem848@gmail.com" }
]
license = { text = "BSD-3-Clause" }
readme = "README.md"
requires-python = ">=3.11"
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: BSD License",
    "Operating System :: OS Independent",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Rust",
    "Framework :: AsyncIO",
    "Typing :: Typed",
]
# BUILD-01 note: rdkafka is a SYSTEM LIBRARY. Install before maturin develop:
#   macOS:  brew install librdkafka
#   Ubuntu: apt install librdkafka-dev
#   Fedora: dnf install librdkafka-devel
dependencies = []

[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[tool.maturin]
# BUILD-02: module-name must match Cargo.toml [lib] name = "_kafpy"
# Python imports _kafpy, so this must be "kafpy._kafpy"
module-name = "kafpy._kafpy"
python-source = "kafpy"
```

### kafpy/guides/__init__.py — DOC-03

```python
"""Embedded guide documents for KafPy.

KafPy ships with getting-started, handler-patterns, configuration,
and error-handling guides that are accessible at runtime.

Example usage::

    from importlib.resources import files
    guide = (files("kafpy.guides") / "getting-started.md").read_text()
    print(guide)

"""

from __future__ import annotations

__all__ = [
    "list_guides",
]

import importlib.resources as _resources

def list_guides() -> list[str]:
    """Return names of available guide documents."""
    return [
        "getting-started",
        "handler-patterns",
        "configuration",
        "error-handling",
    ]
```

### kafpy/guides/getting-started.md — DOC-03

```markdown
# Getting Started with KafPy

## Installation

KafPy requires Python 3.11+ and librdkafka.

### Prerequisites

**librdkafka** must be installed on your system before installing KafPy:

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora / RHEL
dnf install librdkafka-devel
```

### Install KafPy

Once librdkafka is installed, build and install KafPy:

```bash
pip install maturin
maturin develop --release
```

## Quick Start

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)

app = kafpy.KafPy(kafpy.Consumer(config))

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(msg.payload)
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Next Steps

- Read the [handler patterns guide](../guides/handler-patterns.md) for sync, async, and batch handlers
- Read the [configuration guide](../guides/configuration.md) for all config options
- Read the [error handling guide](../guides/error-handling.md) for exception handling
```

### kafpy/guides/handler-patterns.md — DOC-03, DOC-06

```markdown
# Handler Patterns

KafPy supports three handler types: sync, async, and batch.

## Sync Handler

A plain Python function receives each message individually:

```python
@app.handler(topic="events")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Received: {msg.payload}")
    return kafpy.HandlerResult(action="ack")
```

## Async Handler

An `async def` function handles messages concurrently:

```python
@app.handler(topic="events")
async def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    result = await process_message(msg.payload)
    return kafpy.HandlerResult(action="ack")
```

## Batch Handler

A generator function receives batches of messages:

```python
@app.handler(topic="events", routing=kafpy.RoutingConfig(routing_mode="python"))
async def handle_batch(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    for msg in messages:
        print(f"Batch item: {msg.payload}")
    return kafpy.HandlerResult(action="ack")
```

## Error Handling in Handlers

Raise a `HandlerError` for Python-side errors:

```python
from kafpy.exceptions import HandlerError

@app.handler(topic="events")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    if not msg.payload:
        raise HandlerError(
            message="empty payload",
            error_code=None,
            partition=msg.partition,
            topic=msg.topic,
        )
    return kafpy.HandlerResult(action="ack")
```

The runtime catches `HandlerError` and treats the handler as failed, triggering retry or DLQ.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| setup.py-only packaging | pyproject.toml with maturin | v1.8 (this project) | Single config file for both Python metadata and Rust build |
| Star imports for re-exports | Explicit re-exports in `__all__` | Phase 33 | BUILD-03 compliance, cleaner public API |
| No docstrings | Google-style docstrings on all public items | Phase 33-37 | Enables pdoc auto-generation |
| Hidden guides in README only | Embedded guides/ package + docs/ directory | Phase 37 | Runtime-accessible and docs-hostable guides |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `pyproject.toml [tool.maturin] module-name = "kafpy._kafpy"` is correct | Current State & Gaps | BUILD-02 violation -- extension won't import. Must verify against actual Cargo.toml [lib] name |
| A2 | The project uses Google-style docstrings (confirmed by reading exceptions.py and handlers.py) | Docstring Format | If numpy or sphinx format were expected, docstrings would look wrong. Current format is Google and is correct for this project |
| A3 | `maturin develop --release` works on a machine with librdkafka installed | maturin develop Workflow | BUILD-04 requires this exact command. Must verify maturin version >= 1.0 |

**If this table is empty:** All claims in this research were verified or cited.

## Open Questions

1. **What is the exact `module-name` value currently in pyproject.toml?**
   - What we know: The read showed `module-name = 'kafpy._kafpy'` which is correct
   - What's unclear: Whether this was recently changed or is the original value
   - Resolution needed: CONFIRMED correct. Verified by reading pyproject.toml directly.

2. **Does `maturin develop` currently work without additional configuration?**
   - What we know: maturin 1.12.4 is installed, Cargo.toml has proper [lib] and [package.metadata.maturin]
   - What's unclear: Whether there are any linking issues specific to this environment
   - Recommendation: Test `maturin develop --release` early in execution to catch any BUILD-04 issues

3. **Should `docs/` use pdoc auto-generation or hand-written Markdown?**
   - What we know: DOC-02 says "auto-generated from docstrings" but this is a MAY, not a MUST
   - What's unclear: Whether the team prefers automated or hand-maintained API docs
   - Recommendation: Start with hand-maintained quickstart.md in docs/ and pdoc-generated API reference

4. **Is there a `CONTEXT.md` for Phase 37?**
   - What we know: No Phase 37 context file exists yet
   - What's unclear: Nothing -- Phase 36 plans are complete per STATE.md
   - Recommendation: N/A

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| maturin | BUILD-01, BUILD-04 | yes | 1.12.4 | — |
| pdoc | DOC-02 | not installed | — | mkdocs with pydoc plugin |
| librdkafka | BUILD-04 | not verified | — | Install via system package manager |
| Python 3.11+ | project requirement | yes | 3.13.0 | — |
| Cargo/Rust toolchain | BUILD-02, BUILD-04 | yes | rustc 1.79.0 | — |

**Missing dependencies with fallback:**
- pdoc: `pip install pdoc` before running doc generation

**Missing dependencies with no fallback (blocking for BUILD-04):**
- librdkafka: Must be installed via system package manager before `maturin develop` works

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | pytest |
| Config file | `pytest.ini` or `pyproject.toml` [tool.pytest] (none detected — use pyproject.toml) |
| Quick run command | `pytest tests/ -x -q` |
| Full suite command | `pytest tests/ -v` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DOC-01 | README.md exists at root with install instructions, <30 line quickstart, feature list | manual | N/A (file check) | README.md exists |
| DOC-02 | docs/ directory has quickstart and API reference | manual | N/A (dir check) | docs/ does not exist yet |
| DOC-03 | kafpy/guides/ package with 4 .md files exists | manual | N/A (import check) | guides/ does not exist yet |
| DOC-04 | All public classes/functions have docstrings | unit | `pytest tests/test_docstrings.py -x` (to be created) | test_exceptions.py has some coverage |
| DOC-05 | Quickstart demo: install, config, handler, app.run() | manual | N/A (code inspection) | in README.md |
| DOC-06 | docs/guides/handler-patterns.md covers sync/async/batch/error | manual | N/A (file content check) | does not exist yet |
| BUILD-01 | pyproject.toml has correct maturin backend and rdkafka note | unit | `pytest tests/test_build.py -x` (to be created) | pyproject.toml exists |
| BUILD-02 | Cargo.toml [lib] name = "_kafpy" | unit | `pytest tests/test_build.py -x` | Cargo.toml verified |
| BUILD-03 | kafpy/__init__.py explicit re-exports, no star imports | unit | `pytest tests/test_pkg_structure.py -x` (to be created) | __init__.py verified |
| BUILD-04 | maturin develop works | manual/integration | `maturin develop --release` (manual verification) | N/A |

### Sampling Rate

- **Per task commit:** `pytest tests/test_docstrings.py tests/test_pkg_structure.py tests/test_build.py -x -q`
- **Per wave merge:** Full suite (pytest tests/ -v)
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `tests/test_docstrings.py` — verifies DOC-04 (all public APIs have docstrings)
- [ ] `tests/test_pkg_structure.py` — verifies BUILD-03 (explicit re-exports, no star imports)
- [ ] `tests/test_build.py` — verifies BUILD-01/BUILD-02 (pyproject.toml and Cargo.toml settings)
- [ ] Framework install: all test files use pytest (already available)

*(If no gaps: "None — existing test infrastructure covers all phase requirements")*

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | Docstrings document expected input types and ranges |
| V4 Access Control | no | Not applicable to documentation/packaging |
| V3 Session Management | no | Not applicable |
| V2 Authentication | no | Not applicable |

### Known Threat Patterns for maturin/Rust-PyO3 packaging

| Pattern | STRIDE | Standard Mitigation |
|--------|--------|---------------------|
| Malicious build script in pyproject.toml | Tampering | Use `--trusted-host` only for PyPI, verify maturin is from PyPI |
| rdkafka version mismatch | Denial of Service | Document required rdkafka version, provide version check at import |
| Accidental inclusion of secrets in .md files | Information Disclosure | Verify no credentials in shipped .md files |

## Sources

### Primary (HIGH confidence)
- [VERIFIED: maturin 1.12.4 installed] — `pip show maturin` returned version 1.12.4
- [VERIFIED: pyproject.toml] — Read directly from project root
- [VERIFIED: Cargo.toml] — Read directly from project root
- [VERIFIED: kafpy/__init__.py] — Read directly, confirms BUILD-03 compliance
- [VERIFIED: kafpy/exceptions.py] — Google-style docstrings confirmed
- [VERIFIED: kafpy/handlers.py] — Google-style docstrings confirmed

### Secondary (MEDIUM confidence)
- [pypi.org maturin page] — General maturin configuration patterns
- [PyO3 maturin documentation] — module-name configuration for PyO3 extensions
- [pdoc.dev] — pdoc auto-generation tool for API reference

### Tertiary (LOW confidence)
- [WebSearch: maturin pyproject.toml extension-module-name best practices] — General patterns, not verified against project

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM — maturin is well-documented, but version-specific behavior not verified
- Architecture: HIGH — Project structure understood from direct file reading
- Pitfalls: MEDIUM — Common maturin pitfalls understood, but environment-specific issues unknown

**Research date:** 2026-04-20
**Valid until:** 2026-05-20 (30 days — documentation patterns are stable)
