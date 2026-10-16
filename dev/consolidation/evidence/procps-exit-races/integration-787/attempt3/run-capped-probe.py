import os
import signal
import subprocess
import sys

output = "/scratch"
command = ["/bin/sh", "/scratch/proc-churn-probe.sh"]
with open(f"{output}/churn-probe.argv.txt", "w", encoding="utf-8") as file:
    file.write("/bin/sh /scratch/proc-churn-probe.sh\n")
process = subprocess.Popen(
    command,
    cwd="/work",
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    start_new_session=True,
)
timed_out = False
try:
    stdout, stderr = process.communicate(timeout=22)
except subprocess.TimeoutExpired:
    timed_out = True
    process.send_signal(signal.SIGTERM)
    try:
        stdout, stderr = process.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
with open(f"{output}/churn-container.stdout", "wb") as file:
    file.write(stdout)
with open(f"{output}/churn-container.stderr", "wb") as file:
    file.write(stderr)
with open(f"{output}/churn-container.status", "w", encoding="utf-8") as file:
    file.write(f"{process.returncode}\n")
with open(f"{output}/churn-container.timed-out", "w", encoding="utf-8") as file:
    file.write(f"{str(timed_out).lower()}\n")
sys.exit(0)
