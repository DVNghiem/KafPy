# kafpy/benchmark.py
# Public API module for kafpy.benchmark.
# Provides frozen dataclasses and functions for running benchmarks and hardening checks.

"""
KafPy benchmark public API.

Example usage::

    from kafpy.benchmark import (
        ScenarioConfig,
        BenchmarkResult,
        BenchmarkReport,
        run_scenario,
        run_hardening_checks,
        HardeningCheck,
        ValidationResult,
    )

    # Run a throughput benchmark
    config = ScenarioConfig(
        scenario_type="throughput",
        num_messages=100_000,
        payload_bytes=256,
        rate=None,  # unlimited
        warmup_messages=1000,
        failure_rate=0.0,
    )
    result = run_scenario("throughput", config=config)
    print(f"Throughput: {result.throughput_msg_s:.2f} msg/s")

    # Validate production readiness
    validations = run_hardening_checks(result)
    for v in validations:
        print(f"{v.check_name}: {'PASS' if v.passed else 'FAIL'}")
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Optional

# Re-export from Rust extension (_kafpy)
try:
    from ._kafpy import run_scenario_py as _run_scenario_py
    from ._kafpy import run_hardening_checks_py as _run_hardening_checks_py
except ImportError:
    _run_scenario_py = None  # type: ignore
    _run_hardening_checks_py = None  # type: ignore


# ─── Public Dataclasses (frozen/immutable) ────────────────────────────────────


@dataclass(frozen=True)
class ScenarioConfig:
    """Scenario configuration for a benchmark run (mirrors Rust ScenarioConfig)."""

    scenario_type: str
    num_messages: int
    payload_bytes: int
    rate: Optional[int]  # messages per second, None = unlimited
    warmup_messages: int
    failure_rate: float


@dataclass(frozen=True)
class BenchmarkResult:
    """Immutable benchmark result returned after a scenario run."""

    scenario_config: ScenarioConfig
    total_messages: int
    duration_ms: int
    throughput_msg_s: float
    latency_p50_ms: float
    latency_p95_ms: float
    latency_p99_ms: float
    error_rate: float
    memory_delta_bytes: int
    timestamp_ms: int


@dataclass(frozen=True)
class BenchmarkReport:
    """Human-readable benchmark report with metrics and validation results."""

    scenario_name: str
    metrics: dict[str, Any]
    passed_checks: list[str]
    suggestions: list[str]


@dataclass(frozen=True)
class ValidationResult:
    """Result of a single hardening check."""

    check_name: str
    passed: bool
    details: str
    suggestions: list[str]


# ─── HardeningCheck Enum ───────────────────────────────────────────────────────


class HardeningCheck:
    """Hardening check identifiers for production readiness validation."""

    BACKPRESSURE = "BackpressureThreshold"
    MEMORY_LEAK = "MemoryLeakCheck"
    GRACEFUL_SHUTDOWN = "GracefulShutdownCheck"
    DLQ_DRAIN = "DlqDrainCheck"
    RETRY_BUDGET = "RetryBudgetCheck"


# ─── Public API Functions ──────────────────────────────────────────────────────


def run_scenario(
    scenario_type: str,
    *,
    num_messages: int = 100_000,
    payload_bytes: int = 256,
    rate: Optional[int] = None,
    warmup_messages: int = 1000,
    failure_rate: float = 0.0,
    target_topic: str = "benchmark",
) -> BenchmarkResult:
    """
    Run a benchmark scenario and return the result.

    Args:
        scenario_type: One of "throughput", "latency", "failure", "batch_vs_sync", "async_vs_sync"
        num_messages: Total messages to send
        payload_bytes: Size of each message payload
        rate: Target messages per second (None = unlimited)
        warmup_messages: Warmup messages excluded from measurement
        failure_rate: Failure injection rate (0.0 to 1.0)
        target_topic: Kafka topic to use

    Returns:
        BenchmarkResult with latency/throughput metrics

    Raises:
        RuntimeError: If the Rust extension is not available
        ValueError: If scenario_type is unknown or config is invalid
    """
    if _run_scenario_py is None:
        raise RuntimeError("kafpy Rust extension not built; run `maturin develop` or `maturin build` first")

    config_dict = {
        "target_topic": target_topic,
        "payload_bytes": payload_bytes,
        "warmup_messages": warmup_messages,
        "num_messages": num_messages,
    }

    if scenario_type == "throughput":
        config_dict["messages_per_second"] = None  # unlimited
        config_dict["duration_secs"] = None
    elif scenario_type == "latency":
        config_dict["messages_per_second"] = rate if rate is not None else 10_000
    else:
        raise ValueError(
            f"unknown scenario '{scenario_type}', expected one of: "
            "throughput, latency, failure, batch_vs_sync, async_vs_sync"
        )

    config_json = json.dumps(config_dict)
    result_json = _run_scenario_py(scenario_type, config_json)
    result_dict = json.loads(result_json)

    # Build ScenarioConfig from result
    scenario_config = ScenarioConfig(
        scenario_type=scenario_type,
        num_messages=num_messages,
        payload_bytes=payload_bytes,
        rate=rate,
        warmup_messages=warmup_messages,
        failure_rate=failure_rate,
    )

    return BenchmarkResult(
        scenario_config=scenario_config,
        total_messages=result_dict["total_messages"],
        duration_ms=result_dict["duration_ms"],
        throughput_msg_s=result_dict["throughput_msg_s"],
        latency_p50_ms=result_dict["latency_p50_ms"],
        latency_p95_ms=result_dict["latency_p95_ms"],
        latency_p99_ms=result_dict["latency_p99_ms"],
        error_rate=result_dict["error_rate"],
        memory_delta_bytes=result_dict["memory_delta_bytes"],
        timestamp_ms=result_dict["timestamp_ms"],
    )


def run_hardening_checks(result: BenchmarkResult) -> list[ValidationResult]:
    """
    Run hardening checks against a benchmark result.

    Args:
        result: BenchmarkResult from a previous run_scenario call

    Returns:
        List of ValidationResult, one per hardening check

    Raises:
        RuntimeError: If the Rust extension is not built
        ValueError: If the result JSON is invalid
    """
    if _run_hardening_checks_py is None:
        raise RuntimeError("kafpy Rust extension not built; run `maturin develop` or `maturin build` first")

    result_dict = {
        "scenario_name": result.scenario_config.scenario_type,
        "total_messages": result.total_messages,
        "duration_ms": result.duration_ms,
        "throughput_msg_s": result.throughput_msg_s,
        "latency_p50_ms": result.latency_p50_ms,
        "latency_p95_ms": result.latency_p95_ms,
        "latency_p99_ms": result.latency_p99_ms,
        "error_rate": result.error_rate,
        "memory_delta_bytes": result.memory_delta_bytes,
        "timestamp_ms": result.timestamp_ms,
    }

    result_json = json.dumps(result_dict)
    validations_json = _run_hardening_checks_py(result_json)
    validations_list = json.loads(validations_json)

    return [
        ValidationResult(
            check_name=v["check_name"],
            passed=v["passed"],
            details=v["details"],
            suggestions=list(v["suggestions"]),
        )
        for v in validations_list
    ]


# ─── Public API Surface ─────────────────────────────────────────────────────────

__all__ = [
    "run_scenario",
    "BenchmarkResult",
    "ScenarioConfig",
    "BenchmarkReport",
    "run_hardening_checks",
    "HardeningCheck",
    "ValidationResult",
]


# ─── CLI Entry Point ───────────────────────────────────────────────────────────
# Accessible via: python -m kafpy.benchmark run --scenario throughput --output ./results

if __name__ == "__main__":
    import argparse
    import sys
    from pathlib import Path

    def run(args: argparse.Namespace) -> None:
        """Execute a benchmark scenario."""
        scenario_name = args.scenario
        output_path = Path(args.output)
        output_path.mkdir(parents=True, exist_ok=True)

        config: dict[str, Any] = {
            "target_topic": args.topic or "benchmark",
            "payload_bytes": args.payload_bytes or 256,
            "warmup_messages": args.warmup or 1000,
        }

        if scenario_name == "throughput":
            config["messages_per_second"] = None  # unlimited
            config["num_messages"] = args.num_messages or 100_000
            config["duration_secs"] = None
        elif scenario_name == "latency":
            config["messages_per_second"] = args.messages_per_second or 10_000
            config["num_messages"] = args.num_messages or 100_000
        else:
            print(f"Error: unknown scenario '{scenario_name}'", file=sys.stderr)
            sys.exit(1)

        config_json = json.dumps(config)

        if _run_scenario_py is None:
            print("Error: kafpy Rust extension not built", file=sys.stderr)
            sys.exit(1)

        result_json = _run_scenario_py(scenario_name, config_json)
        result = json.loads(result_json)

        result_file = output_path / f"{scenario_name}_result.json"
        with open(result_file, "w") as f:
            json.dump(result, f, indent=2)

        if args.csv:
            csv_file = output_path / f"{scenario_name}_result.csv"
            _write_csv(result, csv_file)

        print(f"Benchmark complete. Results written to {result_file}")

        print(f"  Scenario:     {result.get('scenario_name', scenario_name)}")
        print(f"  Duration:     {result.get('duration_ms', 0)} ms")
        print(f"  Throughput:   {result.get('throughput_msg_s', 0):.2f} msg/s")
        print(f"  Latency p50:  {result.get('latency_p50_ms', 0):.3f} ms")
        print(f"  Latency p95:  {result.get('latency_p95_ms', 0):.3f} ms")
        print(f"  Latency p99:  {result.get('latency_p99_ms', 0):.3f} ms")
        print(f"  Error rate:   {result.get('error_rate', 0):.4f}")

    def _write_csv(result: dict[str, Any], csv_path: Path) -> None:
        """Write benchmark result to CSV format."""
        import csv

        headers = [
            "scenario_name", "total_messages", "duration_ms",
            "throughput_msg_s", "latency_p50_ms", "latency_p95_ms",
            "latency_p99_ms", "error_rate", "memory_delta_bytes", "timestamp_ms",
        ]
        with open(csv_path, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=headers)
            writer.writeheader()
            writer.writerow({k: result.get(k, "") for k in headers})

    def main() -> None:
        parser = argparse.ArgumentParser(
            prog="python -m kafpy.benchmark",
            description="KafPy benchmark runner",
        )
        subparsers = parser.add_subparsers(dest="command", required=True)

        run_parser = subparsers.add_parser("run", help="Run a benchmark scenario")
        run_parser.add_argument(
            "--scenario", type=str, required=True,
            choices=["throughput", "latency", "failure", "batch_vs_sync", "async_vs_sync"],
            help="Scenario to run",
        )
        run_parser.add_argument(
            "--output", type=str, default="./results",
            help="Output directory for results (default: ./results)",
        )
        run_parser.add_argument(
            "--topic", type=str, default="benchmark",
            help="Kafka topic to use (default: benchmark)",
        )
        run_parser.add_argument(
            "--payload-bytes", type=int, default=256,
            help="Message payload size in bytes (default: 256)",
        )
        run_parser.add_argument(
            "--warmup", type=int, default=1000,
            help="Number of warmup messages (default: 1000)",
        )
        run_parser.add_argument(
            "--num-messages", type=int,
            help="Total number of messages (scenario-dependent default)",
        )
        run_parser.add_argument(
            "--messages-per-second", type=int,
            help="Target message rate for latency scenarios",
        )
        run_parser.add_argument(
            "--duration-secs", type=int,
            help="Total duration for throughput scenarios (alternative to --num-messages)",
        )
        run_parser.add_argument(
            "--csv", action="store_true",
            help="Also write results to CSV file",
        )

        run_parser.set_defaults(func=run)
        args = parser.parse_args()
        args.func(args)

    main()