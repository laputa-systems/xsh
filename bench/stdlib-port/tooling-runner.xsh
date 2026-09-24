# Times selected xsht commands inside Dockerfile.test, excluding Docker startup.
proc measure(binary: Str, name: Str, phase: Str, index: Int, side: Str) [process, time, io, error] {
  var command = [binary, "api", "summary"]
  if name == "xsht_check_core" {
    command = [binary, "check", "core/ls.xsh"]
  } else if name == "xsht_lint_core" {
    command = [binary, "lint", "core/ls.xsh"]
  } else if name != "xsht_api" {
    abort(2)
  }
  let measured = time.measure(process.command_argv(binary, command), quiet: true)?
  if ! measured.status.exited_with(0) { abort(1) }
  print f"${name}\t${phase}\t${index}\t${side}\t${measured.wall_ns}"
}

proc main(...argv: List[Str]) [process, time, io, error] {
  let reference = argv[0]
  let candidate = argv[1]
  let round = argv[2].parse_int()?
  let samples = argv[3].parse_int()?
  let names = argv[4].split(",")
  for name in names {
    if round % 2 == 0 {
      measure(reference, name, "warmup", 0, "reference")?
      measure(candidate, name, "warmup", 0, "candidate")?
    } else {
      measure(candidate, name, "warmup", 0, "candidate")?
      measure(reference, name, "warmup", 0, "reference")?
    }
    var index = 0
    while index < samples {
      if (round + index) % 2 == 0 {
        measure(reference, name, "sample", index, "reference")?
        measure(candidate, name, "sample", index, "candidate")?
      } else {
        measure(candidate, name, "sample", index, "candidate")?
        measure(reference, name, "sample", index, "reference")?
      }
      index = index + 1
    }
  }
}
