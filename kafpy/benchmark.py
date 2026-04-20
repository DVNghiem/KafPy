# kafpy/benchmark.py
# Python CLI entry point for running benchmarks (RUN-07).
#
# Usage:
#   python -m kafpy.benchmark run --scenario throughput --output ./results
#   python -m kafpy.benchmark run --scenario latency --output ./results --config '{"messages_per_second": 10000}'

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, Optional

import kafpy


def run(args: argparse.Namespace) -> None:
    """Execute a benchmark scenario."""
    scenario_name = args.scenario
    output_path = Path(args.output)
    output_path.mkdir(parents=True, exist_ok=True)

    # Build scenario configuration from args or defaults
    config: Dict[str, Any] = {
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

    # Create BenchmarkRunner (no external MetricsSink for CLI usage)
    runner = kafpy.BenchmarkRunner()

    # Run scenario asynchronously — use asyncio.run in a worker thread
    import asyncio

    async def run_async():
        return await runner.run_scenario_py(scenario_name, config_json)

    result = asyncio.run(run_async())

    # Write result to output file
    result_file = output_path / f"{scenario_name}_result.json"
    with open(result_file, "w") as f:
        json.dump(dict(result), f, indent=2)

    # Also write CSV if requested
    if args.csv:
        csv_file = output_path / f"{scenario_name}_result.csv"
        _write_csv(result, csv_file)

    print(f"Benchmark complete. Results written to {result_file}")

    # Print summary to stdout
    r = dict(result)
    print(f"  Scenario:     {r.get('scenario_name', scenario_name)}")
    print(f"  Duration:     {r.get('duration_ms', 0)} ms")
    print(f"  Throughput:   {r.get('throughput_msg_s', 0):.2f} msg/s")
    print(f"  Latency p50:  {r.get('latency_p50_ms', 0):.3f} ms")
    print(f"  Latency p95:  {r.get('latency_p95_ms', 0):.3f} ms")
    print(f"  Latency p99:  {r.get('latency_p99_ms', 0):.3f} ms")
    print(f"  Error rate:   {r.get('error_rate', 0):.4f}")


def _write_csv(result: Dict[str, Any], csv_path: Path) -> None:
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

    # 'run' subcommand
    run_parser = subparsers.add_parser("run", help="Run a benchmark scenario")
    run_parser.add_argument(
        "--scenario",
        type=str,
        required=True,
        choices=["throughput", "latency", "failure", "batch_vs_sync", "async_vs_sync"],
        help="Scenario to run",
    )
    run_parser.add_argument(
        "--output",
        type=str,
        default="./results",
        help="Output directory for results (default: ./results)",
    )
    run_parser.add_argument(
        "--topic",
        type=str,
        default="benchmark",
        help="Kafka topic to use (default: benchmark)",
    )
    run_parser.add_argument(
        "--payload-bytes",
        type=int,
        default=256,
        help="Message payload size in bytes (default: 256)",
    )
    run_parser.add_argument(
        "--warmup",
        type=int,
        default=1000,
        help="Number of warmup messages (default: 1000)",
    )
    run_parser.add_argument(
        "--num-messages",
        type=int,
        help="Total number of messages (scenario-dependent default)",
    )
    run_parser.add_argument(
        "--messages-per-second",
        type=int,
        help="Target message rate for latency scenarios",
    )
    run_parser.add_argument(
        "--duration-secs",
        type=int,
        help="Total duration for throughput scenarios (alternative to --num-messages)",
    )
    run_parser.add_argument(
        "--csv",
        action="store_true",
        help="Also write results to CSV file",
    )

    run_parser.set_defaults(func=run)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
