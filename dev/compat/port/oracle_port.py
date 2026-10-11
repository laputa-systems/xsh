#!/usr/bin/env python3
"""Run unchanged native ports against GNU or BusyBox in the oracle image.

    XSH_BIN=/path/to/release/xsh python3 dev/compat/port/oracle_port.py \
        [--reference gnu|busybox] core/tests/test-uu-basename.xsh

Only a temporary copy of support/uu.xsh changes its invocation target. Test
assertions, inputs and fixtures are copied unchanged. Select only ports whose
applet launches use this helper. Logs remain in .work.
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import uuid


REPO = Path(__file__).resolve().parents[3]


def rewrite_helper(source: str, reference: str) -> str:
    if reference not in ("gnu", "busybox"):
        raise ValueError(f"unknown reference: {reference}")
    script = '  let script = fp"{s.ctx.core_dir}/{util}.xsh"'
    text_argv = '  let words = [s.ctx.xsh_bin.display(), script.display()].extend(args)'
    path_argv = '  let words = [s.ctx.xsh_bin, script].extend(args)'
    command = 'process.command_argv(s.ctx.xsh_bin, argv,'
    for fragment, expected in ((script, 3), (text_argv, 2), (path_argv, 1), (command, 3)):
        actual = source.count(fragment)
        if actual != expected:
            raise ValueError(f"uu helper launch shape changed: expected {expected} occurrences of {fragment!r}, got {actual}")
    if reference == "gnu":
        target = '  let executable = process.which(util)?'
        text_words = '[executable.display()]'
        path_words = '[executable]'
    else:
        target = '  let executable = p"/bin/busybox"'
        text_words = '[executable.display(), util]'
        path_words = '[executable, fp"{util}"]'
    return (source.replace(script, target)
        .replace(text_argv, f'  let words = {text_words}.extend(args)')
        .replace(path_argv, f'  let words = {path_words}.extend(args)'))


def prepare_corpus(repo: Path, destination: Path, files: list[Path], reference: str) -> list[str]:
    selected = []
    for file in files:
        resolved = file.resolve(strict=True)
        relative = resolved.relative_to(repo.resolve())
        if relative.parent != Path("core/tests") or relative.suffix != ".xsh":
            raise ValueError(f"expected a native test file directly in core/tests: {file}")
        if "use support.uu" not in resolved.read_text():
            raise ValueError(f"oracle selection must use support.uu for applet launches: {file}")
        selected.append(relative)
    tests = destination / "core/tests"
    tests.mkdir(parents=True)
    shutil.copytree(repo / "core/tests/support", tests / "support", symlinks=True)
    shutil.copytree(repo / "core/tests/data", tests / "data", symlinks=True)
    helper = tests / "support/uu.xsh"
    helper.write_text(rewrite_helper(helper.read_text(), reference))
    for relative in selected:
        shutil.copy2(repo / relative, destination / relative)
    (destination / "xsht-config.ini").write_text(
        "module_path = .\ntest_roots = core/tests\n"
        "exclude = core/tests/data/**/*.xsh\n  core/tests/support/**/*.xsh\n"
    )
    return [str(relative) for relative in selected]


def docker_command(corpus: Path, binaries: Path, name: str) -> list[str]:
    return [
        "docker", "run", "--rm", "--init", "--name", name,
        "--platform", "linux/amd64", "--network", "none", "--user", "1000:1000",
        "--memory", "1g", "--memory-swap", "1g",
        "--tmpfs", "/tmp:rw,exec,nosuid,mode=1777,size=256m",
        "-v", f"{corpus}:/corpus:ro", "-v", f"{binaries}:/release:ro",
        "-w", "/corpus", "-e", "HOME=/tmp", "-e", "TMPDIR=/tmp",
        "-e", "LC_ALL=C", "-e", "TZ=UTC", "-e", "XSH_BIN=/release/xsh",
        "-e", "PATH=/release:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        os.environ.get("XSH_ORACLE_IMAGE", "xsh-oracle"), "/release/xsht", "test", "-j", "1",
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", choices=("gnu", "busybox"), default="gnu")
    parser.add_argument("files", type=Path, nargs="+")
    args = parser.parse_args()
    xsh = Path(os.environ["XSH_BIN"]).resolve(strict=True)
    binaries = xsh.parent
    if xsh.name != "xsh" or "release" not in binaries.parts:
        raise ValueError("XSH_BIN must name a release xsh binary")
    for binary in (xsh, binaries / "xsht"):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise ValueError(f"missing executable: {binary}")
    work = REPO / ".work/oracle-port"
    work.mkdir(parents=True, exist_ok=True)
    name = f"xsh-oracle-port-{uuid.uuid4().hex}"
    log_path = work / f"{name}.log"
    with tempfile.TemporaryDirectory(prefix=f"{name}-", dir=work) as temporary:
        corpus = Path(temporary)
        # Docker's unprivileged account must traverse the host-created directory.
        corpus.chmod(0o755)
        prepare_corpus(REPO, corpus, args.files, args.reference)
        print(f"{args.reference} oracle log: {log_path}", flush=True)
        with log_path.open("w") as log:
            process = None
            try:
                process = subprocess.Popen(
                    docker_command(corpus, binaries, name),
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                )
                for line in process.stdout:
                    log.write(line)
                    log.flush()
                    print(line, end="", flush=True)
                return process.wait()
            finally:
                # Remove only this run's container, including after interruption.
                cleanup = subprocess.run(["docker", "rm", "-f", name], capture_output=True, text=True)
                if process is not None:
                    process.wait()
                if cleanup.returncode and "No such container" not in cleanup.stderr:
                    raise RuntimeError(f"oracle container cleanup failed: {cleanup.stderr.strip()}")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"oracle_port: {error}", file=sys.stderr)
        sys.exit(2)
