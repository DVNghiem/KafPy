# Stack Research

**Domain:** Rust Code Quality Refactoring Tools
**Project:** KafPy (PyO3-based Kafka framework)
**Researched:** 2026-04-20
**Confidence:** HIGH

## Recommended Stack

### Core Tools

| Tool | Version | Purpose | Why |
|------|---------|---------|-----|
| `clippy` | bundled with Rust | Primary linter for code quality | 800+ lints covering correctness, style, complexity, perf, pedantic, nursery categories. Identifies god objects, cognitive complexity, code smells, and refactoring opportunities |
| `rustfmt` | bundled with Rust | Code formatting | Enforces consistent style; pairs with clippy for automated cleanup |
| `cargo-audit` | 0.22.1 | Security vulnerability scanning | Scans Cargo.lock for crates with known security advisories. Non-negotiable for production PyO3 libraries |
| `cargo-deny` | 0.19.4 | Dependency graph management | Enforces license compliance, bans duplicate deps, checks advisories at CI time |
| `cargo-machete` | 0.9.2 | Unused dependency detection | Finds deps listed in Cargo.toml but never imported. Reduces compile time and noise |

### Refactoring Analysis Tools

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `cargo-expand` | 1.0.121 | Macro expansion debugging | Reveals macro-generated code during refactoring. Identifies code that macro-expands into bloat or that belongs elsewhere |
| `cargo-udeps` | 0.1.60 | Unused deps by feature gate | More thorough than machete -- detects feature-gated unused deps |
| `cargo-bloat` | 0.12.1 | Binary size analysis | Identifies which modules/dependencies contribute most to binary size |
| `cargo-geiger` | 0.13.0 | Unsafe code tracking | Detects unsafe Rust usage in crate and dependencies. Critical for PyO3 bridges where unsafe is expected but should be minimal and isolated |
| `cargo-spellcheck` | 0.15.7 | Documentation spell check | Catches spelling errors in doc comments; maintains quality during refactoring |

### Code Fix Tools

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `rustfix` | 0.9.5 | Automated code fixing | Applies rustc suggestions automatically; used by clippy for auto-fixes |

## Code Smell Detection Reference

### Key Clippy Lints for Refactoring

| Lint | Category | Threshold | What It Catches |
|------|----------|-----------|----------------|
| `cognitive_complexity` | complexity | 30 | Functions too deeply nested or too many branches |
| `too_many_arguments` | complexity | 7 | Functions with excessive parameters |
| `too_many_lines` | complexity | 100 (files), 150 (functions) | Excessively long code units |
| `type_complexity` | complexity | 250 | Types with excessive trait implementations or generics |
| `module_name_repetitions` | style | N/A | Modules named redundantly (e.g., `consumer::consumer`) |
| `similar_names` | pedantic | N/A | Variables named too similarly to distinguish |
| `wildcard_imports` | style | N/A | Globs that obscure actual imports |
| `pub_use` | restriction | N/A | Re-exports that may indicate missing module boundaries |
| `rc_buffer` | pedantic | N/A | `Rc<Buffer>` where `Arc<Buffer>` would be clearer |
| `rc_mut` | perf | N/A | `Rc<RefCell<T>>` where `Arc<Mutex<T>>` may be more appropriate |
| `result_large_err` | complexity | 16 bytes | Error types unexpectedly large on stack |
| `struct_excessive_bools` | complexity | 4 | Structs with too many bool fields (suggests state enum) |
| `from_over_into` | style | N/A | `Into::into` used where `From::from` would be clearer |
| `redundant_closure` | complexity | N/A | Closures that just forward arguments |
| `unnecessary_struct_initialization` | style | N/A | Struct literal where default would suffice |
| `empty_loop` | suspicious | N/A | Empty loops indicating unfinished code |
| `unused_self` | style | N/A | Methods that never use `self` |
| `ref_binding_to_reference` | style | N/A | `ref` pattern in destructuring where `&` would be clearer |

### PyO3-Specific Lints

| Lint | Category | What It Catches |
|------|----------|----------------|
| `blocking` | perf | Blocking calls inside async context without `spawn_blocking` |
| `await_holding_refcell_ref` | correctness | Holding `RefCell` ref across await point (violates Send bound) |
| `async_in_async_fn` | style | `async fn` that returns `impl Future` instead of direct await |
| `磁带` (future-not-send) | correctness | Futures that are not `Send` across thread boundaries |
| `arc_with_non_send_sync` | style | `Arc` wrapping non-Send/Sync type |

## Installation

```bash
# Core tools (ship with Rust)
rustup component add clippy rustfmt

# Security and dependency auditing
cargo install cargo-audit cargo-deny cargo-machete

# Refactoring helpers
cargo install cargo-expand cargo-spellcheck

# Binary size analysis
cargo install cargo-bloat

# Unsafe code tracking
cargo install cargo-geiger
```

## Configuration

### clippy.toml for KafPy Code Quality Refactor

