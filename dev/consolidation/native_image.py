#!/usr/bin/env python3

import json
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[2]
LOCK_PATH = ROOT / "bench/consolidation/versions.lock.json"
IMAGE_ID = json.loads(LOCK_PATH.read_text(encoding="utf-8"))["base_images"]["amd64"]
if re.fullmatch(r"sha256:[0-9a-f]{64}", IMAGE_ID) is None:
    raise ValueError("versions.lock.json must name a lowercase sha256 amd64 base image")


def runner_image_id(runner):
    output = subprocess.check_output([str(runner), "--image-id"], text=True, timeout=30)
    if output != IMAGE_ID + "\n":
        raise ValueError("native runner selected image differs from versions.lock.json")
    return IMAGE_ID


if __name__ == "__main__":
    print(IMAGE_ID)
