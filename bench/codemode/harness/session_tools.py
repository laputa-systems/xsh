import json
import threading
import time
from pathlib import Path

import boundary
from config import XSH_IMAGE

EDITABLE_SUFFIX = ".xsh"
RESULT_CHARS = 12_000


class SessionInfraError(RuntimeError):
    pass


class Session:
    def __init__(self, state: Path, service: str | None, timeout_s: float, env: dict[str, str]) -> None:
        self.state = state
        self.workspace = state / "workspace"
        self.work = state / "work"
        self.service = service
        self.timeout_s = timeout_s
        self.env = env
        self.log = (state / "tools.jsonl").open("a", buffering=1)
        self.lock = threading.Lock()

    def record(self, entry: dict) -> None:
        with self.lock:
            self.log.write(json.dumps({**entry, "time_ns": time.time_ns()}) + "\n")

    def container(self, argv: list[str], network: str, mount_work: bool) -> boundary.Execution:
        name = boundary.container_name("session")
        flags = [*boundary.base_flags(name, network), *boundary.xsh_mounts(), "-v", f"{self.workspace}:/task:ro"]
        if mount_work:
            flags += ["-v", f"{self.work}:/work", "-w", "/work"]
            for key, value in self.env.items():
                flags += ["-e", f"{key}={value}"]
        else:
            flags += ["-w", "/task"]
        try:
            return boundary.execute([*flags, XSH_IMAGE, *argv], name, self.timeout_s + 60)
        except boundary.BoundaryError as failure:
            self.record({"tool": "infra", "detail": str(failure)})
            raise SessionInfraError(str(failure)) from failure

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        if name == "api":
            query = [str(q) for q in arguments.get("query", [])]
            done = self.container(["xsht", "api", *query], "none", False)
            text = (done.stdout + done.stderr).strip()
            self.record({"tool": "api", "query": query, "exit": done.exit, "bytes": len(text.encode()), "seconds": done.seconds})
            return clip(text), done.exit != 0
        if name == "edit":
            relative = Path(str(arguments.get("path", "")))
            target = (self.workspace / relative).resolve()
            if target.parent != self.workspace.resolve() or target.suffix != EDITABLE_SUFFIX or target.name == "svc.xsh":
                self.record({"tool": "edit", "path": str(relative), "refused": True})
                return "edit writes only .xsh files directly in the workspace, and svc.xsh is read-only.", True
            content = str(arguments.get("content", ""))
            target.write_text(content)
            self.record({"tool": "edit", "path": str(relative), "bytes": len(content.encode())})
            return f"wrote {relative} ({len(content.encode())} bytes)", False
        if name == "check":
            done = self.container(["xsht", "check", "/task/main.xsh"], "none", False)
            text = (done.stdout + done.stderr).strip()
            self.record({"tool": "check", "exit": done.exit, "seconds": done.seconds})
            return clip(f"exit status {done.exit}\n{text}"), False
        if name == "run":
            args = [str(a) for a in arguments.get("args", []) or []]
            network = f"container:{self.service}" if self.service else "none"
            done = self.container(["xsh", "/task/main.xsh", *(["--", *args] if args else [])], network, True)
            self.record({"tool": "run", "args": args, "exit": done.exit, "timed_out": done.timed_out,
                         "seconds": done.seconds})
            status = "timed out" if done.timed_out else f"exit status {done.exit}"
            return clip(f"{status}\nstdout:\n{done.stdout}\nstderr:\n{done.stderr}"), False
        return f"unknown tool {name}", True


def clip(text: str) -> str:
    return text if len(text) <= RESULT_CHARS else text[:RESULT_CHARS] + "\n[output truncated]"
