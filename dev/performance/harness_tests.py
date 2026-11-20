import subprocess
import sys

commands = [
    [sys.executable, '-B', '-m', 'unittest', 'discover', '-s', 'bench/consolidation', '-p', 'test_harness.py'],
    [sys.executable, '-B', '-m', 'unittest', 'dev.performance.test_bench', 'dev.performance.test_qualified_bench'],
]
for command in commands:
    status = subprocess.call(command)
    if status:
        sys.exit(status)
