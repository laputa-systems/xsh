import json
import os
import subprocess
import time
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path

from config import (
    CONTAINER_CPUS,
    CONTAINER_MEMORY,
    CONTAINER_PIDS,
    MASKED_INTERPRETERS,
    SERVICE_PORT,
    TOOLCHAIN,
    TS_IMAGE_TAG,
    XSH_IMAGE,
    HARNESS,
)

CHECK_TIMEOUT_S = 120
SERVICE_READY_S = 60
OUTPUT_LIMIT = 200_000
PATH_VALUE = "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"


class BoundaryError(RuntimeError):
    pass


@dataclass(slots=True)
class Execution:
    command: list[str]
    exit: int | None
    stdout: str
    stderr: str
    seconds: float
    timed_out: bool

    def to_json(self) -> dict:
        return asdict(self)


def uid_gid() -> str:
    return f"{os.getuid()}:{os.getgid()}"


def base_flags(name: str, network: str) -> list[str]:
    return [
        "docker", "run", "--rm", "--name", name, "--network", network,
        "--cpus", CONTAINER_CPUS, "--memory", CONTAINER_MEMORY, "--pids-limit", CONTAINER_PIDS,
        "--read-only", "--tmpfs", "/tmp:rw,exec,size=256m,mode=1777", "--user", uid_gid(),
        "--security-opt", "no-new-privileges", "--cap-drop", "ALL",
        "-e", "HOME=/tmp", "-e", "TMPDIR=/tmp", "-e", f"PATH={PATH_VALUE}", "-e", "LANG=C.UTF-8",
    ]


def clip(text: bytes) -> str:
    decoded = text.decode("utf-8", errors="replace")
    if len(decoded) > OUTPUT_LIMIT:
        return decoded[:OUTPUT_LIMIT] + f"\n[output truncated at {OUTPUT_LIMIT} characters]\n"
    return decoded


def kill_container(name: str) -> None:
    subprocess.run(["docker", "kill", name], capture_output=True, check=False)


def execute(command: list[str], name: str, timeout_s: float, stdin: bytes | None = None) -> Execution:
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command, input=stdin if stdin is not None else b"", capture_output=True, timeout=timeout_s, check=False,
        )
    except subprocess.TimeoutExpired as expired:
        kill_container(name)
        return Execution(
            command, None, clip(expired.stdout or b""), clip(expired.stderr or b""), time.perf_counter() - started, True,
        )
    return Execution(
        command, completed.returncode, clip(completed.stdout), clip(completed.stderr), time.perf_counter() - started,
        False,
    )


def container_name(kind: str) -> str:
    return f"codemode-{kind}-{uuid.uuid4().hex[:12]}"


def xsh_mounts() -> list[str]:
    mounts = [
        "-v", f"{TOOLCHAIN / 'xsh'}:/usr/local/bin/xsh:ro",
        "-v", f"{TOOLCHAIN / 'xsht'}:/usr/local/bin/xsht:ro",
    ]
    for path in MASKED_INTERPRETERS:
        mounts += ["-v", f"/dev/null:{path}:ro"]
    return mounts


def check_xsh(program_dir: Path) -> Execution:
    name = container_name("check")
    command = [
        *base_flags(name, "none"), *xsh_mounts(), "-v", f"{program_dir}:/task:ro", "-w", "/task",
        XSH_IMAGE, "xsht", "check", "/task/main.xsh",
    ]
    return execute(command, name, CHECK_TIMEOUT_S)


def check_ts(program_dir: Path) -> Execution:
    name = container_name("check")
    command = [
        *base_flags(name, "none"), "-v", f"{program_dir}:/task:ro", "-w", "/task",
        TS_IMAGE_TAG, "/opt/ts/node_modules/.bin/tsc", "-p", "/task/tsconfig.json", "--pretty", "false",
    ]
    return execute(command, name, CHECK_TIMEOUT_S)


@dataclass(slots=True)
class Service:
    name: str
    log: Path
    process: subprocess.Popen

    def stop(self) -> None:
        subprocess.run(["docker", "stop", "-t", "2", self.name], capture_output=True, check=False)
        try:
            self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            kill_container(self.name)
            self.process.wait(timeout=20)

    def calls(self) -> list[dict]:
        if not self.log.exists():
            return []
        records = [json.loads(line) for line in self.log.read_text().splitlines() if line.strip()]
        return [r for r in records if not r.get("ready")]


def start_service(spec: dict, directory: Path) -> Service:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "spec.json").write_text(json.dumps(spec))
    log = directory / "log"
    log.mkdir(exist_ok=True)
    calls = log / "calls.jsonl"
    calls.unlink(missing_ok=True)
    name = container_name("svc")
    command = [
        *base_flags(name, "none"),
        "-v", f"{HARNESS / 'service.mjs'}:/svc/service.mjs:ro", "-v", f"{directory / 'spec.json'}:/svc/spec.json:ro",
        "-v", f"{log}:/svc/log", TS_IMAGE_TAG, "node", "/svc/service.mjs", "/svc/spec.json", "/svc/log/calls.jsonl",
        str(SERVICE_PORT),
    ]
    process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    deadline = time.monotonic() + SERVICE_READY_S
    while time.monotonic() < deadline:
        if calls.exists() and '"ready":true' in calls.read_text():
            return Service(name, calls, process)
        if process.poll() is not None:
            raise BoundaryError(f"the fixture service exited early: {process.stderr.read().decode()[-2000:]}")
        time.sleep(0.05)
    kill_container(name)
    raise BoundaryError("the fixture service did not become ready")


def run_program(
    arm: str, program_dir: Path, work: Path, argv: list[str], env: dict[str, str], stdin: bytes | None,
    timeout_s: float, service: Service | None,
) -> Execution:
    name = container_name("run")
    network = f"container:{service.name}" if service is not None else "none"
    flags = [*base_flags(name, network), "-v", f"{program_dir}:/task:ro", "-v", f"{work}:/work", "-w", "/work"]
    for key, value in env.items():
        flags += ["-e", f"{key}={value}"]
    if stdin is not None:
        flags.append("-i")
    if arm == "xsh":
        command = [*flags, *xsh_mounts(), XSH_IMAGE, "xsh", "/task/main.xsh", *(["--", *argv] if argv else [])]
    else:
        command = [*flags, TS_IMAGE_TAG, "node", "/task/main.mts", *argv]
    return execute(command, name, timeout_s + 10, stdin)
