import hashlib
import importlib.util
import json
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType

import bindings
from config import TASKS

SPLITS = ("dev", "held", "session")
TRACKS = ("shared", "native")
REQUIRED = (
    "id", "split", "track", "families", "description", "variants", "exit_codes", "timeout_s", "xsh_api", "ts_modules",
)
TSCONFIG = {
    "compilerOptions": {
        "strict": True, "noEmit": True, "target": "es2024", "lib": ["es2024"], "module": "nodenext",
        "moduleResolution": "nodenext", "erasableSyntaxOnly": True, "allowImportingTsExtensions": True,
        "skipLibCheck": True, "types": ["node"], "typeRoots": ["/opt/ts/node_modules/@types"],
    },
}
EXTENSIONS = {"xsh": ".xsh", "ts": ".mts"}


@dataclass(frozen=True, slots=True)
class Task:
    id: str
    directory: Path
    data: dict
    fixture: ModuleType

    @property
    def shared(self) -> bool:
        return self.data["track"] == "shared"

    @property
    def variants(self) -> list[str]:
        return list(self.data["variants"])

    def reference(self, arm: str) -> str:
        return (self.directory / f"reference{EXTENSIONS[arm]}").read_text()

    def wrong_programs(self) -> list[tuple[str, str, str]]:
        found = []
        wrong = self.directory / "wrong"
        if wrong.is_dir():
            for path in sorted(wrong.iterdir()):
                arm = "xsh" if path.suffix == ".xsh" else "ts" if path.suffix == ".mts" else None
                if arm is not None:
                    found.append((path.stem, arm, path.read_text()))
        return found

    def digest(self) -> str:
        hasher = hashlib.sha256()
        for name in ("task.json", "fixture.py"):
            hasher.update(name.encode() + b"\0" + (self.directory / name).read_bytes() + b"\0")
        return hasher.hexdigest()


def load_fixture(directory: Path, task_id: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(f"fixture_{task_id.replace('-', '_')}", directory / "fixture.py")
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {directory / 'fixture.py'}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    for name in ("setup", "judge"):
        if not callable(getattr(module, name, None)):
            raise ValueError(f"{directory / 'fixture.py'} defines no {name}()")
    return module


def validate(data: dict, directory: Path) -> None:
    missing = [key for key in REQUIRED if key not in data]
    if missing:
        raise ValueError(f"{directory / 'task.json'} lacks {', '.join(missing)}")
    if data["id"] != directory.name:
        raise ValueError(f"{directory}: id {data['id']!r} differs from the directory name")
    if data["split"] not in SPLITS or data["track"] not in TRACKS:
        raise ValueError(f"{directory}: split or track is not one of {SPLITS} / {TRACKS}")
    if data["track"] == "shared" and not data.get("capabilities"):
        raise ValueError(f"{directory}: a shared task declares capabilities")
    if not data["exit_codes"] or {1, 3} & set(data["exit_codes"]):
        raise ValueError(f"{directory}: exit_codes must be nonempty and exclude 1 and 3, the crash statuses")
    for capability in data.get("capabilities", []):
        if not capability["input"]:
            raise ValueError(f"{directory}: {capability['id']} declares no input field")
    bindings.capabilities(data)
    bindings.types(data)


def load(split: str, only: list[str] | None = None) -> list[Task]:
    root = TASKS / split
    found = []
    if not root.is_dir():
        return found
    for directory in sorted(p for p in root.iterdir() if (p / "task.json").is_file()):
        data = json.loads((directory / "task.json").read_text())
        validate(data, directory)
        if only and data["id"] not in only:
            continue
        found.append(Task(data["id"], directory, data, load_fixture(directory, data["id"])))
    if only:
        unknown = sorted(set(only) - {t.id for t in found})
        if unknown:
            raise ValueError(f"no {split} task named {', '.join(unknown)}")
    return found


def prepare_program(arm: str, task: Task, code: str, directory: Path) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    if arm == "xsh":
        (directory / "main.xsh").write_text(code if code.endswith("\n") else code + "\n")
        if task.shared:
            (directory / "svc.xsh").write_text(bindings.xsh_module(task.data))
    else:
        (directory / "main.mts").write_text(code if code.endswith("\n") else code + "\n")
        files = ["main.mts"]
        if task.shared:
            (directory / "svc.mts").write_text(bindings.ts_module(task.data))
            files.append("svc.mts")
        (directory / "tsconfig.json").write_text(json.dumps({**TSCONFIG, "files": files}, indent=2))
    return directory
