#!/usr/bin/env python3
"""Regenerate the patch applet's expected results from GNU patch.

    python3 -I core/tests/data/patch/generate.py [CASE...]

Every case below runs once in the reference container (dev/compat/oracle.sh)
inside a copy of its `in/` tree. Standard output, standard error, the exit
status, and the final tree are stored under `<case>/out/`, so the native test
runs the XSH applet only and never needs the container. The image has no
`ed`, so `ed.awk` stands in for the editor commands GNU patch feeds it.

Case layout written to disk:
    args       one argument per line
    stdin      standard input (optional)
    setup      `chmod OCT PATH` and `touch SECONDS PATH` lines (optional)
    mtimes     paths whose modification time is part of the result (optional)
    in/        the starting tree
    out/stdout out/stderr out/status out/manifest out/tree/
               out/tree holds only the files that differ from in/
"""
import os
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
ORACLE = os.path.join(REPO, "dev", "compat", "oracle.sh")

CASES = []


def nums(first, last):
    return "".join("%d\n" % n for n in range(first, last + 1))


def udiff(old, new, a="f", b="f", n=3, a_date="", b_date=""):
    """A unified diff of two texts, headers as given."""
    import difflib
    return "".join(
        difflib.unified_diff(
            old.splitlines(True), new.splitlines(True), a, b, a_date, b_date, n=n
        )
    )


def cdiff(old, new, a="f", b="f", n=3, a_date="", b_date=""):
    """A context diff of two texts, headers as given."""
    import difflib
    return "".join(
        difflib.context_diff(
            old.splitlines(True), new.splitlines(True), a, b, a_date, b_date, n=n
        )
    )


def lines(*items):
    return "".join(item + "\n" for item in items)


def case(name, args="", stdin=None, links=None, dirs=(), chmod=(), touch=(), mtimes=(), env=None, **files):
    """Define a case. Keyword names are file paths with `__` for `/`."""
    CASES.append(
        {
            "name": name,
            "args": shlex.split(args),
            "stdin": stdin,
            "files": {key.replace("__", "/"): value for key, value in files.items()},
            "links": links or {},
            "dirs": list(dirs),
            "chmod": list(chmod),
            "touch": list(touch),
            "mtimes": list(mtimes),
            "env": env or {},
        }
    )


SAFE = set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-/")


def enc(name):
    """A file name as stored in the repository: bytes outside a small safe set
    become %XX, so the repository never holds names that need quoting."""
    out = []
    for char in name:
        if char in SAFE:
            out.append(char)
        else:
            out.extend("%%%02X" % byte for byte in char.encode("utf-8"))
    return "".join(out)


def dec(name):
    import re
    return re.sub(rb"%([0-9A-F]{2})", lambda m: bytes([int(m.group(1), 16)]), name.encode("utf-8")).decode("utf-8")


def as_bytes(value):
    return value if isinstance(value, bytes) else value.encode("utf-8")


