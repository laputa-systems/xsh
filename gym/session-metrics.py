#!/usr/bin/env python3
"""Summarize one Pi session and its gym evaluation manifest.

The session JSONL is Pi's durable source of truth. This adapter deliberately
keeps the report small and stable so the outer loop can compare runs without
depending on Pi's HTML export.
"""

from __future__ import annotations

import argparse
import json
import math
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any


def finite_number(value: Any) -> float | int | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    if not math.isfinite(value):
        return None
    return value


def add_number(target: dict[str, int | float], key: str, value: Any) -> None:
    number = finite_number(value)
    if number is not None:
        target[key] += number


def timestamp_ms(value: Any) -> int | None:
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return int(parsed.timestamp() * 1000)


def median(values: list[float]) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    middle = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[middle]
    return (ordered[middle - 1] + ordered[middle]) / 2


def read_session(path: Path) -> tuple[dict[str, Any], list[str]]:
    counters = Counter(
        assistant_turns=0,
        user_messages=0,
        tool_results=0,
        tool_errors=0,
        tool_calls=0,
        thinking_blocks=0,
        malformed_lines=0,
    )
    usage: dict[str, int | float] = {
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_read_tokens": 0,
        "cache_write_tokens": 0,
        "reasoning_tokens": 0,
        "cost_usd": 0,
    }
    tool_names: Counter[str] = Counter()
    stop_reasons: Counter[str] = Counter()
    timestamps: list[int] = []
    models: list[str] = []
    session_header: dict[str, Any] = {}

    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                counters["malformed_lines"] += 1
                continue
            if not isinstance(entry, dict):
                counters["malformed_lines"] += 1
                continue

            entry_time = timestamp_ms(entry.get("timestamp"))
            if entry_time is not None:
                timestamps.append(entry_time)
            if entry.get("type") == "session":
                session_header = entry
                continue
            if entry.get("type") != "message" or not isinstance(entry.get("message"), dict):
                continue

            message = entry["message"]
            role = message.get("role")
            if role == "user":
                counters["user_messages"] += 1
                continue
            if role == "toolResult":
                counters["tool_results"] += 1
                if message.get("isError") is True:
                    counters["tool_errors"] += 1
                continue
            if role != "assistant":
                continue

            counters["assistant_turns"] += 1
            stop_reason = message.get("stopReason")
            if isinstance(stop_reason, str):
                stop_reasons[stop_reason] += 1
            provider = message.get("provider")
            model = message.get("model")
            if isinstance(provider, str) and isinstance(model, str):
                models.append(f"{provider}/{model}")

            content = message.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    block_type = block.get("type")
                    if block_type == "thinking":
                        counters["thinking_blocks"] += 1
                    elif block_type == "toolCall":
                        counters["tool_calls"] += 1
                        name = block.get("name")
                        if isinstance(name, str):
                            tool_names[name] += 1

            message_usage = message.get("usage")
            if not isinstance(message_usage, dict):
                continue
            add_number(usage, "input_tokens", message_usage.get("input"))
            add_number(usage, "output_tokens", message_usage.get("output"))
            add_number(usage, "cache_read_tokens", message_usage.get("cacheRead"))
            add_number(usage, "cache_write_tokens", message_usage.get("cacheWrite"))
            add_number(usage, "reasoning_tokens", message_usage.get("reasoning"))
            cost = message_usage.get("cost")
            if isinstance(cost, dict):
                add_number(usage, "cost_usd", cost.get("total"))

    total_tokens = sum(
        usage[key]
        for key in ("input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens")
    )
    start_ms = min(timestamps) if timestamps else None
    end_ms = max(timestamps) if timestamps else None
    return (
        {
            "session_version": session_header.get("version"),
            "assistant_turns": counters["assistant_turns"],
            "user_messages": counters["user_messages"],
            "tool_calls": counters["tool_calls"],
            "tool_results": counters["tool_results"],
            "tool_errors": counters["tool_errors"],
            "thinking_blocks": counters["thinking_blocks"],
            "malformed_lines": counters["malformed_lines"],
            "tool_names": dict(sorted(tool_names.items())),
            "stop_reasons": dict(sorted(stop_reasons.items())),
            "models": sorted(set(models)),
            "usage": {
                **usage,
                "total_tokens": total_tokens,
            },
            "session_span_ms": None if start_ms is None or end_ms is None else end_ms - start_ms,
        },
        models,
    )


def read_manifest(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    return value if isinstance(value, dict) else {}


def program_metrics(manifest: dict[str, Any]) -> dict[str, Any]:
    timings = manifest.get("timings")
    if not isinstance(timings, dict):
        return {}
    pairs: list[tuple[float, float]] = []
    for key, value in timings.items():
        if not key.endswith("_candidate_wall_ns") or not isinstance(value, (int, float)):
            continue
        prefix = key[: -len("_candidate_wall_ns")]
        oracle = timings.get(f"{prefix}_oracle_wall_ns")
        if isinstance(oracle, (int, float)) and value > 0 and oracle > 0:
            pairs.append((float(value), float(oracle)))
    ratios = [candidate / oracle for candidate, oracle in pairs]
    candidate_median = median([candidate for candidate, _ in pairs])
    oracle_median = median([oracle for _, oracle in pairs])
    median_ratio = (
        candidate_median / oracle_median
        if candidate_median is not None and oracle_median is not None and oracle_median > 0
        else None
    )
    return {
        "result": manifest.get("result"),
        "correctness": manifest.get("correctness", {}),
        "timings": timings,
        "runtime_ratios": ratios,
        "candidate_wall_median_ns": candidate_median,
        "oracle_wall_median_ns": oracle_median,
        "median_runtime_ratio": median_ratio,
        "all_runtime_ratios_within_band": (
            all(0.9 <= ratio <= 1.1 for ratio in ratios) if ratios else None
        ),
        "within_runtime_band": (
            0.9 <= median_ratio <= 1.1 if median_ratio is not None else None
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--agent-wall-ms", type=float)
    args = parser.parse_args()

    session, _ = read_session(args.session)
    manifest = read_manifest(args.manifest) if args.manifest else {}
    report: dict[str, Any] = {
        "schema_version": 1,
        "session": session,
        "program": program_metrics(manifest),
        "inputs": manifest.get("inputs", {}),
        "result": manifest.get("result"),
    }
    if args.agent_wall_ms is not None and math.isfinite(args.agent_wall_ms):
        report["agent_wall_ms"] = args.agent_wall_ms

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, sort_keys=True)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
