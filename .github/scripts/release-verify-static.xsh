#!/usr/bin/env -S xsh --
proc main(target: Str, ...bins: List[Str]) [process, error, io] {
  let bin_dir = fp"target/${target}/dist"

  for bin in bins {
    let candidate = fp"${bin_dir}/${bin}"
    let result = run.capture --text "readelf" -d $candidate ?

    if result.stdout.contains("NEEDED") {
      eprint f"${bin} is not static"
      abort(1)
    }
  }
}

main(@args)?
