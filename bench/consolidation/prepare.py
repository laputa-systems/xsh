"""Prepare a checksummed comparator overlay without changing Dockerfile.test."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request

from fixtures import digest

HERE = Path(__file__).resolve().parent


def prepare(args):
    lock = json.loads((HERE / "versions.lock.json").read_text())
    base = args.base_image or lock["base_images"].get(args.arch)
    if not base or not base.startswith("sha256:"):
        raise ValueError("supply the immutable Dockerfile.test image ID for this architecture")
    if args.arch in lock["base_images"] and base != lock["base_images"][args.arch]:
        raise ValueError("base image differs from the committed architecture pin")
    info = json.loads(subprocess.check_output(["docker", "image", "inspect", base]))[0]
    if info["Architecture"] != args.arch or info["Id"] != base:
        raise ValueError("base image identity or architecture differs from the requested target")
    context = args.destination.resolve()
    if context == Path("/tmp") or Path("/tmp") in context.parents:
        raise ValueError("preparation artifacts must live on disk outside /tmp")
    context.mkdir(parents=True, exist_ok=True)
    checksums = []
    for name, source in lock["sources"].items():
        artifact = source if "url" in source else source[args.arch]
        filename = {"nushell": "nu"}.get(name, name) + ".tar.gz"
        path = context / filename
        if not path.exists():
            staging = path.with_suffix(".download")
            with urllib.request.urlopen(artifact["url"], timeout=120) as response, staging.open("wb") as destination:
                shutil.copyfileobj(response, destination)
            if digest(staging) != artifact["sha256"]:
                raise ValueError(f"download checksum mismatch: {artifact['url']}")
            staging.replace(path)
        if digest(path) != artifact["sha256"]:
            raise ValueError(f"cached artifact checksum mismatch: {path}")
        checksums.append(f"{artifact['sha256']}  {filename}\n")
    (context / "checksums").write_text("".join(checksums))
    for filename in ("Dockerfile", "versions.lock.json"):
        shutil.copyfile(HERE / filename, context / filename)
    request = {"base_image": base, "architecture": args.arch, "dockerfile_sha256": digest(HERE / "Dockerfile"), "lock_sha256": digest(HERE / "versions.lock.json")}
    (context / "preparation.json").write_text(json.dumps(request, indent=2) + "\n")
    if args.build:
        # BuildKit parses an image ID in FROM as a registry tag, so resolve a local alias first.
        reference = "xsh-comparative-base:" + args.arch + "-" + base.removeprefix("sha256:")
        subprocess.run(["docker", "image", "tag", base, reference], check=True)
        resolved = subprocess.check_output(["docker", "image", "inspect", "--format", "{{.Id}}", reference], text=True).strip()
        if resolved != base:
            raise ValueError("comparator base alias differs from its pinned image ID")
        subprocess.run(["docker", "build", "--platform", "linux/" + args.arch, "--build-arg", "BASE_REFERENCE=" + reference, "--build-arg", "BASE_IMAGE=" + base, "--build-arg", "TARGETARCH=" + args.arch, "--tag", args.tag, str(context)], check=True)
        request["image_id"] = subprocess.check_output(["docker", "image", "inspect", "--format", "{{.Id}}", args.tag], text=True).strip()
        (context / "preparation.json").write_text(json.dumps(request, indent=2) + "\n")
    print(json.dumps(request, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--arch", choices=("amd64", "arm64"), default="amd64")
    parser.add_argument("--base-image")
    parser.add_argument("--tag", default="xsh-comparative:consolidation")
    parser.add_argument("--build", action="store_true", help="compile the overlay; coordinate the machine build queue first")
    prepare(parser.parse_args())
