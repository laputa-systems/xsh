#!/usr/bin/env python3
"""Regenerate cases.json for test-awk.xsh from the GNU awk oracle.

    python3 core/tests/data/awk/regenerate.py generate   # run gawk in the oracle container, rewrite cases.json
    python3 core/tests/data/awk/regenerate.py check [-v] [NAME...]   # run the XSH applet, compare with cases.json
    python3 core/tests/data/awk/regenerate.py groups     # rewrite the native test's grouped fixture files
    python3 core/tests/data/awk/regenerate.py check-groups  # verify exact grouped fixture and shard coverage

The case definitions live in cases_src.py. `generate` needs docker and the
xsh-oracle image; `check`, `groups`, `check-groups`, and the native test need
neither. Expected output is whatever gawk 5.3.2 (default mode, UTF-8 locale)
produced: the temporary directory is rendered as @DIR@ and every file operand
or argument spelled @DIR@/name is substituted before the run.
"""
import base64
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
ORACLE = os.path.join(ROOT, "dev", "compat", "oracle.sh")
XSH = os.environ.get("XSH_BIN", os.path.join(ROOT, "target", "release", "xsh"))

MATRIX_SHARD_SIZE = 8
MATRIX_GROUP_PREFIXES = {
    "patterns_and_records": ("pat_", "rs_"),
    "fields_and_separators": ("fld_", "fs_"),
    "getline": ("get_",),
    "printf": ("printf",),
    "strings": ("str_",),
    "numbers": ("num_",),
    "arrays_and_functions": ("arr_", "fn_"),
    "streams_and_processes": ("io_",),
    "options_and_uninitialized": ("opt_",),
    "grammar": ("gram_",),
    "regex": ("rx_", "rxdyn_"),
    "for_in_order_and_non_utf8_bytes": ("ord_", "bin_"),
    "lexical_forms": ("lex_",),
    "special_variables": ("sv_",),
    "declaration_and_newline_diagnostics": ("diag_declname_", "diag_newline_in_"),
    "rule_and_regexp_constant_diagnostics": ("diag_rule_noaction_", "diag_regexconst_", "diag_string_end_"),
    "source_end_diagnostics": ("diag_source_end_", "file_syntax_"),
    "untyped_variables": ("typing_",),
    "assignment_targets": ("assign_", "field_assign_"),
    "arithmetic_and_bit_functions": ("arith_",),
    "builtin_argument_counts": ("arity_",),
    "regexp_syntax_first_half": ("regexp_syntax_0", "regexp_syntax_1"),
    "regexp_syntax_second_half": tuple(f"regexp_syntax_{index}" for index in range(2, 10)),
    "gnu_variables_and_field_modes": ("gnu_",),
    "program_files_and_includes": ("include_", "option_"),
    "dialect_options": ("mode_",),
}


def matrix_groups(cases):
    groups = {name: [] for name in MATRIX_GROUP_PREFIXES}
    owners = {}
    for case in cases:
        name = case["name"]
        matches = [
            group
            for group, prefixes in MATRIX_GROUP_PREFIXES.items()
            if any(name.startswith(prefix) for prefix in prefixes)
        ]
        if len(matches) != 1:
            raise ValueError(f"matrix case {name!r} belongs to {len(matches)} groups")
        if name in owners:
            raise ValueError(f"duplicate matrix case {name!r}")
        owners[name] = matches[0]
        groups[matches[0]].append(case)
    return groups


def matrix_group_directory():
    return os.path.join(HERE, "matrix")


def write_matrix_groups(cases):
    groups = matrix_groups(cases)
    directory = matrix_group_directory()
    os.makedirs(directory, exist_ok=True)
    for group, rows in groups.items():
        with open(os.path.join(directory, group + ".json"), "w") as handle:
            json.dump(rows, handle, ensure_ascii=False, separators=(",", ":"))
            handle.write("\n")
    return groups


