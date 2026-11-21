#!/usr/bin/env python3
import hashlib
import os
import pathlib
import stat
import sys

for argument in sys.argv[1:]:
    root = pathlib.Path(argument)
    digest = hashlib.sha256()
    count = 0
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in sorted(directories + files):
            path = pathlib.Path(directory) / name
            metadata = path.lstat()
            relative = path.relative_to(root).as_posix()
            if stat.S_ISREG(metadata.st_mode):
                content = hashlib.file_digest(path.open('rb'), 'sha256').digest()
            elif stat.S_ISLNK(metadata.st_mode):
                content = os.readlink(path).encode()
            elif stat.S_ISDIR(metadata.st_mode):
                content = b''
            else:
                raise ValueError(path)
            digest.update(relative.encode() + b'\0' + str(stat.S_IMODE(metadata.st_mode)).encode() + b'\0' + content)
            count += 1
        directories.sort()
    print(root.name, count, digest.hexdigest(), sep='\t')