def run_case(item, out_root):
    out = os.path.join(out_root, item["name"])
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(os.path.join(out, "in"))
    os.makedirs(os.path.join(out, "out", "tree"))
    with open(os.path.join(out, "args"), "w") as handle:
        handle.write("".join(arg + "\n" for arg in item["args"]))
    if item["stdin"] is not None:
        with open(os.path.join(out, "stdin"), "wb") as handle:
            handle.write(as_bytes(item["stdin"]))
    setup = ["chmod %s %s" % pair for pair in item["chmod"]]
    setup += ["touch %s %s" % pair for pair in item["touch"]]
    if setup:
        with open(os.path.join(out, "setup"), "w") as handle:
            handle.write("".join(line + "\n" for line in setup))
    if item["mtimes"]:
        with open(os.path.join(out, "mtimes"), "w") as handle:
            handle.write("".join(path + "\n" for path in item["mtimes"]))
    if item["env"]:
        with open(os.path.join(out, "env"), "w") as handle:
            handle.write("".join("%s=%s\n" % pair for pair in sorted(item["env"].items())))
    for path, content in item["files"].items():
        full = os.path.join(out, "in", enc(path))
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "wb") as handle:
            handle.write(as_bytes(content))
    for path, target in item["links"].items():
        full = os.path.join(out, "in", enc(path))
        os.makedirs(os.path.dirname(full), exist_ok=True)
        os.symlink(target, full)
    for path in item["dirs"]:
        os.makedirs(os.path.join(out, "in", enc(path)), exist_ok=True)

    work = tempfile.mkdtemp(prefix="patch-case-")
    try:
        shutil.copytree(os.path.join(out, "in"), os.path.join(work, "w"), symlinks=True)
        # Stored names are percent-encoded; the run sees the real names.
        for base, dirs, names in os.walk(os.path.join(work, "w"), topdown=False):
            for entry in dirs + names:
                if "%" in entry:
                    os.rename(os.path.join(base, entry), os.path.join(base, dec(entry)))
        if item["stdin"] is not None:
            shutil.copy(os.path.join(out, "stdin"), os.path.join(work, "stdin"))
        else:
            open(os.path.join(work, "stdin"), "wb").close()
        shutil.copy(os.path.join(HERE, "ed.awk"), os.path.join(work, "ed.awk"))
        script = ["cd /fixture/w"]
        for key, value in sorted(item["env"].items()):
            script.append("export %s=%s" % (key, shlex.quote(value)))
        for mode, path in item["chmod"]:
            script.append("chmod %s %s" % (mode, shlex.quote(path)))
        for seconds, path in item["touch"]:
            script.append("touch -d @%s %s" % (seconds, shlex.quote(path)))
        script.append('patch "$@" < ../stdin > ../stdout 2> ../stderr')
        script.append("echo $? > ../status")
        with open(os.path.join(work, "run.sh"), "w") as handle:
            handle.write("\n".join(script) + "\n")
        subprocess.run(["chmod", "-R", "a+rwX", work], check=True)
        prelude = (
            "umask 022; install -m 755 /dev/null /bin/ed; "
            "printf '#!/bin/sh\\nexec gawk -f /fixture/ed.awk \"$@\"\\n' > /bin/ed; "
            "exec setpriv --reuid=1000 --regid=1000 --clear-groups sh /fixture/run.sh \"$@\""
        )
        subprocess.run(
            [ORACLE, "--mount", work, "--rw", "--root", "--", "sh", "-c", prelude, "sh"] + item["args"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for name in ("stdout", "stderr", "status"):
            shutil.copy(os.path.join(work, name), os.path.join(out, "out", name))
        manifest = []
        wroot = os.path.join(work, "w")
        # Record permissions first, then make everything readable for the copy.
        modes = {}
        for base, dirs, names in os.walk(wroot):
            for entry in dirs + names:
                full = os.path.join(base, entry)
                modes[full] = os.lstat(full).st_mode
        subprocess.run(["chmod", "-R", "u+rwX", wroot], check=False, stderr=subprocess.DEVNULL)
        mtimes = set(item["mtimes"])
        for base, dirs, names in os.walk(wroot):
            for entry in dirs + names:
                full = os.path.join(base, entry)
                rel = os.path.relpath(full, wroot)
                info = os.lstat(full)
                owner = (modes.get(full, info.st_mode) >> 6) & 7
                if os.path.islink(full):
                    manifest.append("l 0 %s -> %s" % (rel, os.readlink(full)))
                elif os.path.isdir(full):
                    manifest.append("d %d %s" % (owner, rel))
                else:
                    line = "f %d %s" % (owner, rel)
                    if rel in mtimes:
                        line += " mtime=%d" % int(info.st_mtime)
                    manifest.append(line)
                    original = os.path.join(out, "in", enc(rel))
                    same = os.path.isfile(original) and open(original, "rb").read() == open(full, "rb").read()
                    if not same:
                        target = os.path.join(out, "out", "tree", enc(rel))
                        os.makedirs(os.path.dirname(target), exist_ok=True)
                        shutil.copy(full, target)
        with open(os.path.join(out, "out", "manifest"), "w") as handle:
            handle.write("".join(line + "\n" for line in sorted(manifest)))
    finally:
        subprocess.run(["chmod", "-R", "u+rwX", work], check=False, stderr=subprocess.DEVNULL)
        shutil.rmtree(work, ignore_errors=True)


exec(open(os.path.join(HERE, "cases.py")).read())

if __name__ == "__main__":
    wanted = set(sys.argv[1:])
    for item in CASES:
        if wanted and item["name"] not in wanted:
            continue
        print("case", item["name"], flush=True)
        run_case(item, HERE)
