import argparse
import json
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
import session
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
        try:
            prompts.xsh_api_facts(task.data["xsh_api"])
        except RuntimeError as missing:
            good = False
            print(f"{task.id:28} xsh_api selectors do not all resolve: {missing}", flush=True)
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
        if args.resume and args.task and cell["task"] not in args.task:
            continue
        if args.resume and args.only_sample is not None and cell["sample"] != args.only_sample:
            continue
        record = trial.load(trial.trial_directory(out, cell["task"], cell["condition"], cell["sample"]) / "trial.json")
        if record is not None and record.state == "done":
            continue
        if record is not None and record.state == "infra-error" and not args.retry_infra:
            continue
        if record is not None and record.state == "infra-error":
            directory = trial.trial_directory(out, cell["task"], cell["condition"], cell["sample"])
            kept = out / "discarded" / cell["task"] / f"{directory.name}-infra-{int(time.time())}"
            kept.parent.mkdir(parents=True, exist_ok=True)
            directory.rename(kept)
        pending.append(cell)
    manifest.log_event(out, {"event": "start", "pending": len(pending), "jobs": args.jobs, "resume": args.resume,
                             "retry_infra": args.retry_infra, "harness": manifest.harness_revision()})
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
    selected = path.parent / "selected.md"
    published = contexts.agent_context_path()
    if selected.exists() and published.exists():
        if contexts.published_body(published.read_text()) != selected.read_text():
            print("docs/agent-context.md differs from contexts/selected.md; edit the template or the snippet")
            return 1
        print("docs/agent-context.md publishes selected.md byte for byte")
    return 0


def measure_contexts(args: argparse.Namespace) -> int:
    out = RUNS / "measure" / time.strftime("%Y%m%dT%H%M%S")
    out.mkdir(parents=True)
    instruction = "Reply with only the word OK."
    rows = []
    texts = {"none": ""}
    for name in args.context:
        texts[name] = (contexts.baseline_path().parent / name).read_text()
    measured = {}
    for name, text in texts.items():
        prompt = f"{text.rstrip()}\n\n{instruction}\n" if text else instruction + "\n"
        reply = model.ask(prompt, out, out / f"{name}.events.jsonl")
        measured[name] = reply.usage.input
        rows.append({"context": name, "bytes": len(text.encode()), "input": reply.usage.input,
                     "cache_read": reply.usage.cache_read, "error": reply.error})
    for row in rows:
        base = measured["none"]
        row["context_tokens"] = None if row["input"] is None or base is None else row["input"] - base
        print(f"{row['context']:14} {row['bytes']:7} bytes  input {row['input']}  context tokens {row['context_tokens']}")
    (out / "measure.json").write_text(json.dumps(rows, indent=2))
    print(f"records: {out}")
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


def run_sessions(args: argparse.Namespace) -> int:
    out = Path(args.out).resolve()
    if args.resume:
        loaded = manifest.load(out)
    else:
        selected = tasks.load(args.split, args.task)
        loaded = manifest.create(out, "sessions", selected, args.condition, args.samples, args.seed, 1, args.split)
    manifest.verify_toolchain()
    manifest.verify_images()
    by_id = {t.id: t for t in tasks.load(loaded["split"], sorted(loaded["tasks"]))}
    for task_id, digest in loaded["tasks"].items():
        if by_id[task_id].digest() != digest:
            raise RuntimeError(f"{task_id} changed since the manifest was written")
    status = 0
    backoff = model.Backoff()
    for cell in loaded["schedule"]:
        directory = session.session_directory(out, cell["task"], cell["condition"], cell["sample"])
        record_path = directory / "session.json"
        if record_path.exists():
            previous = json.loads(record_path.read_text())
            if previous["state"] == "done":
                continue
            kept = directory.with_name(f"{directory.name}.interrupted-{int(time.time())}")
            directory.rename(kept)
        elif directory.exists():
            directory.rename(directory.with_name(f"{directory.name}.incomplete-{int(time.time())}"))
        expected = loaded["contexts"][cell["condition"]]
        record = session.run_session(
            by_id[cell["task"]], prompts.CONDITIONS[cell["condition"]], cell["sample"], out,
            None if expected is None else expected["sha256"], backoff,
        )
        print(f"{cell['cell']:44} {record.state:11} {record.verdict:9} turns {len(record.turns)} tools {record.tools} "
              f"{record.elapsed_seconds:.0f}s stopped by {record.stopped_by}{' ERROR ' + record.error if record.error else ''}",
              flush=True)
        status |= 3 if record.state == "interrupted" else 0
    return status


