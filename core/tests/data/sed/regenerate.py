#!/usr/bin/env python3
"""Regenerate or check the sed differential tables.

The expected transcript is produced by GNU sed in the xsh-oracle container; the
native test reads cases.jsonl, fixtures/ and expected.txt and never needs it.

  regen.py oracle        write expected.txt from the oracle
  regen.py xsh           run the XSH applet and diff against expected.txt
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.environ.get("XSH_REPO", os.path.abspath(os.path.join(HERE, "../../../..")))
sys.path.insert(0, HERE)
from cases import FIXTURES, CASES  # noqa: E402


def esc(data):
    out = []
    for b in data:
        if b == 10:
            out.append("\\n")
        elif b == 92:
            out.append("\\\\")
        elif b == 39:
            out.append("\\'")
        elif 32 <= b < 127:
            out.append(chr(b))
        else:
            out.append("\\x%02x" % b)
    return "".join(out)


def write_fixtures(root):
    os.makedirs(root, exist_ok=True)
    for name, data in FIXTURES.items():
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)


def q(s):
    return "'" + s.replace("'", "'\\''") + "'"


# The same script drives both implementations: only the `sed` found on PATH differs.
def run_script(cases, guard=""):
    lines = ['fx=$1; out=$2; export LC_ALL=C', 'mkdir -p "$out"']
    for i, case in enumerate(cases):
        n = case["name"]
        lines.append('rm -rf "$out/w"; cp -R "$fx" "$out/w"; cd "$out/w"')
        lines.append("ln -s a.txt link.txt; ln -s nowhere dangling; ln -s d dirlink; chmod 000 noaccess.txt; chmod 444 ro.txt; chmod 555 rodir")
        for extra in case.get("setup", []):
            lines.append(extra)
        stdin = case.get("stdin")
        redirect = "< " + q(stdin) if stdin else "< /dev/null"
        env = "".join("%s=%s " % (k, q(v)) for k, v in case.get("env", {}).items())
        cmd = "%senv %s sed %s" % (guard, env, " ".join(q(a) for a in case["args"]))
        lines.append('%s %s > "$out/%d.o" 2> "$out/%d.e"; echo $? > "$out/%d.s"' % (cmd, redirect, i, i, i))
        for k, f in enumerate(case.get("show", [])):
            base = '"$out/%d.f%d"' % (i, k)
            lines.append(
                'if [ -L %s ]; then echo L > %s.k; readlink %s > %s; '
                'elif [ -d %s ]; then echo D > %s.k; : > %s; '
                'elif [ -f %s ]; then echo F > %s.k; cat %s > %s 2>/dev/null; stat -c %%a %s > %s.m; '
                'else echo A > %s.k; : > %s; fi'
                % (q(f), base, q(f), base, q(f), base, base, q(f), base, q(f), base, q(f), base, base, base)
            )
        lines.append('chmod -R u+rwx "$out/w" 2>/dev/null; cd "$out"')
    return "\n".join(lines) + "\n"


def transcript(cases, out):
    chunks = []
    for i, case in enumerate(cases):
        def rd(name, mode="rb"):
            with open(os.path.join(out, name), mode) as f:
                return f.read()
        status = int(rd("%d.s" % i).decode().strip() or "-1")
        stdout = rd("%d.o" % i)
        stderr = re.sub(rb"sed[A-Za-z0-9]{6}", b"sedXXXXXX", rd("%d.e" % i))
        lines = ["case %s" % case["name"], "status %d" % status, "stdout '%s'" % esc(stdout), "stderr '%s'" % esc(stderr)]
        for k, f in enumerate(case.get("show", [])):
            kind = rd("%d.f%d.k" % (i, k)).decode().strip()
            if kind == "A":
                lines.append("file %s absent" % f)
            elif kind == "D":
                lines.append("file %s directory" % f)
            elif kind == "L":
                lines.append("file %s symlink '%s'" % (f, esc(rd("%d.f%d" % (i, k)).rstrip(b"\n"))))
            else:
                mode = rd("%d.f%d.m" % (i, k)).decode().strip()
                lines.append("file %s mode %s '%s'" % (f, mode, esc(rd("%d.f%d" % (i, k)))))
        chunks.append("\n".join(lines) + "\n")
    return "".join(chunks)


def run_oracle(work, cases):
    mnt = os.path.join(work, "mnt")
    write_fixtures(os.path.join(mnt, "fx"))
    with open(os.path.join(mnt, "run.sh"), "w") as f:
        f.write(run_script(cases, guard="timeout 10 "))
    subprocess.check_call(["chmod", "-R", "a+rwX", mnt])
    subprocess.check_call(
        [os.path.join(REPO, "dev/compat/oracle.sh"), "--mount", mnt, "--rw", "--", "sh", "/fixture/run.sh", "/fixture/fx", "/fixture/out"]
    )
    return transcript(cases, os.path.join(mnt, "out"))


def run_xsh(work, cases):
    mnt = os.path.join(work, "xmnt")
    write_fixtures(os.path.join(mnt, "fx"))
    script = os.path.join(mnt, "run.sh")
    with open(script, "w") as f:
        f.write(run_script(cases))
    bindir = os.path.join(work, "bin")
    os.makedirs(bindir, exist_ok=True)
    xsh = os.environ["XSH_BIN"]
    app = os.path.join(REPO, "core/sed.xsh")
    with open(os.path.join(bindir, "sed"), "w") as f:
        f.write("#!/bin/sh\nexec perl -e 'alarm 30; exec @ARGV' %s %s -- \"$@\"\n" % (q(xsh), q(app)))
    os.chmod(os.path.join(bindir, "sed"), 0o755)
    env = dict(os.environ, PATH=bindir + ":" + os.environ["PATH"])
    out = os.path.join(mnt, "out")
    subprocess.check_call(["sh", script, os.path.join(mnt, "fx"), out], env=env)
    return transcript(cases, out)


def select(cases, pattern):
    return [c for c in cases if re.search(pattern, c["name"])] if pattern else cases


def runnable(cases):
    return [c for c in cases if "skip" not in c]


def main():
    mode = sys.argv[1]
    pattern = sys.argv[2] if len(sys.argv) > 2 else None
    cases = select(CASES, pattern)
    work = tempfile.mkdtemp()
    try:
        if mode == "oracle":
            text = run_oracle(work, cases)
            if pattern:
                print(text)
            else:
                with open(os.path.join(HERE, "expected.txt"), "w") as f:
                    f.write(text)
                with open(os.path.join(HERE, "cases.jsonl"), "w") as f:
                    for c in CASES:
                        row = {
                            "name": c["name"],
                            "args": c["args"],
                            "stdin": c.get("stdin", ""),
                            "show": c.get("show", []),
                            "env": ["%s=%s" % kv for kv in sorted(c.get("env", {}).items())],
                            "setup": c.get("setup", []),
                            "skip": c.get("skip", ""),
                        }
                        f.write(json.dumps(row, ensure_ascii=True) + "\n")
                fx = os.path.join(HERE, "fixtures")
                shutil.rmtree(fx, ignore_errors=True)
                write_fixtures(fx)
        elif mode == "xsh":
            cases = runnable(cases)
            actual = run_xsh(work, cases)
            if pattern and os.environ.get("SHOW"):
                print(actual)
            with open(os.path.join(HERE, "expected.txt")) as f:
                expected = f.read()
            table = {}
            for chunk in expected.split("case ")[1:]:
                table[chunk.split("\n", 1)[0]] = "case " + chunk
            bad = 0
            for chunk in actual.split("case ")[1:]:
                name = chunk.split("\n", 1)[0]
                if table.get(name) != "case " + chunk:
                    bad += 1
                    print("DIFF", name)
                    if os.environ.get("VERBOSE"):
                        print(" expected:", table.get(name))
                        print(" actual:  ", "case " + chunk)
            print("%d cases, %d differ" % (len(cases), bad))
    finally:
        shutil.rmtree(work, ignore_errors=True)


main()
