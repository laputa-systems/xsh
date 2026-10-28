import json
import sys


def fold(pairs):
    folded = {}
    for key, value in pairs:
        if key in folded and folded[key] != value:
            raise ValueError(f"repeated key {key!r} carries different values")
        folded[key] = value
    return folded


with open(sys.argv[1], "rb") as source:
    document = json.loads(source.read().decode("utf-8"), object_pairs_hook=fold)
json.dump(document, sys.stdout, separators=(",", ":"))
