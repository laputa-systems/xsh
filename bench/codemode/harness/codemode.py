import argparse
import json
import shutil
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import contexts
import evaluate
import guidecheck
import manifest
import model
import prompts
import report
import tasks
import trial
from config import RUNS

ARMS = ("xsh", "ts")


def references(args: argparse.Namespace) -> int:
    manifest.verify_toolchain()
    manifest.verify_images()
    out = RUNS / "qualification" / time.strftime("%Y%m%dT%H%M%S")
    good = True
    rows = []
    for task in tasks.load(args.split, args.task):
        for arm in args.arm or ARMS:
            started = time.perf_counter()
            result = evaluate.evaluate(arm, task, task.reference(arm), out / task.id / f"reference-{arm}")
            seconds = time.perf_counter() - started
            ok = result.status == "correct"
            good &= ok
            rows.append({"task": task.id, "arm": arm, "status": result.status, "detail": result.detail,
                         "variants": len(result.variants), "seconds": round(seconds, 1)})
            print(f"{task.id:28} {arm:3} {result.status:8} variants {len(result.variants)} {seconds:6.1f}s "
                  f"{result.detail or ''}{(' ' + (result.feedback or '')[:400]) if not ok else ''}", flush=True)
    (out / "references.json").write_text(json.dumps(rows, indent=2))
    print(f"records: {out}")
    return 0 if good else 1


def judges(args: argparse.Namespace) -> int:
    manifest.verify_toolchain()
    out = RUNS / "qualification" / time.strftime("%Y%m%dT%H%M%S") / "wrong"
    good = True
    rows = []
    for task in tasks.load(args.split, args.task):
        programs = task.wrong_programs()
        if not programs:
            print(f"{task.id:28} has no wrong programs")
            good = False
        for name, arm, code in programs:
            result = evaluate.evaluate(arm, task, code, out / task.id / f"{arm}-{name}")
            rejected_by_judge = result.status == "wrong"
            good &= rejected_by_judge
            rows.append({"task": task.id, "program": name, "arm": arm, "status": result.status, "detail": result.detail})
            mark = "rejected" if rejected_by_judge else "NOT REJECTED BY THE JUDGE"
            print(f"{task.id:28} {arm:3} {name:28} {result.status:8} {mark}: {result.detail[:300]}", flush=True)
    out.mkdir(parents=True, exist_ok=True)
    (out / "judges.json").write_text(json.dumps(rows, indent=2))
    print(f"records: {out}")
    return 0 if good else 1


def show_prompt(args: argparse.Namespace) -> int:
    task = tasks.load(args.split, [args.task[0]])[0]
    print(prompts.initial_prompt(task, prompts.CONDITIONS[args.condition]), end="")
    return 0


def run_cells(args: argparse.Namespace) -> int:
    out = Path(args.out).resolve()
    if args.resume:
        loaded = manifest.load(out)
    else:
        selected = tasks.load(args.split, args.task)
        loaded = manifest.create(out, args.phase, selected, args.condition, args.samples, args.seed, args.jobs, args.split)
    manifest.verify_toolchain()
    manifest.verify_images()
    by_id = {t.id: t for t in tasks.load(loaded["split"], sorted(loaded["tasks"]))}
    for task_id, digest in loaded["tasks"].items():
        if by_id[task_id].digest() != digest:
            raise RuntimeError(f"{task_id} changed since the manifest was written")
    pending = []
    for cell in loaded["schedule"]:
        if args.limit is not None and len(pending) >= args.limit:
            break
        record = trial.load(trial.trial_directory(out, cell["task"], cell["condition"], cell["sample"]) / "trial.json")
        if record is not None and record.state == "done":
            continue
        if record is not None and record.state == "infra-error" and not args.retry_infra:
            continue
        if record is not None and record.state == "infra-error":
            shutil.rmtree(trial.trial_directory(out, cell["task"], cell["condition"], cell["sample"]))
        pending.append(cell)
    manifest.log_event(out, {"event": "start", "pending": len(pending), "jobs": args.jobs, "resume": args.resume})
    backoff = model.Backoff()
    stopped = None

    def one(cell: dict) -> str:
        condition = prompts.CONDITIONS[cell["condition"]]
        expected = loaded["contexts"][cell["condition"]]
        record = trial.run_trial(
            by_id[cell["task"]], condition, cell["sample"], out, backoff, None if expected is None else expected["sha256"],
        )
        statuses = [a.status for a in record.attempts]
        return f"{cell['order']:3} {cell['cell']:44} {record.state:11} {statuses}"

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(one, cell) for cell in pending]
        for future in futures:
            try:
                print(future.result(), flush=True)
            except model.ProviderUnavailable as unavailable:
                stopped = str(unavailable)
                print(f"provider unavailable: {unavailable}", flush=True)
                for other in futures:
                    other.cancel()
    manifest.log_event(out, {"event": "stop", "provider_stop": stopped, "backoff": backoff.log})
    return 3 if stopped else 0


