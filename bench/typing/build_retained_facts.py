"""Link the profiling helper against an existing release libxsh, without Cargo edits."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(library_root, output, library=None, product_probe_output=None):
    library_root = Path(library_root).resolve()
    release = library_root / "target" / "release"
    libraries = sorted(release.rglob("libxsh-*.rlib"))
    if library:
        library = Path(library).resolve()
        if not library.is_file():
            raise ValueError(f"missing selected libxsh: {library}")
    elif len(libraries) == 1:
        library = libraries[0]
    else:
        raise ValueError(f"expected exactly one libxsh release artifact, found {len(libraries)}; pass --library")
    source = Path(__file__).with_name("retained_facts.rs").resolve()
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    # Recent Cargo artifacts keep full crate metadata beside an rlib stub.
    metadata = library.with_suffix(".rmeta")
    extern = metadata if metadata.is_file() else library
    dependencies = sorted({path.parent for suffix in ("*.rmeta", "*.rlib", "*.dylib") for path in release.rglob(suffix)})
    native = sorted(path for path in (release / "build").glob("**/out") if path.is_dir())
    command = ["rustc", "--edition=2024", "-C", "opt-level=3", "-C", "debuginfo=0", "-C", "lto=thin", "--crate-name", "typing_retained_facts", str(source), "--extern", f"xsh={extern}", "-o", str(output)]
    if metadata.is_file():
        command.extend(["--extern", f"xsh={library}"])
    for directory in dependencies:
        command.extend(["-L", f"dependency={directory}"])
    for directory in native:
        command.extend(["-L", f"native={directory}"])
    compiler = subprocess.check_output(["rustc", "--version", "--verbose"], cwd=library_root, text=True).strip()
    completed = subprocess.run(command, cwd=library_root, text=True, capture_output=True)
    if completed.returncode:
        raise RuntimeError(f"retained-facts rustc failed ({completed.returncode}):\n{completed.stderr}")
    self_test = json.loads(subprocess.check_output([str(output), "--self-test"], cwd=library_root, text=True))
    if self_test.get("status") != "passed":
        raise ValueError("retention collector self-test did not pass")
    provenance = {
        "schema_version": 1,
        "library_root": str(library_root),
        "library_path": str(library),
        "library_sha256": sha256(library),
        "metadata_path": str(extern),
        "metadata_sha256": sha256(extern),
        "helper_source_sha256": sha256(source),
        "builder_source_sha256": sha256(Path(__file__)),
        "helper_binary": str(output),
        "helper_binary_sha256": sha256(output),
        "rustc": compiler,
        "build_command": command,
        "self_test": self_test,
        "profiling_only": True,
        "product_binaries_modified": False,
        "features": "inherited from existing libxsh; native-tests preparation facade required",
    }
    if product_probe_output:
        product_source = source.with_name("product_probe.rs")
        product_output = Path(product_probe_output).resolve()
        product_output.parent.mkdir(parents=True, exist_ok=True)
        product_command = list(command)
        product_command[product_command.index(str(source))] = str(product_source)
        product_command[product_command.index("typing_retained_facts")] = "typing_product_probe"
        product_command[product_command.index(str(output))] = str(product_output)
        completed = subprocess.run(product_command, cwd=library_root, text=True, capture_output=True)
        if completed.returncode:
            raise RuntimeError(f"product-probe rustc failed ({completed.returncode}):\n{completed.stderr}")
        provenance["product_probe"] = {
            "source_sha256": sha256(product_source),
            "binary": str(product_output),
            "binary_sha256": sha256(product_output),
            "build_command": product_command,
            "allocator": "System, matching production macOS entrypoints",
            "instrumentation": "none; no custom allocator, clock reads, or work counters",
            "scope": "whole process through verified preparation and teardown; user instructions never executed",
        }
    return provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library-root", required=True, type=Path)
    parser.add_argument("--library", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--product-probe-output", type=Path)
    args = parser.parse_args()
    print(json.dumps(build(args.library_root, args.output, args.library, args.product_probe_output), indent=2))


if __name__ == "__main__":
    main()