def check_matrix_groups(cases=None):
    if cases is None:
        with open(os.path.join(HERE, "cases.json")) as handle:
            cases = json.load(handle)
    expected = matrix_groups(cases)
    directory = matrix_group_directory()
    for group, rows in expected.items():
        path = os.path.join(directory, group + ".json")
        if not os.path.exists(path):
            raise SystemExit(f"missing AWK matrix group file: {path}")
        with open(path) as handle:
            actual = json.load(handle)
        if actual != rows:
            raise SystemExit(f"AWK matrix group {group} differs from cases.json")

    source_path = os.path.join(HERE, "..", "..", "test-awk.xsh")
    with open(source_path) as handle:
        source = handle.read()
    declarations = re.findall(
        r'^test test_awk_matrix_(.+)_shard_(\d{3}) \{ \|ctx\| '
        r'check_matrix_shard\(ctx, "([a-z0-9_]+)", (\d+)\) \}$',
        source,
        re.MULTILINE,
    )
    declared = {}
    for test_group, test_shard, call_group, call_shard in declarations:
        if test_group != call_group or int(test_shard) != int(call_shard):
            raise SystemExit(f"matrix test name and shard call disagree: {test_group}/{test_shard}")
        declared.setdefault(call_group, []).append(int(call_shard))
    wanted = {
        group: list(range((len(rows) + MATRIX_SHARD_SIZE - 1) // MATRIX_SHARD_SIZE))
        for group, rows in expected.items()
    }
    if declared != wanted:
        raise SystemExit(f"matrix test shards differ from fixture coverage: {declared!r}")
    return sum(len(rows) for rows in expected.values())


def generate_matrix_groups():
    with open(os.path.join(HERE, "cases.json")) as handle:
        cases = json.load(handle)
    groups = write_matrix_groups(cases)
    print(f"wrote {len(groups)} AWK matrix groups for {sum(map(len, groups.values()))} fixtures")


def audit_matrix_groups():
    fixture_count = check_matrix_groups()
    print(f"verified {fixture_count} AWK matrix fixtures across exact shard coverage")
AWK = os.path.join(ROOT, "core", "awk.xsh")
sys.path.insert(0, HERE)


def encode(data):
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return {"b64": base64.b64encode(data).decode()}


def decode(value):
    if isinstance(value, dict):
        return base64.b64decode(value["b64"])
    return value.encode("utf-8")


def stage(case, directory, root):
    for name, content in case.get("files", {}).items():
        path = os.path.join(directory, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(content if isinstance(content, bytes) else decode(content))
    for name, mode in case.get("modes", {}).items():
        os.chmod(os.path.join(directory, name), mode)
    for name, target in case.get("links", {}).items():
        os.symlink(target, os.path.join(directory, name))
    for name in case.get("dirs", []):
        os.makedirs(os.path.join(directory, name), exist_ok=True)
    stdin = case.get("stdin")
    with open(os.path.join(directory, ".stdin"), "wb") as handle:
        handle.write(stdin if isinstance(stdin, bytes) else (decode(stdin) if stdin is not None else b""))
    return [arg.replace("@DIR@", root) for arg in case["args"]]


def run_oracle(case):
    directory = tempfile.mkdtemp(prefix="awkcase")
    try:
        os.chmod(directory, 0o777)
        args = stage(case, directory, "/fixture")
        script = (
            'ln -s /usr/bin/gawk /tmp/awk; export LC_ALL=C.UTF-8; ulimit -f 20000; '
            + "".join(f'export {k}={shell_quote(v)}; ' for k, v in case.get("env", {}).items())
            + 'timeout 20 /tmp/awk "$@" < .stdin > /tmp/out 2> /tmp/err; echo $? > /tmp/st; '
            "cp /tmp/out /fixture/.out; cp /tmp/err /fixture/.err; cp /tmp/st /fixture/.st"
        )
        subprocess.run([ORACLE, "--mount", directory, "--rw", "--", "sh", "-c", script, "sh"] + args,
                       stdin=subprocess.DEVNULL, capture_output=True, check=False)
        def get(name):
            with open(os.path.join(directory, name), "rb") as handle:
                return handle.read()
        out, err, st = get(".out"), get(".err"), int(get(".st").strip() or b"0")
        return out.replace(b"/fixture", b"@DIR@"), err.replace(b"/fixture", b"@DIR@"), st, directory
    except FileNotFoundError:
        return None
    finally:
        shutil.rmtree(directory, ignore_errors=True)


def shell_quote(text):
    return "'" + text.replace("'", "'\\''") + "'"


def run_xsh(case):
    directory = tempfile.mkdtemp(prefix="awkcase")
    try:
        args = stage(case, directory, directory)
        env = dict(os.environ)
        env.update(case.get("env", {}))
        with open(os.path.join(directory, ".stdin"), "rb") as stdin:
            done = subprocess.run([XSH, AWK, "--"] + args, stdin=stdin, capture_output=True, cwd=directory, env=env, timeout=30,
                                  preexec_fn=lambda: __import__("resource").setrlimit(__import__("resource").RLIMIT_FSIZE, (20000000, 20000000)))
        d = directory.encode()
        return done.stdout.replace(d, b"@DIR@"), done.stderr.replace(d, b"@DIR@"), done.returncode
    except subprocess.TimeoutExpired:
        return b"", b"TIMEOUT", 124
    finally:
        shutil.rmtree(directory, ignore_errors=True)


def load_cases():
    import cases_src
    names = set()
    for case in cases_src.CASES:
        assert case["name"] not in names, case["name"]
        names.add(case["name"])
        if isinstance(case.get("stdin"), bytes):
            case["stdin"] = encode(case["stdin"])
        if "files" in case:
            case["files"] = {k: encode(v) if isinstance(v, bytes) else v for k, v in case["files"].items()}
    return cases_src.CASES


def split_text(data):
    """A (text, base64) pair: text when valid UTF-8, otherwise the base64 form."""
    try:
        return data.decode("utf-8"), ""
    except UnicodeDecodeError:
        return "", base64.b64encode(data).decode()


def as_bytes(case, name):
    """Bytes of a text/base64 field pair of a normalized case."""
    if case.get(name + "_b64"):
        return base64.b64decode(case[name + "_b64"])
    return case.get(name, "").encode("utf-8")


def normalize(case, stdout, stderr, status):
    """The committed shape of one case: every field present, typed uniformly."""
    record = {"name": case["name"], "args": case["args"]}
    stdin = case.get("stdin")
    stdin_bytes = decode(stdin) if stdin is not None else b""
    record["stdin"], record["stdin_b64"] = split_text(stdin_bytes)
    files, files_b64 = {}, {}
    for name, content in case.get("files", {}).items():
        text, packed = split_text(decode(content))
        if packed:
            files_b64[name] = packed
        else:
            files[name] = text
    # Mappings are written as lists of [key, value] pairs, the shape the
    # native test reads as plain typed lists.
    record["files"], record["files_b64"] = [[k, v] for k, v in files.items()], [[k, v] for k, v in files_b64.items()]
    record["modes"] = [[k, str(v)] for k, v in case.get("modes", {}).items()]
    record["links"] = [[k, v] for k, v in case.get("links", {}).items()]
    record["dirs"] = case.get("dirs", [])
    record["env"] = [[k, v] for k, v in case.get("env", {}).items()]
    record["stdout"], record["stdout_b64"] = split_text(stdout)
    record["stderr"], record["stderr_b64"] = split_text(stderr)
    record["status"] = status
    # Documented deviations: what this implementation prints instead, and the
    # channels deliberately not compared.
    record["xsh"] = [[k, str(v)] for k, v in case.get("xsh", {}).items()]
    record["loose"] = case.get("loose", [])
    return record


def case_key(case):
    """What determines the oracle's answer for a case, to reuse a recorded one."""
    fixed = normalize(case, b"", b"", 0)
    return json.dumps([fixed["args"], fixed["stdin"], fixed["stdin_b64"], fixed["files"], fixed["files_b64"],
                       fixed["modes"], fixed["links"], fixed["dirs"], fixed["env"]], sort_keys=True)


def recorded_results():
    path = os.path.join(HERE, "cases.json")
    recorded = {}
    if os.path.exists(path):
        for old in json.load(open(path)):
            raw = {"name": old["name"], "args": old["args"], "stdin": {"b64": old["stdin_b64"]} if old["stdin_b64"] else old["stdin"],
                   "files": {k: v for k, v in old["files"]}, "modes": {k: int(v) for k, v in old["modes"]},
                   "links": dict(old["links"]), "dirs": old["dirs"], "env": dict(old["env"])}
            for k, v in old["files_b64"]:
                raw["files"][k] = {"b64": v}
            recorded[case_key(raw)] = (as_bytes(old, "stdout"), as_bytes(old, "stderr"), old["status"], None)
    return recorded


def generate():
    cases = load_cases()
    stable = "--stable" in sys.argv
    recorded = recorded_results() if "--reuse" in sys.argv else {}
    todo = [c for c in cases if case_key(c) not in recorded]
    with concurrent.futures.ThreadPoolExecutor(6) as pool:
        fresh = list(pool.map(run_oracle, todo))
        again = list(pool.map(run_oracle, todo)) if stable else fresh
    answers = {}
    for case, result, second_run in zip(todo, fresh, again):
        if result is not None and stable and result[:3] != second_run[:3]:
            print("unstable oracle result, case dropped from the matrix:", case["name"])
            continue
        answers[case_key(case)] = result
    results = [recorded.get(case_key(c)) or answers.get(case_key(c), "dropped") for c in cases]
    second = results
    out = []
    for case, result in zip(cases, results):
        if result == "dropped":
            continue
        if result is None:
            sys.exit("oracle failed for " + case["name"])
        stdout, stderr, status, _ = result
        out.append(normalize(case, stdout, stderr, status))
    path = os.path.join(HERE, "cases.json")
    with open(path, "w") as handle:
        json.dump(out, handle, indent=None, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
    write_matrix_groups(out)
    print(f"wrote {len(out)} cases")


def expected_channel(case, channel):
    override = dict(case["xsh"]).get(channel)
    if override is not None:
        return override.encode("utf-8") if channel != "status" else int(override)
    if channel == "status":
        return case["status"]
    return as_bytes(case, channel)


def check():
    verbose = "-v" in sys.argv
    wanted = [a for a in sys.argv[2:] if a != "-v"]
    cases = json.load(open(os.path.join(HERE, "cases.json")))
    check_matrix_groups(cases)
    if wanted:
        cases = [c for c in cases if any(w in c["name"] for w in wanted)]
    runnable = []
    for case in cases:
        raw = dict(case)
        raw["stdin"] = case["stdin"].encode("utf-8") if not case["stdin_b64"] else base64.b64decode(case["stdin_b64"])
        merged = {}
        for name, text in case["files"]:
            merged[name] = text
        for name, packed in case["files_b64"]:
            merged[name] = {"b64": packed}
        raw["files"] = merged
        raw["modes"] = {k: int(v) for k, v in case["modes"]}
        raw["links"] = dict(case["links"])
        raw["env"] = dict(case["env"])
        runnable.append(raw)
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        results = list(pool.map(run_xsh, runnable))
    bad = 0
    for case, (out, err, st) in zip(cases, results):
        problems = []
        if "stdout" not in case["loose"] and out != expected_channel(case, "stdout"):
            problems.append("stdout")
        if "stderr" not in case["loose"] and err != expected_channel(case, "stderr"):
            problems.append("stderr")
        if st != expected_channel(case, "status"):
            problems.append("status")
        if problems:
            bad += 1
            print(f"FAIL {case['name']}: {','.join(problems)}")
            if verbose:
                print("  args:", case["args"])
                print("  stdin:", repr(case["stdin"]))
                if "stdout" in problems:
                    print("  want out:", expected_channel(case, "stdout"))
                    print("  got  out:", out)
                if "stderr" in problems:
                    print("  want err:", expected_channel(case, "stderr"))
                    print("  got  err:", err)
                if "status" in problems:
                    print("  want st:", expected_channel(case, "status"), "got", st)
    print(f"{len(cases) - bad}/{len(cases)} match")


if __name__ == "__main__":
    {"generate": generate, "check": check, "groups": generate_matrix_groups, "check-groups": audit_matrix_groups}[sys.argv[1]]()
