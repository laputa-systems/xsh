#!/usr/bin/env python3
"""Dev scoreboard for showcase/jq.xsh against jq's own upstream test corpus.

Mirrors jq's `--run-tests` (src/jq_test.c): outputs are compared by *value*
(`jv_equal`), not textually — so `19.0` and `19` match, and object key order is
ignored. This is the per-tranche gate: run it before/after each tranche and watch
the pass count rise with no regressions.

Usage:
  showcase/tests/jq-score.py [TESTFILE] [--bin PATH] [--start N] [--end N]
                             [--show-fails N] [--list-fails]
Defaults: TESTFILE=~/d/jq/tests/jq.test, bin=target/debug/xsh
"""
import argparse, json, os, subprocess, sys
from pathlib import Path

def parse_tests(path):
    """Yield (lineno, fail_mode, program, input_str, [expected_lines])."""
    lines = Path(path).read_text().splitlines()
    i, n = 0, len(lines)
    while i < n:
        line = lines[i]
        if not line.strip() or line.lstrip().startswith("#"):
            i += 1
            continue
        fail_mode = None
        if line.strip() in ("%%FAIL", "%%FAIL IGNORE MSG"):
            fail_mode = "ignore" if "IGNORE" in line else "msg"
            i += 1
        prog_line = i + 1
        program = lines[i]; i += 1
        if i >= n: break
        input_str = lines[i]; i += 1
        expected = []
        while i < n and lines[i].strip() != "" and not lines[i].lstrip().startswith("#"):
            expected.append(lines[i]); i += 1
        yield (prog_line, fail_mode, program, input_str, expected)

def jv_equal(a, b):
    """Structural value equality matching jq's jv_equal (numeric, order-insensitive objects)."""
    if isinstance(a, bool) or isinstance(b, bool):
        return a is b
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return float(a) == float(b)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(jv_equal(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(jv_equal(a[k], b[k]) for k in a)
    return a == b

def run_case(bin_path, program, input_str):
    """Return (exit_code, stdout_lines, stderr)."""
    try:
        p = subprocess.run(
            [bin_path, "showcase/jq.xsh", "--", "-c", program],
            input=input_str + "\n", capture_output=True, text=True, timeout=5)
        out = p.stdout.splitlines()
        return (p.returncode, out, p.stderr)
    except subprocess.TimeoutExpired:
        return (-1, [], "TIMEOUT")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("testfile", nargs="?",
                    default=os.path.expanduser("~/d/jq/tests/jq.test"))
    ap.add_argument("--bin", default="target/debug/xsh")
    ap.add_argument("--start", type=int, default=0)
    ap.add_argument("--end", type=int, default=10**9)
    ap.add_argument("--show-fails", type=int, default=0)
    ap.add_argument("--list-fails", action="store_true")
    args = ap.parse_args()

    passed = total = 0
    fails = []
    for lineno, fail_mode, program, input_str, expected in parse_tests(args.testfile):
        if lineno < args.start or lineno > args.end:
            continue
        total += 1
        code, out, err = run_case(args.bin, program, input_str)
        ok = False
        if fail_mode is not None:
            ok = code != 0  # %%FAIL: program must error (compile or runtime)
        else:
            # Compare actual vs expected output values.
            if code == 0 and len(out) == len(expected):
                ok = True
                for a_line, e_line in zip(out, expected):
                    try:
                        av, ev = json.loads(a_line), json.loads(e_line)
                        if not jv_equal(av, ev):
                            ok = False; break
                    except (json.JSONDecodeError, ValueError):
                        if a_line != e_line:  # raw/-r or non-JSON expected
                            ok = False; break
        if ok:
            passed += 1
        else:
            fails.append((lineno, program, input_str, expected, out, err, code))

    if args.list_fails:
        for lineno, prog, _, _, _, _, _ in fails:
            print(f"L{lineno}: {prog}")
    if args.show_fails:
        for lineno, prog, inp, exp, out, err, code in fails[:args.show_fails]:
            print(f"--- L{lineno} (exit {code}) ---")
            print(f"  prog : {prog}")
            print(f"  input: {inp}")
            print(f"  want : {exp}")
            print(f"  got  : {out}")
            if err.strip():
                print(f"  err  : {err.strip().splitlines()[0] if err.strip() else ''}")
    print(f"{passed} / {total} passed ({total - passed} failed)")

if __name__ == "__main__":
    main()
