from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import tempfile
from unittest.mock import patch

root = Path(__file__).resolve().parents[2]
spec = spec_from_file_location("scheduler_bridge", Path(__file__).with_name("final_scheduler_receipt_bridge.py"))
bridge = module_from_spec(spec)
spec.loader.exec_module(bridge)


def test_reference_runner_receives_and_records_its_tool_directory():
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as temporary:
        source = Path(temporary)
        runner = source / "runner.py"
        runner.write_text('''#!/usr/bin/env python3
import os
from pathlib import Path
import sys
expected = Path(__file__).parent / "dev/consolidation/winddown/gate-tools/final-correctness-gates"
sys.exit(77 if os.environ.get("XSH_GATE_TOOL_DIR") == str(expected) else 78)
''')
        runner.chmod(0o700)
        build_root = source / ".work/build"
        reference_run = source / ".work/reference"
        with patch.multiple(bridge, create=True, ROOT=source, BUILD_ROOT=build_root, REFERENCE_RUN=reference_run, REFERENCE_ROOT=source / "reference", RUNNER=runner), patch.object(bridge, "load_candidate", return_value=({}, {}, None)), patch.object(bridge, "validate_reference_inputs", return_value={}), patch.dict(os.environ, {"XSH_GATE_TOOL_DIR": "unrelated-ambient-tools"}):
            try:
                bridge.reference_build(object())
            except ValueError as error:
                assert "reference Cargo build exited 77" in str(error), str(error)
            else:
                raise AssertionError("reference build accepted the runner failure")
        record = json.loads((build_root / "reference-build-process.json").read_text())
        assert record["runner_environment"]["XSH_GATE_TOOL_DIR"] == str(source / "dev/consolidation/winddown/gate-tools/final-correctness-gates")
        assert record["returncode"] == 77


if __name__ == "__main__":
    test_reference_runner_receives_and_records_its_tool_directory()
    print("scheduler reference runner receives and records the reviewed tool directory")
