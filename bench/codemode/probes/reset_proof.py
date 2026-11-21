import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "harness"))

import evaluate
import tasks
from config import RUNS


def main() -> int:
    task = tasks.load("dev", ["dev-config-set"])[0]
    code = (Path(__file__).resolve().parent / "reset.xsh").read_text()
    directory = RUNS / "proofs" / "reset" / "attempt"
    good = True
    for round_number in (1, 2):
        result = evaluate.evaluate("xsh", task, code, directory)
        for run in result.variants:
            line = run.execution["stdout"].strip()
            good &= line.startswith("marker already present: false")
            print(f"round {round_number} {run.variant:10} {line}")
    print("fixture reset proof:", "holds" if good else "FAILS")
    return 0 if good else 1


if __name__ == "__main__":
    raise SystemExit(main())
