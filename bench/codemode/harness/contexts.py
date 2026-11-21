import hashlib
import re
import subprocess
from pathlib import Path

from config import CONTEXTS, LANE_BASE, ROOT, TOOLCHAIN

TOUR = "docs/user-tour.md"
TOUR_SECTIONS = (
    "Running a Script",
    "Values at a Glance",
    "Commands and argv",
    "Paths",
    "Errors, Results, and `?`",
    "Environment Variables",
    "Records, Lists, and Maps",
    "Streams and Pipelines",
    "JSON Boundaries",
    "Effects",
    "Functions and Inference",
    "Concurrency: Fan Out, Collect",
    "Cleanup with `defer`",
    "Timeouts and Retries",
    "A Real Program",
    "Project Layout",
    "Where to Go Next",
)
HTML_COMMENT = re.compile(r"^<!--.*-->\n\n?", re.MULTILINE)


def tour_at_base() -> str:
    return subprocess.run(
        ["git", "-C", str(ROOT), "show", f"{LANE_BASE}:{TOUR}"], capture_output=True, text=True, check=True,
    ).stdout


def split_sections(markdown: str) -> tuple[str, dict[str, str]]:
    intro: list[str] = []
    sections: dict[str, list[str]] = {}
    current: list[str] = intro
    fenced = False
    for line in markdown.splitlines(keepends=True):
        if line.startswith("```"):
            fenced = not fenced
        if not fenced and line.startswith("## "):
            title = line[3:].strip()
            current = sections.setdefault(title, [])
        current.append(line)
    return "".join(intro), {title: "".join(lines) for title, lines in sections.items()}


def xsht_api(*selectors: str) -> str:
    return subprocess.run(
        [str(TOOLCHAIN / "xsht"), "api", *selectors], capture_output=True, text=True, check=True,
    ).stdout.strip()


def demote(markdown: str) -> str:
    out = []
    fenced = False
    for line in markdown.splitlines(keepends=True):
        if line.startswith("```"):
            fenced = not fenced
        out.append("#" + line if not fenced and line.startswith("#") else line)
    return "".join(out)


def build_baseline() -> str:
    intro, sections = split_sections(tour_at_base())
    missing = [title for title in TOUR_SECTIONS if title not in sections]
    if missing:
        raise ValueError(f"the tour at {LANE_BASE} lacks {missing}")
    tour = HTML_COMMENT.sub("", intro).strip() + "\n\n" + "".join(sections[t] for t in TOUR_SECTIONS).rstrip()
    parts = [
        "# XSH reference material",
        "",
        "## Getting started (`xsht api`)",
        "",
        "```text",
        xsht_api(),
        "```",
        "",
        "## Language rules (`xsht api language:core`)",
        "",
        "```text",
        xsht_api("language:core"),
        "```",
        "",
        demote(HTML_COMMENT.sub("", tour)),
    ]
    return "\n".join(parts).rstrip() + "\n"


def digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def baseline_path() -> Path:
    return CONTEXTS / "baseline.md"
