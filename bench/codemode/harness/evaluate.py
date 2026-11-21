import json
import re
import shutil
from dataclasses import asdict, dataclass, field
from pathlib import Path

import boundary
import bindings
from config import FEEDBACK_CHARS, FEEDBACK_LINES
from fixtures import Outcome, Setup, Verdict, snapshot, tree_effects
from tasks import Task, prepare_program

STATUSES = ("correct", "wrong", "failed", "rejected", "no-answer", "infra-error")
COMMAND_HEAD = re.compile(r"\brun(?:\.[a-z]+)?(?:\s+--[a-z_]+(?:=\[[^\]]*\]|=\S+)?)*\s+([A-Za-z0-9_./][A-Za-z0-9_./-]*)")
TS_SPAWN = re.compile(r"\b(?:execFile|execFileSync|spawn|spawnSync)\(\s*[\"'`]([^\"'`]+)[\"'`]")


@dataclass(slots=True)
class VariantRun:
    variant: str
    setup: dict
    execution: dict
    completed: bool
    verdict: dict
    calls: list[dict]
    effects: dict


@dataclass(slots=True)
class Evaluation:
    status: str
    detail: str
    feedback: str | None
    check: dict | None
    variants: list[VariantRun] = field(default_factory=list)
    diagnostic_codes: list[str] = field(default_factory=list)
    commands: list[str] = field(default_factory=list)
    check_seconds: float = 0.0
    run_seconds: float = 0.0

    def to_json(self) -> dict:
        return asdict(self)


def clip_feedback(text: str) -> str:
    lines = text.strip().splitlines()
    clipped = "\n".join(lines[:FEEDBACK_LINES])
    if len(lines) > FEEDBACK_LINES:
        clipped += f"\n[{len(lines) - FEEDBACK_LINES} more lines]"
    if len(clipped) > FEEDBACK_CHARS:
        clipped = clipped[:FEEDBACK_CHARS] + "\n[truncated]"
    return clipped


def diagnostic_codes(arm: str, text: str) -> list[str]:
    if arm == "xsh":
        return sorted(set(re.findall(r"^(?:err|warn)\[([a-z0-9_.-]+)\]", text, flags=re.MULTILINE)))
    return sorted(set(re.findall(r"error (TS\d+)", text)))


def external_commands(arm: str, code: str) -> list[str]:
    pattern = COMMAND_HEAD if arm == "xsh" else TS_SPAWN
    return sorted({m.group(1) for m in pattern.finditer(code)})


def setup_json(setup: Setup) -> dict:
    return {
        "argv": setup.argv, "env": setup.env, "stdin": None if setup.stdin is None else setup.stdin.decode(errors="replace"),
        "service": setup.responses is not None,
    }


def run_variant(arm: str, task: Task, program: Path, variant: str, directory: Path, public: bool) -> VariantRun:
    work = directory / "work"
    if directory.exists():
        shutil.rmtree(directory)
    work.mkdir(parents=True)
    setup: Setup = task.fixture.setup(variant, work)
    before = snapshot(work)
    service = None
    if task.shared:
        if setup.responses is None:
            raise ValueError(f"{task.id} {variant}: a shared task's setup returns service responses")
        service = boundary.start_service(bindings.service_spec(task.data, setup.responses), directory / "service")
    try:
        execution = boundary.run_program(
            arm, program, work, setup.argv, setup.env, setup.stdin, float(task.data["timeout_s"]), service,
        )
    finally:
        if service is not None:
            service.stop()
    calls = service.calls() if service is not None else []
    after = snapshot(work)
    allowed = execution.exit in task.data["exit_codes"] if public else execution.exit is not None
    completed = not execution.timed_out and allowed
    outcome = Outcome(execution.exit, execution.stdout, execution.stderr, execution.timed_out, calls, before, after)
    verdict: Verdict = task.fixture.judge(variant, work, outcome)
    if execution.timed_out and verdict.correct:
        verdict = Verdict.wrong("the program did not finish before the time limit", **verdict.effects)
    effects = {"tree": tree_effects(before, after), "calls": len(calls)}
    (directory / "execution.json").write_text(json.dumps(execution.to_json(), indent=2))
    return VariantRun(variant, setup_json(setup), execution.to_json(), completed, verdict.to_json(), calls, effects)


def failure_feedback(arm: str, run: VariantRun) -> str:
    execution = run.execution
    if execution["timed_out"]:
        head = "The program did not finish before the time limit and was stopped."
    else:
        head = f"The program exited with status {execution['exit']}."
    stderr = execution["stderr"].strip()
    body = f"stderr:\n{stderr}" if stderr else "It wrote nothing to stderr."
    return clip_feedback(f"{head}\n{body}")


def evaluate(arm: str, task: Task, code: str, directory: Path) -> Evaluation:
    program = prepare_program(arm, task, code, directory / "program")
    commands = external_commands(arm, code)
    try:
        checked = boundary.check_xsh(program) if arm == "xsh" else boundary.check_ts(program)
    except boundary.BoundaryError as failure:
        return Evaluation("infra-error", str(failure), None, None, commands=commands)
    check_record = checked.to_json()
    if checked.timed_out or checked.exit is None:
        return Evaluation("infra-error", "the checker timed out", None, check_record, commands=commands,
                          check_seconds=checked.seconds)
    if checked.exit != 0:
        text = (checked.stdout + "\n" + checked.stderr).strip()
        if arm == "xsh" and checked.exit != 2:
            return Evaluation("infra-error", f"xsht check exited {checked.exit}", None, check_record,
                              commands=commands, check_seconds=checked.seconds)
        return Evaluation(
            "rejected", "the checker rejected the program", clip_feedback(text), check_record, [],
            diagnostic_codes(arm, text), commands, checked.seconds,
        )
    runs: list[VariantRun] = []
    run_seconds = 0.0
    for index, variant in enumerate(task.variants):
        try:
            run = run_variant(arm, task, program, variant, directory / f"variant-{variant}", public=index == 0)
        except boundary.BoundaryError as failure:
            return Evaluation("infra-error", f"{variant}: {failure}", None, check_record, runs, [], commands,
                              checked.seconds, run_seconds)
        runs.append(run)
        run_seconds += run.execution["seconds"]
        if index == 0 and not run.completed:
            return Evaluation(
                "failed", f"the {variant} run did not complete", failure_feedback(arm, run), check_record, runs, [],
                commands, checked.seconds, run_seconds,
            )
    wrong = [r for r in runs if not r.verdict["correct"]]
    if wrong:
        detail = "; ".join(f"{r.variant}: {r.verdict['reason']}" for r in wrong)
        return Evaluation("wrong", detail, None, check_record, runs, [], commands, checked.seconds, run_seconds)
    return Evaluation("correct", "", None, check_record, runs, [], commands, checked.seconds, run_seconds)
