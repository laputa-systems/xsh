from pathlib import Path
import re
import sys

pattern = re.compile(r"^#define[ \t]+(CAP_[A-Z0-9_]+)[ \t]+([0-9]+)[ \t]*$")
with (Path(sys.argv[1]) / "capability.h").open(encoding="ascii", newline="") as source:
    for line in source:
        match = pattern.fullmatch(line.rstrip("\n"))
        if match:
            print('{"' + match[1].lower() + '",' + match[2] + '},')
