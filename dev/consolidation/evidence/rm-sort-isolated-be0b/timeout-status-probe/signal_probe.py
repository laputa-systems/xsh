import pathlib
import signal
import subprocess
import sys
import time

xsh, timeout_script, probe = sys.argv[1:]
process = subprocess.Popen(
    [xsh, timeout_script, "5s", "/bin/sleep", "3"],
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
time.sleep(0.2)
process.send_signal(signal.SIGTERM)
stdout, stderr = process.communicate(timeout=2)
root = pathlib.Path(probe)
(root / "timeout-signal.stdout").write_bytes(stdout)
(root / "timeout-signal.stderr").write_bytes(stderr)
(root / "timeout-signal.status").write_text(f"{process.returncode}\n")
