import json
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import evaluate
import model
import prompts
from config import MAX_PROGRAMS
from tasks import Task

TRIAL_SCHEMA = "codemode-trial/1"


@dataclass(slots=True)
class Attempt:
    index: int
    prompt_file: str
    prompt_chars: int
    prompt_sections: dict
    reply_file: str
    reply: dict
    interruptions: list[dict]
    extraction: str
    code_file: str | None
    evaluation: dict | None
    status: str
    detail: str
    feedback: str | None
    started: float
    finished: float


@dataclass(slots=True)
class TrialRecord:
    schema: str
    cell: str
    task: str
    task_digest: str
    track: str
    condition: str
    arm: str
    context_digest: str | None
    sample: int
    model: str
    thinking_requested: str
    state: str
    attempts: list[Attempt] = field(default_factory=list)
    started: float = 0.0
    finished: float | None = None

    def to_json(self) -> dict:
        return asdict(self)


def load(path: Path) -> TrialRecord | None:
    if not path.is_file():
        return None
    data = json.loads(path.read_text())
    attempts = [Attempt(**a) for a in data.pop("attempts")]
    return TrialRecord(**data, attempts=attempts)


def save(record: TrialRecord, path: Path) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(record.to_json(), indent=2))
    temporary.replace(path)


def trial_directory(out: Path, task_id: str, condition: str, sample: int) -> Path:
    return out / "trials" / task_id / f"{condition}-{sample}"


def run_trial(
    task: Task, condition: prompts.Condition, sample: int, out: Path, backoff: model.Backoff,
    expected_context: str | None,
) -> TrialRecord:
    directory = trial_directory(out, task.id, condition.name, sample)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "trial.json"
    digest = prompts.context_digest(condition)
    if digest != expected_context:
        raise RuntimeError(f"{condition.name}: context digest {digest} differs from the manifest's {expected_context}")
    record = load(path)
    if record is None:
        record = TrialRecord(
            TRIAL_SCHEMA, f"{task.id}/{condition.name}/{sample}", task.id, task.digest(), task.data["track"],
            condition.name, condition.arm, digest, sample, model.MODEL, model.THINKING, "running", [], time.time(),
        )
    if record.task_digest != task.digest():
        raise RuntimeError(f"{record.cell}: the task changed since the trial started")
    if record.state in ("done", "infra-error"):
        return record
    original = prompts.initial_prompt(task, condition)
    while len(record.attempts) < MAX_PROGRAMS:
        index = len(record.attempts)
        previous = [(a_code(directory, a), a.feedback or "") for a in record.attempts]
        prompt = original if index == 0 else prompts.repair_prompt(original, previous, condition.arm)
        prompt_file = directory / f"prompt-{index}.md"
        prompts.write_text(prompt_file, prompt)
        started = time.time()
        reply, interruptions = model.ask_with_backoff(prompt, directory, directory / f"events-{index}.jsonl", backoff)
        reply_file = directory / f"reply-{index}.md"
        prompts.write_text(reply_file, reply.text)
        code, rule = prompts.extract(reply.text, condition.arm)
        base = dict(
            index=index, prompt_file=prompt_file.name, prompt_chars=len(prompt),
            prompt_sections=prompts.prompt_sections(prompt), reply_file=reply_file.name, reply=reply.to_json(),
            interruptions=interruptions, extraction=rule, started=started,
        )
        if not code:
            status = "provider-error" if reply.error and reply.stop_reason != "length" else "no-answer"
            record.attempts.append(Attempt(
                **base, code_file=None, evaluation=None, status=status, detail=reply.error or "empty reply",
                feedback=None, finished=time.time(),
            ))
            break
        code_file = directory / f"code-{index}{'.xsh' if condition.arm == 'xsh' else '.mts'}"
        prompts.write_text(code_file, code + "\n")
        evaluation = evaluate.evaluate(condition.arm, task, code, directory / f"attempt-{index}")
        record.attempts.append(Attempt(
            **base, code_file=code_file.name, evaluation=evaluation.to_json(), status=evaluation.status,
            detail=evaluation.detail, feedback=evaluation.feedback, finished=time.time(),
        ))
        save(record, path)
        if evaluation.status == "infra-error":
            record.state = "infra-error"
            break
        if evaluation.status == "correct" or evaluation.feedback is None:
            break
    if record.state != "infra-error":
        record.state = "done"
    record.finished = time.time()
    save(record, path)
    return record


def a_code(directory: Path, attempt: Attempt) -> str:
    if attempt.code_file is None:
        return ""
    return (directory / attempt.code_file).read_text().rstrip("\n")
