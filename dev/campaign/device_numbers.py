import os
import sys


device = os.stat(sys.argv[1]).st_rdev
print(os.major(device), os.minor(device))
