import subprocess

for index in range(128):
    subprocess.run(["/usr/bin/printf", "probe %s\n", str(index)], check=True)
