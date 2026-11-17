import errno
import fcntl
import os
import struct
import subprocess
import sys
import termios

xsh, script, root = sys.argv[1:]
args = ['--sysroot', root, '-o', 'RANGE,SIZE', '--summary=never']

def run(command):
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 0, 0, 0))
    child = subprocess.Popen(command, stdout=slave, stderr=subprocess.PIPE, env={**os.environ, 'LC_ALL': 'C'})
    os.close(slave)
    output = b''
    while True:
        try:
            chunk = os.read(master, 8192)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        output += chunk
    os.close(master)
    stderr = child.stderr.read()
    return child.wait(timeout=10), output, stderr

reference = run(['/usr/bin/lsmem', *args])
candidate = run([xsh, script, '--', *args])
if reference != candidate:
    print(f'reference={reference!r} candidate={candidate!r}', file=sys.stderr)
    sys.exit(1)