def failures(args: argparse.Namespace) -> int:
    out = Path(args.out).resolve()
    for path in sorted(out.glob("trials/*/*/trial.json")):
        record = trial.load(path)
        if record is None:
            continue
        for attempt in record.attempts:
            if attempt.status == "correct" or (args.condition and record.condition not in args.condition):
                continue
            print(f"## {record.task}/{record.condition}/{record.sample}/{attempt.index} {attempt.status}")
            print(f"detail: {attempt.detail}")
            if attempt.feedback:
                print("feedback:")
                print(attempt.feedback)
            if attempt.code_file:
                print(f"```\n{(path.parent / attempt.code_file).read_text()}```")
            print()
    return 0


SUGGESTED = {
    "check.redundant-parens": "syntax/binding", "check.named-arg": "syntax/binding",
    "check.bare-print-ident": "syntax/binding", "parse.expected-ident": "syntax/binding",
    "check.standard-module-shadow": "syntax/binding", "check.return-outside-callable": "syntax/binding",
    "check.module-member": "missing/misunderstood API", "check.unknown-method": "missing/misunderstood API",
    "check.unknown-module-api": "missing/misunderstood API", "check.unresolved-name": "missing/misunderstood API",
    "check.try-result": "result/optional/error handling", "check.result-fallback": "result/optional/error handling",
    "check.type-mismatch": "result/optional/error handling",
}


def taxonomy_draft(args: argparse.Namespace) -> int:
    out = Path(args.out).resolve()
    entries = []
    for path in sorted(out.glob("trials/*/*/trial.json")):
        record = trial.load(path)
        if record is None:
            continue
        for attempt in record.attempts:
            if attempt.status == "correct":
                continue
            codes = (attempt.evaluation or {}).get("diagnostic_codes", [])
            if attempt.status in ("infra-error", "provider-error"):
                category = "harness/provider"
            elif codes:
                category = SUGGESTED.get(codes[0], "")
            else:
                category = ""
            entries.append({
                "cell": record.cell, "attempt": attempt.index, "condition": record.condition, "task": record.task,
                "status": attempt.status, "codes": codes, "category": category, "note": "",
            })
    print(json.dumps({"run": out.name, "attempts": entries}, indent=2))
    return 0


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
    p.add_argument("--only-sample", type=int, help="with --resume, run only this sample's pending cells")
    p.set_defaults(func=run_cells)

    p = sub.add_parser("context", help="build or verify the frozen baseline context and print context digests")
    p.add_argument("--write", action="store_true")
    p.set_defaults(func=context)

    p = sub.add_parser("measure-contexts", help="measure each context's input tokens with one unscored call")
    p.add_argument("context", nargs="+")
    p.set_defaults(func=measure_contexts)

    p = sub.add_parser("isolation", help="run the isolation probes inside a task's program boundary")
    p.add_argument("--task", default="dev-audit-errors")
    p.set_defaults(func=isolation)

    p = sub.add_parser("verify-guide", help="check every statement and example of a guide against the frozen toolchain")
    p.add_argument("context", choices=["candidate.md", "revised.md", "selected.md"])
    p.set_defaults(func=verify_guide)

    p = sub.add_parser("sessions", help="run or resume tool-enabled agent sessions (XSH conditions)")
    task_options(p)
    p.add_argument("--out", required=True)
    p.add_argument("--condition", action="append", choices=sorted(c for c in prompts.CONDITIONS if c != "ts"))
    p.add_argument("--samples", type=int, default=1)
    p.add_argument("--seed", type=int, default=20261121)
    p.add_argument("--resume", action="store_true")
    p.set_defaults(func=run_sessions)

    p = sub.add_parser("failures", help="print every attempt that was not correct, for classification")
    p.add_argument("out")
    p.add_argument("--condition", action="append")
    p.set_defaults(func=failures)

    p = sub.add_parser("taxonomy-draft", help="draft a failure taxonomy with categories suggested from diagnostic codes")
    p.add_argument("out")
    p.set_defaults(func=taxonomy_draft)

    p = sub.add_parser("report", help="report saved trials")
    p.add_argument("runs", nargs="+")
    p.add_argument("--taxonomy", type=Path)
    p.set_defaults(func=render_report)

    args = parser.parse_args()
    if args.command in ("trials", "sessions") and not args.resume and not args.condition:
        parser.error(f"{args.command} needs --condition unless it resumes")
    if args.command == "trials" and args.jobs > 2:
        parser.error("keep model concurrency at 2 or below on the shared host")
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