```toml
# .clippy.toml

# Critical for PyO3: do not auto-fix public API
avoid-breaking-exported-api = true

# Cognitive complexity: lower threshold forces smaller, more focused functions
cognitive-complexity-threshold = 25

# Too many arguments: KafPy's handler callbacks legitimately carry context
too-many-arguments-threshold = 8

# Type complexity: structs with many generic params need review
type-complexity-threshold = 200

# Disallowed names for test clarity
disallowed-names = ["foo", "bar", "baz", "quux", "tmp", "temp"]

# MSRV for the project (clippy respects this for feature-based lints)
msrv = "1.75"
```

### Cargo.toml workspace lint config

```toml
# Root Cargo.toml [workspace.lints.clippy]
[workspace.lints.clippy]
# Enforce stricter rules across workspace during refactor
cognitive_complexity = "warn"
too_many_arguments = "warn"
too_many_lines = "warn"
type_complexity = "warn"
unwrap_used = "deny"
expect_used = "warn"
panic = "deny"

# Nursery lints surface upcoming changes early
nursery = "warn"

# Allow noise lints that PyO3 bridge patterns legitimately trigger
module_name_repetitions = "allow"
must_use_candidate = "allow"
rc_buffer = "allow"

# Cargo-related lints
cargo = "warn"

[workspace.lints.rust]
# Forbid unsafe code in PyO3 crate; use explicit allow for PyO3 internals
unsafe_code = "forbid"
```

### Per-crate override in PyO3 bridge crate

```toml
# src/pyconsumer/Cargo.toml (if extracted)
[package]
name = "kafpy-pyconsumer"

[lints]
workspace = true  # Inherit workspace settings

# Override specific noisy lints for PyO3 bridge
[lints.clippy]
arc_with_non_send_sync = "allow"  # PyO3 types may legitimately not be Send/Sync
```

### cargo-deny configuration

```toml
# deny.toml or [workspace.metadata.cargo-deny]
[licenses]
unlicensed = "deny"
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0"]

[advisories]
ignore = []  # Review advisories; do not blanket ignore

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []
```

## Refactoring Workflow for v2.0

### Phase 1: Baseline Analysis

```bash
# Get baseline complexity metrics across all crates
cargo clippy --all-features --all-targets -- \
  -W clippy::cognitive_complexity \
  -W clippy::type_complexity \
  -W clippy::too_many_arguments \
  -W clippy::too_many_lines \
  -W clippy::struct_excessive_bools \
  -W clippy::result_large_err \
  | tee /tmp/clippy-baseline.txt

# Find unused dependencies
cargo machete

# Check for unsafe code (baseline)
cargo geiger

# Binary size baseline
cargo bloat --release --features full 2>/dev/null | tee /tmp/bloat-baseline.txt

# Format check (before any manual changes)
cargo fmt -- --check
```

### Phase 2: Automated Fixes

```bash
# Apply rustfix-compatible suggestions (dry run first)
cargo fix --edition --allow-dirty --dry-run

# Apply for real
cargo fix --edition --allow-dirty

# Format all code after fixes
cargo fmt

# Re-check unused deps after automated fixes
cargo machete
```

### Phase 3: Structured Refactoring Triggers

Based on clippy output, systematically address:

1. **Cognitive complexity violations** -- Extract helper functions, reduce branching depth
2. **Type complexity violations** -- Split large trait impls, extract sub-traits
3. **Too many arguments** -- Introduce a config/context struct for related params
4. **Struct excessive bools** -- Replace with state enum (clippy suggests this)
5. **Module name repetitions** -- Rename modules to avoid redundancy
6. **Pub use warnings** -- Review re-export decisions; may reveal boundary issues

### Phase 4: Behavior Verification

