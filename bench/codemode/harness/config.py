import os
from pathlib import Path

HARNESS = Path(__file__).resolve().parent
BENCH = HARNESS.parent
ROOT = BENCH.parent.parent
RUNS = Path(os.environ.get("CODEMODE_RUNS", ROOT / ".work" / "codemode-runs"))
TOOLCHAIN = Path(os.environ.get("CODEMODE_TOOLCHAIN", RUNS / "toolchain"))
TASKS = BENCH / "tasks"
CONTEXTS = BENCH / "contexts"

XSH_IMAGE = "oci.17166969.xyz/xsh/test@sha256:fed05abfa97c947798fd88621278a2ecd9c7334a840ac638a2c7719b461bb0c6"
TS_BASE_IMAGE = "node@sha256:6bd9ab43c0ec0190ca7b2d193906cb8b7edf6e7f4afa43ddeccd6452710bffb5"
TS_IMAGE_TAG = "xsh-codemode-ts:local"
TS_IMAGE_ID = "sha256:b149f7ea2b99f98cf09a1fd7c12ae80794812d020618b1bca90d66314b50f958"
TOOLCHAIN_SHA256 = {
    "xsh": "08d7fe15e786931b9422a66df676de9477b0adfcfeb07611df360ca653985074",
    "xsht": "a4a5c9be204a301886dcab45c07606d0e589d64e5f28b821ab04bb012cd94e54",
}
LANE_BASE = "c68ac9594"
SX_REVISION = "9e9e4e5bf3fcb834748eafedd227e22f5c95b527"

MASKED_INTERPRETERS = ("/usr/bin/python3.14", "/usr/bin/perl", "/usr/bin/perl5.44.0")
SERVICE_PORT = 8080
CONTAINER_CPUS = "4"
CONTAINER_MEMORY = "2g"
CONTAINER_PIDS = "512"

PI = Path.home() / ".local" / "bin" / "pi"
MODEL = "opencode-go/hy4-flash-free"
THINKING = "medium"
SYSTEM_PROMPT = "You write one program that solves the task you are given. Reply with the program only."
PI_ISOLATION = (
    "--no-session", "--no-tools", "--no-extensions", "--no-mcp", "--no-skills", "--no-context-files",
    "--no-prompt-templates", "--offline",
)
MAX_PROGRAMS = 3
FEEDBACK_LINES = 60
FEEDBACK_CHARS = 6000
