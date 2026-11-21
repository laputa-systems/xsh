import json
import subprocess
import threading
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

from config import MODEL, PI, PI_ISOLATION, SYSTEM_PROMPT, THINKING

CALL_TIMEOUT_S = 900
USAGE_FIELDS = ("input", "output", "cacheRead", "cacheWrite", "reasoning", "totalTokens")
PROVIDER_MARKERS = (
    "429", "rate limit", "rate-limit", "ratelimit", "too many requests", "quota", "overloaded", "503", "502",
    "504", "timeout", "timed out", "econnreset", "socket hang up", "fetch failed", "network", "unavailable",
    "insufficient", "limit exceeded", "free tier", "capacity", "api key", "login",
)
BACKOFF_SCHEDULE_S = (60, 120, 240, 480, 900, 1200, 1800, 1800)
BACKOFF_BUDGET_S = 2 * 3600
UNEXPLAINED_LIMIT = 3


class ModelRefused(ValueError):
    pass


class ProviderUnavailable(RuntimeError):
    pass


def allowed_model(model: str) -> str:
    if model != MODEL:
        raise ModelRefused(f"{model} is refused: every scored call uses {MODEL}")
    return model


@dataclass(slots=True)
class Usage:
    input: int | None = None
    output: int | None = None
    cache_read: int | None = None
    cache_write: int | None = None
    reasoning: int | None = None
    total: int | None = None
    cost_estimate: float | None = None
    cost_from_provider: bool | None = None

    @staticmethod
    def from_message(usage: dict | None) -> "Usage":
        if not isinstance(usage, dict):
            return Usage()

        def number(key: str) -> int | None:
            value = usage.get(key)
            return int(value) if isinstance(value, (int, float)) else None

        cost = usage.get("cost") if isinstance(usage.get("cost"), dict) else {}
        total_cost = cost.get("total")
        from_provider = cost.get("totalFromProvider")
        return Usage(
            input=number("input"), output=number("output"), cache_read=number("cacheRead"),
            cache_write=number("cacheWrite"), reasoning=number("reasoning"), total=number("totalTokens"),
            cost_estimate=float(total_cost) if isinstance(total_cost, (int, float)) else None,
            cost_from_provider=from_provider if isinstance(from_provider, bool) else None,
        )


@dataclass(slots=True)
class Reply:
    text: str
    usage: Usage
    stop_reason: str | None
    error: str | None
    provider_interruption: bool
    model: str | None
    provider: str | None
    thinking_level: str | None
    duration_ms: int | None
    wall_seconds: float
    exit: int | None
    content_kinds: list[str] = field(default_factory=list)
    events: int = 0

    def to_json(self) -> dict:
        return asdict(self)


def looks_like_provider(text: str) -> bool:
    lowered = text.lower()
    return any(marker in lowered for marker in PROVIDER_MARKERS)


def parse_events(stdout: str) -> tuple[list[dict], list[dict]]:
    events = []
    for line in stdout.splitlines():
        if line.startswith("{"):
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    assistant = [
        e["message"] for e in events
        if e.get("type") == "message_end" and isinstance(e.get("message"), dict) and e["message"].get("role") == "assistant"
    ]
    return events, assistant


def ask(prompt: str, workdir: Path, events_path: Path, model: str = MODEL, thinking: str = THINKING) -> Reply:
    command = [
        str(PI), "-p", "--mode", "json", *PI_ISOLATION, "--model", allowed_model(model), "--thinking", thinking,
        "--system-prompt", SYSTEM_PROMPT,
    ]
    workdir.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command, input=prompt.replace("\0", "\\u0000"), capture_output=True, text=True, cwd=workdir,
            timeout=CALL_TIMEOUT_S, check=False,
        )
        stdout, stderr, code = completed.stdout, completed.stderr, completed.returncode
    except subprocess.TimeoutExpired as expired:
        stdout = (expired.stdout or b"").decode() if isinstance(expired.stdout, bytes) else (expired.stdout or "")
        stderr, code = f"pi ran longer than {CALL_TIMEOUT_S} seconds", None
    wall = time.perf_counter() - started
    events_path.write_text("".join(
        line + "\n" for line in stdout.splitlines() if '"type":"message_update"' not in line[:40]
    ))
    (events_path.parent / (events_path.stem + ".stderr.txt")).write_text(stderr)
    events, assistant = parse_events(stdout)
    text: list[str] = []
    kinds: list[str] = []
    error = None
    usage = Usage()
    stop = model_id = provider = level = None
    duration = None
    if assistant:
        message = assistant[-1]
        for part in message.get("content", []):
            kinds.append(str(part.get("type")))
            if part.get("type") == "text":
                text.append(str(part.get("text", "")))
        usage = Usage.from_message(message.get("usage"))
        stop = message.get("stopReason")
        model_id, provider = message.get("model"), message.get("provider")
        level = message.get("thinkingLevel")
        duration = message.get("durationMs")
        if stop not in ("stop", "length"):
            error = f"the model stopped with {stop}: {message.get('errorMessage', '')}"
        elif stop == "length":
            error = "the model stopped with length: the reply hit the output limit"
    if code != 0 and error is None:
        error = f"pi exited {code}: {stderr.strip()[-800:]}"
    if not assistant and error is None:
        error = f"pi produced no assistant message: {stderr.strip()[-800:]}"
    interrupted = error is not None and stop != "length"
    return Reply(
        "".join(text), usage, stop, error, interrupted, model_id, provider, level, duration, wall, code, kinds,
        len(events),
    )


class Backoff:
    def __init__(self, budget_s: float = BACKOFF_BUDGET_S) -> None:
        self.lock = threading.Lock()
        self.until = 0.0
        self.waited = 0.0
        self.streak = 0
        self.budget = budget_s
        self.log: list[dict] = []

    def wait_turn(self) -> None:
        while True:
            with self.lock:
                remaining = self.until - time.monotonic()
            if remaining <= 0:
                return
            time.sleep(min(remaining, 5))

    def interrupted(self, reason: str) -> None:
        with self.lock:
            delay = BACKOFF_SCHEDULE_S[min(self.streak, len(BACKOFF_SCHEDULE_S) - 1)]
            self.streak += 1
            if self.waited + delay > self.budget:
                raise ProviderUnavailable(f"the provider stayed unavailable past {self.budget / 3600:.1f} h of backoff: {reason}")
            self.waited += delay
            self.until = max(self.until, time.monotonic() + delay)
            self.log.append({"at": time.time(), "delay_s": delay, "reason": reason[:500]})

    def succeeded(self) -> None:
        with self.lock:
            self.streak = 0


def ask_with_backoff(prompt: str, workdir: Path, events_path: Path, backoff: Backoff) -> tuple[Reply, list[dict]]:
    interruptions: list[dict] = []
    unexplained = 0
    while True:
        backoff.wait_turn()
        reply = ask(prompt, workdir, events_path)
        if not reply.provider_interruption:
            backoff.succeeded()
            return reply, interruptions
        interruptions.append({"at": time.time(), "error": reply.error, "usage": asdict(reply.usage)})
        if not looks_like_provider(reply.error or ""):
            unexplained += 1
            if unexplained >= UNEXPLAINED_LIMIT:
                return reply, interruptions
        backoff.interrupted(reply.error or "unknown")
