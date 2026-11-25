import pathlib
import sys


target = pathlib.Path(sys.argv[1])
target.write_text(target.read_text().replace(sys.argv[2], sys.argv[3]))
