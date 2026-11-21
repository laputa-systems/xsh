import argparse
import json
import sys
import threading
import time
from pathlib import Path

import boundary
from config import XSH_IMAGE

PROTOCOL_VERSION = "2025-06-18"
EDITABLE_SUFFIX = ".xsh"
RESULT_CHARS = 12_000
TOOLS = [
    {
        "name": "api",
        "description": "Run `xsht api QUERY...` from the XSH toolchain and return its output. Queries: summary, "
                       "module:NAME, api:MODULE.FUNCTION, method:TYPE.METHOD, record:NAME, enum:NAME, language:ID, "
                       "search:WORDS. With no query it prints the getting-started text.",
        "inputSchema": {
            "type": "object",
            "properties": {"query": {"type": "array", "items": {"type": "string"}, "description": "Selectors"}},
            "required": ["query"],
        },
    },
    {
        "name": "edit",
        "description": "Write a whole file in your workspace, replacing it. The workspace holds main.xsh (your "
                       "program) and any other .xsh modules you add beside it.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
            "required": ["path", "content"],
        },
    },
    {
        "name": "check",
        "description": "Run `xsht check main.xsh` on your workspace and return the diagnostics.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "run",
        "description": "Run `xsh main.xsh -- ARGS...` with the working directory /work, the task's fixture. The "
                       "fixture keeps every change between runs. Returns the exit status, stdout and stderr.",
        "inputSchema": {
            "type": "object",
            "properties": {"args": {"type": "array", "items": {"type": "string"}}},
        },
    },
]


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

    def closed(self) -> bool:
        return (self.state / "closed").exists()

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
        return boundary.execute([*flags, XSH_IMAGE, *argv], name, self.timeout_s + 10)

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        if self.closed():
            return "The session's budget is spent; no more tool calls run.", True
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


def respond(message_id: object, result: dict) -> None:
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": message_id, "result": result}) + "\n")
    sys.stdout.flush()


def fail(message_id: object, code: int, text: str) -> None:
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": message_id, "error": {"code": code, "message": text}}) + "\n")
    sys.stdout.flush()


def main() -> int:
    parser = argparse.ArgumentParser(description="MCP stdio server offering the session tools inside the boundary")
    parser.add_argument("--state", type=Path, required=True)
    parser.add_argument("--service")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--env", default="{}")
    args = parser.parse_args()
    session = Session(args.state, args.service, args.timeout, json.loads(args.env))
    for line in sys.stdin:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        method, message_id = message.get("method"), message.get("id")
        if message_id is None:
            continue
        if method == "initialize":
            respond(message_id, {
                "protocolVersion": PROTOCOL_VERSION, "capabilities": {"tools": {}},
                "serverInfo": {"name": "xsh-session", "version": "1"},
            })
        elif method == "tools/list":
            respond(message_id, {"tools": TOOLS})
        elif method == "tools/call":
            params = message.get("params") or {}
            text, is_error = session.call(str(params.get("name")), params.get("arguments") or {})
            respond(message_id, {"content": [{"type": "text", "text": text}], "isError": is_error})
        elif method == "ping":
            respond(message_id, {})
        else:
            fail(message_id, -32601, f"unknown method {method}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
