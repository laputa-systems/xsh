import os
import sys

parent, count, ready, done = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
left = os.path.join(parent, "left")
right = os.path.join(parent, "right")
parked = os.path.join(parent, "parked")
os.mkdir(left)
os.mkdir(right)
for index in range(count):
    os.rename(left, parked)
    os.rename(right, left)
    os.rename(parked, right)
    if index == 999:
        with open(ready, "w"):
            pass
with open(done, "w"):
    pass
