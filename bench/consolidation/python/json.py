import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
with (root / "index.json").open() as source:
    rows = json.load(source)
with (root / "replacement.json").open() as source:
    replacement = json.load(source)
with (root / "addition.json").open() as source:
    addition = json.load(source)
rows = [replacement if row["name"] == replacement["name"] else row for row in rows]
rows.append(addition)
rows.sort(key=lambda row: row["name"])
print(json.dumps(rows, separators=(",", ":")))
