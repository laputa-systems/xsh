#!/usr/bin/env python3
"""Check small annotated grammar witnesses with an explicitly selected xsht."""

import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile

SPEC = importlib.util.spec_from_file_location("typing_scaling", Path(__file__).parents[1] / "scaling.py")
scaling = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scaling)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xsht", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.xsht.resolve()
    checks = []
    for family in scaling.FAMILIES:
        files = scaling.generate(family, 3, "annotated")
        with tempfile.TemporaryDirectory(prefix="xsh-typing-scaling-") as root:
            for path, data in files.items():
                target = Path(root) / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            command = [str(binary), "check", str(Path(root) / "entry.xsh")]
            result = subprocess.run(command, env={**os.environ, "XSH_MODULE_PATH": root},
                                    capture_output=True, text=True, timeout=30)
            checks.append({"family": family, "units": 3, "variant": "annotated",
                           "source_index_sha256": scaling.sha(scaling.encoded(scaling.source_index(files))),
                           "command": command, "environment": {"XSH_MODULE_PATH": root},
                           "exit_status": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    report = {"schema_version": 1,
              "purpose": "Small annotated grammar witnesses only; no full-size scaling or inferred-semantic pass is claimed.",
              "reproduce": ["python3", "bench/typing/scaling/check_grammar.py", "--xsht", str(binary), "--output", "{new_report_path}"],
              "generator_sha256": scaling.sha(Path(scaling.__file__).read_bytes()),
              "binary": {"path": str(binary), "sha256": scaling.sha(binary.read_bytes())},
              "checks": checks}
    args.output.write_bytes(scaling.encoded(report))
    failures = sum(check["exit_status"] != 0 for check in checks)
    print(f"Annotated grammar witnesses: {len(checks) - failures}/{len(checks)} accepted.")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
