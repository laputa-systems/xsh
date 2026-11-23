#!/usr/bin/env -S xsh --
cli main(dir: Path, keep: Int = 3, dry_run = false) {
  guard keep >= 1 else {
    eprint f"--keep must be positive, got {keep}"
    exit 2
  }

  print f"would keep {keep} in {dir}; dry run: {dry_run}"
}