def context(args: argparse.Namespace) -> int:
    built = contexts.build_baseline()
    path = contexts.baseline_path()
    if args.write:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(built)
    frozen = path.read_text() if path.exists() else None
    if frozen != built:
        print(f"{path} differs from the baseline the selection rule builds; rebuild with --write")
        return 1
    print(f"baseline {path.name}: sha256 {contexts.digest(built)}, {len(built.encode())} bytes")
    for name in ("candidate.md", "revised.md", "selected.md"):
        other = path.parent / name
        if other.exists():
            text = other.read_text()
            print(f"{name}: sha256 {contexts.digest(text)}, {len(text.encode())} bytes")
    return 0


def isolation(args: argparse.Namespace) -> int:
    manifest.verify_toolchain()
    probes = Path(__file__).resolve().parent.parent / "probes"
    out = RUNS / "qualification" / time.strftime("%Y%m%dT%H%M%S") / "isolation"
    task = tasks.load("dev", [args.task])[0]
    for arm, name in (("xsh", "isolation.xsh"), ("ts", "isolation.mts")):
        result = evaluate.evaluate(arm, task, (probes / name).read_text(), out / arm)
        public = result.variants[0] if result.variants else None
        print(f"## {arm} probe in the {task.id} boundary (check: {result.check['exit'] if result.check else None})")
        print(public.execution["stdout"] if public else result.feedback)
    print(f"records: {out}")
    return 0


def verify_guide(args: argparse.Namespace) -> int:
    results = guidecheck.verify(args.context)
    for result in results:
        print(f"{'ok  ' if result.ok else 'FAIL'} {result.name:40} {result.detail}")
    failed = sum(not r.ok for r in results)
    print(f"{len(results) - failed} of {len(results)} guide checks hold")
    return 1 if failed else 0


def render_report(args: argparse.Namespace) -> int:
    print(report.render([Path(p).resolve() for p in args.runs], args.taxonomy), end="")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="XSH code mode authoring experiment")
    sub = parser.add_subparsers(dest="command", required=True)

    def task_options(p: argparse.ArgumentParser) -> None:
        p.add_argument("--split", choices=tasks.SPLITS, default="dev")
        p.add_argument("--task", action="append")

    p = sub.add_parser("references", help="qualify reference solutions on every variant")
    task_options(p)
    p.add_argument("--arm", action="append", choices=ARMS)
    p.set_defaults(func=references)

    p = sub.add_parser("judges", help="show each judge rejects the task's plausible wrong programs")
    task_options(p)
    p.set_defaults(func=judges)

    p = sub.add_parser("prompt", help="print a condition's initial prompt for one task")
    task_options(p)
    p.add_argument("--condition", choices=sorted(prompts.CONDITIONS), required=True)
    p.set_defaults(func=show_prompt)

    p = sub.add_parser("trials", help="run or resume a matrix of scored trials")
    task_options(p)
    p.add_argument("--out", required=True)
    p.add_argument("--phase", default="dev")
    p.add_argument("--condition", action="append", choices=sorted(prompts.CONDITIONS))
    p.add_argument("--samples", type=int, default=2)
    p.add_argument("--seed", type=int, default=20261121)
    p.add_argument("--jobs", type=int, default=2)
    p.add_argument("--limit", type=int)
    p.add_argument("--resume", action="store_true")
    p.add_argument("--retry-infra", action="store_true")
    p.set_defaults(func=run_cells)

    p = sub.add_parser("context", help="build or verify the frozen baseline context and print context digests")
    p.add_argument("--write", action="store_true")
    p.set_defaults(func=context)

    p = sub.add_parser("isolation", help="run the isolation probes inside a task's program boundary")
    p.add_argument("--task", default="dev-audit-errors")
    p.set_defaults(func=isolation)

    p = sub.add_parser("verify-guide", help="check every statement and example of a guide against the frozen toolchain")
    p.add_argument("context", choices=["candidate.md", "revised.md", "selected.md"])
    p.set_defaults(func=verify_guide)

    p = sub.add_parser("report", help="report saved trials")
    p.add_argument("runs", nargs="+")
    p.add_argument("--taxonomy", type=Path)
    p.set_defaults(func=render_report)

    args = parser.parse_args()
    if args.command == "trials" and not args.resume and not args.condition:
        parser.error("trials needs --condition unless it resumes")
    if args.command == "trials" and args.jobs > 2:
        parser.error("keep model concurrency at 2 or below on the shared host")
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
