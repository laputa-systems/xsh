#!/usr/bin/env -S xsh --
proc main(target: Str, ...bins: List[Str]) [process, error, io] {
  let bin_dir = fp"target/${target}/dist"
  let expected_machine = if "aarch64" in target {
    "AArch64"
  } else {
    "Advanced Micro Devices X86-64"
  }

  for bin in bins {
    let candidate = fp"${bin_dir}/${bin}"
    let binary_data = candidate.read_bytes()?
    if binary_data.len() < 4 {
      eprint f"${bin} is not an ELF executable"
      abort(1)
    }

    if binary_data.slice(0, length: 4) != b"\x7fELF" {
      eprint f"${bin} is not an ELF executable"
      abort(1)
    }

    if ! candidate.executable()? {
      eprint f"${bin} is not executable"
      abort(1)
    }

    let header = run.capture --text "readelf" -h $candidate ?
    if expected_machine not in header.stdout {
      eprint f"${bin} has the wrong target architecture"
      abort(1)
    }

    let result = run.capture --text "readelf" -d $candidate ?

    if "NEEDED" in result.stdout {
      eprint f"${bin} is not static"
      abort(1)
    }
  }
}