```bash
# Run full test suite
cargo test --all-features
cargo nextest run --all-features 2>/dev/null || cargo test --all-features

# Clippy on all features and targets (including tests)
cargo clippy --all-features --all-targets

# Security audit
cargo audit

# Verify formatting
cargo fmt -- --check
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|------------------------|
| clippy (bundled) | rust-analyzer diagnostics | rust-analyzer gives real-time IDE feedback; clippy is better for CI and batch analysis |
| cargo-machete | cargo-udeps | cargo-udeps is more thorough with feature-gated deps; machete is faster for CI loops |
| cargo-audit | cargo-deny advisory check | cargo-deny covers advisories AND licenses/sources; cargo-audit is advisory-only |
| cargo-geiger | manual unsafe audit | Human review misses subtle issues; always pair with cargo-geiger |
| clippy complexity lints | sonar-rust | sonar-rust offers deeper semantic analysis but is heavier; clippy is standard and well-maintained |
| cargo-bloat | cargo-size | cargo-size offers more visual output; cargo-bloat is simpler for CI |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| Restriction lints as `deny` | Restriction category (`clippy::restriction`) bans language features; too noisy for production CI | Use `correctness = "deny"`, `style/complexity/perf = "warn"` |
| Blanket `allow` for nursery | Nursery lints become stable lints; ignoring them delays catching upcoming breaking changes | Set `nursery = "warn"` to be informed without blocking |
| `cargo-tarpaulin` for coverage | Tarpaulin has known false positives with async/Tokio code | Use `cargo-llvm-cov` for accurate Rust coverage |
| Blanket `unsafe_code = "allow"` | PyO3 requires unsafe but it should be minimal and isolated | Use `unsafe_code = "forbid"` at workspace level, with explicit per-block allow comments |
| Manual unsafe review alone | Human review misses subtle issues (aliasing, Send bounds) | Always pair with `cargo-geiger` |

## Key Refactoring Patterns for Rust

### 1. God Object Splitting
Use `cognitive_complexity` and `type_complexity` to identify structs/traits doing too much. Extract coherent subgroups into new modules.

**Signal:** `type_complexity` threshold exceeded on a single struct.
**Action:** Identify distinct responsibilities, extract sub-structs with their impl blocks.

### 2. Trait Extraction from Large Impl Blocks
When a struct impl implements many methods covering distinct responsibilities, extract a trait and keep the struct implementing only that trait.

**Signal:** `cognitive_complexity` violation in a method-heavy impl.
**Action:** Group methods by responsibility, define trait, move impl to trait + impl Struct.

### 3. Builder Pattern for Config Objects
KafPy's `ConsumerConfigBuilder` already uses this. Verify all config objects follow it consistently.

**Signal:** Structs with >4 constructor args or deeply nested `Option` fields.
**Action:** Introduce or extend a builder with named methods.

### 4. Sealed Traits for Internal APIs
PyO3-exposed traits that should not be implemented outside the crate need the sealed trait pattern.

**Signal:** `pub trait` that should be impl-only-in-crate.
**Action:**
```rust
mod sealed {
    pub trait Sealed {}
}
pub trait MyTrait: sealed::Sealed {}
impl sealed::Sealed for MyType {}
```

### 5. Module Boundary Review
`pub use` warnings identify re-exports that may indicate missing module boundaries or leaky abstractions.

**Signal:** `clippy::pub_use` warnings.
**Action:** Decide if re-export is intentional API convenience or if it reveals a missing direct import path.

### 6. Error Type Unification
`result_large_err` finds oversized error types. Prefer `thiserror` for clean error enums.

**Signal:** Error types >16 bytes on stack.
**Action:** Use `thiserror` to define lightweight enum variants instead of large struct fields.

### 7. Async/Sync Boundary Clarity
PyO3 bridges need clear async/sync separation. Use `blocking` lint and explicit `spawn_blocking` to catch violations.

**Signal:** `clippy::blocking` warning when `spawn_blocking` is the correct call.
**Action:** Wrap blocking Python calls in `spawn_blocking` with explicit await.

### 8. Explicit over Implicit State
Replace bool fields with explicit state enums where behavior depends on combinations of state.

**Signal:** `clippy::struct_excessive_bools` or `option_option` / `option_result` noise.
**Action:** Replace `struct { a: bool, b: bool, c: bool }` with `enum State { Initial, Processing, ... }`.

## PyO3-Specific Considerations

### Unsafe Code Isolation
PyO3 requires unsafe for Python interpreter bridging. Keep all unsafe code:
1. Isolated in a single module (e.g., `src/pyo3/unsafe_impls.rs`)
2. Documented with comments explaining the safety invariant
3. Tracked via `cargo-geiger` on every PR

### GIL Lifetimes
`Py<PyAny>` storage must remain valid across thread Send/Sync bounds. Clippy's `arc_with_non_send_sync` may fire for legitimate PyO3 types. Use allow annotations with comments.

### Blocking in Async Context
Python callbacks invoked via `spawn_blocking` must not hold GIL across await points. The `clippy::blocking` lint catches violations of this pattern.

## Version Compatibility

| Tool | Min Rust Version | Notes |
|------|-----------------|-------|
| clippy (bundled) | Matches toolchain | Always `rustup update` for latest |
| cargo-audit 0.22.1 | Rust 1.56+ | Advisory DB updated daily |
| cargo-deny 0.19.4 | Rust 1.70+ | MSRV enforced |
| cargo-machete 0.9.2 | Rust 1.60+ | Fast; no MSRV bump |
| cargo-expand 1.0.121 | Rust 1.56+ | Some features need nightly |
| cargo-geiger 0.13.0 | Rust 1.56+ | Not actively maintained; still functional |
| cargo-spellcheck 0.15.7 | Rust 1.56+ | hunspell backend recommended |
| rustfix 0.9.5 | Rust 1.56+ | Used by `cargo fix` internally |

## Sources

- [Clippy documentation](https://rust-lang.github.io/rust-clippy/) — lint categories, complexity thresholds (HIGH confidence)
- [Clippy Configuration MD](https://github.com/rust-lang/rust-clippy/blob/master/book/src/configuration.md) — clippy.toml reference (HIGH confidence)
- [Clippy Lint List](https://rust-lang.github.io/rust-clippy/master/lints.html) — complete lint reference (HIGH confidence)
- crates.io — verified current versions: clippy 0.0.302, cargo-audit 0.22.1, cargo-deny 0.19.4, cargo-machete 0.9.2, cargo-geiger 0.13.0, cargo-expand 1.0.121, cargo-spellcheck 0.15.7, rustfix 0.9.5 (HIGH confidence)

---
*Stack research for: Rust code quality refactoring tools*
*Researched: 2026-04-20*
