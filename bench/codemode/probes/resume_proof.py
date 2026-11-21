import json
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "harness"))

import model
import prompts
import tasks
import trial
from config import RUNS

TASK = "dev-audit-errors"
BROKEN = "use svc\nlet page = svc.audit_page(null)?\nprint page\n"


def reply(text: str, error: str | None = None) -> model.Reply:
    usage = model.Usage(input=100, output=10, cache_read=0, cache_write=0, reasoning=4, total=110,
                        cost_estimate=0.0, cost_from_provider=False)
    return model.Reply(text, usage if error is None else model.Usage(), "error" if error else "stop", error,
                       error is not None, "scripted", "scripted", "medium", 1, 0.0, 0 if error is None else 1)


def main() -> int:
    task = tasks.load("dev", [TASK])[0]
    condition = prompts.CONDITIONS["xsh-candidate"]
    out = RUNS / "proofs" / "resume"
    if out.exists():
        shutil.rmtree(out)
    model.BACKOFF_SCHEDULE_S = (0,)
    script = [reply(f"```xsh\n{BROKEN}```")]

    def first_run(prompt: str, workdir: Path, events: Path, *_: object) -> model.Reply:
        events.write_text("")
        if script:
            return script.pop(0)
        raise model.ProviderUnavailable("simulated: the provider stayed unavailable")

    model.ask = first_run
    digest = prompts.context_digest(condition)
    try:
        trial.run_trial(task, condition, 0, out, model.Backoff(), digest)
        print("unexpected: the first run finished")
        return 1
    except model.ProviderUnavailable as stopped:
        print(f"first run stopped: {stopped}")
    path = trial.trial_directory(out, TASK, condition.name, 0) / "trial.json"
    saved = json.loads(path.read_text())
    print(f"after the stop: state={saved['state']} attempts={[a['status'] for a in saved['attempts']]}")

    resumed = [reply("", "429 Too Many Requests: rate limit exceeded"), reply(f"```xsh\n{task.reference('xsh')}```")]

    def second_run(prompt: str, workdir: Path, events: Path, *_: object) -> model.Reply:
        events.write_text("")
        return resumed.pop(0)

    model.ask = second_run
    record = trial.run_trial(task, condition, 0, out, model.Backoff(), digest)
    print(f"after the resume: state={record.state} attempts={[a.status for a in record.attempts]} "
          f"interruptions on attempt 1={len(record.attempts[1].interruptions)}")
    repair = (path.parent / "prompt-1.md").read_text()
    print(f"repair prompt carries the checker's diagnostic: {'err[' in repair}")
    good = record.state == "done" and [a.status for a in record.attempts] == ["rejected", "correct"]
    good &= len(record.attempts[1].interruptions) == 1
    print("resume proof:", "holds" if good else "FAILS")
    return 0 if good else 1


if __name__ == "__main__":
    raise SystemExit(main())
