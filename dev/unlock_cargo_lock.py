import fcntl
import sys


fcntl.flock(int(sys.argv[1]), fcntl.LOCK_UN)
