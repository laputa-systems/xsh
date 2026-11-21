import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "harness"))

from config import RUNS

FIELDS = ("input", "output", "cache_read", "cache_write", "reasoning", "total")


def main() -> int:
    attempts = unknown = mismatched = reasoning_outside = 0
    from_provider = set()
    for run in sys.argv[1:]:
        for path in sorted((RUNS / run).glob("trials/*/*/trial.json")) + sorted((RUNS / run).glob("discarded/*/*/trial.json")):
            for attempt in json.loads(path.read_text())["attempts"]:
                usage = attempt["reply"]["usage"]
                attempts += 1
                if any(usage.get(f) is None for f in FIELDS):
                    unknown += 1
                    continue
                parts = usage["input"] + usage["output"] + usage["cache_read"] + usage["cache_write"]
                mismatched += parts != usage["total"]
                reasoning_outside += usage["reasoning"] > usage["output"]
                from_provider.add(usage.get("cost_from_provider"))
    print(f"attempts {attempts}; with an unknown usage field {unknown}; "
          f"total != input + output + cache_read + cache_write: {mismatched}; reasoning > output: {reasoning_outside}; "
          f"cost totalFromProvider values: {sorted(map(str, from_provider))}")
    good = mismatched == 0 and reasoning_outside == 0
    print("usage accounting:", "holds" if good else "FAILS")
    return 0 if good else 1


if __name__ == "__main__":
    raise SystemExit(main())
