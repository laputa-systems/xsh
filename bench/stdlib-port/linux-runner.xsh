# Runs in Dockerfile.test so only child process time enters each sample.
# Output is tab-separated: workload, phase, sample index, side, wall nanoseconds.
proc measure(binary: Str, script: Str, name: Str, phase: Str, index: Int, side: Str, expected: Int) [process, time, io, error] {
  let measured = time.measure(process.command_argv(binary, [binary, script]), quiet: true)?
  if ! measured.status.exited_with(expected) { abort(1) }
  print f"${name}\t${phase}\t${index}\t${side}\t${measured.wall_ns}"
}

proc main(...argv: List[Str]) [process, time, io, error] {
  let reference = argv[0]
  let candidate = argv[1]
  let round = argv[2].parse_int()?
  var offset = 3
  while offset + 2 < argv.len() {
    let name = argv[offset]
    let samples = argv[offset + 1].parse_int()?
    let expected = argv[offset + 2].parse_int()?
    let script = f"/work/bench/stdlib-port/${name}.xsh"
    if round % 2 == 0 {
      measure(reference, script, name, "warmup", 0, "reference", expected)?
      measure(candidate, script, name, "warmup", 0, "candidate", expected)?
    } else {
      measure(candidate, script, name, "warmup", 0, "candidate", expected)?
      measure(reference, script, name, "warmup", 0, "reference", expected)?
    }

    var index = 0
    while index < samples {
      if (round + index) % 2 == 0 {
        measure(reference, script, name, "sample", index, "reference", expected)?
        measure(candidate, script, name, "sample", index, "candidate", expected)?
      } else {
        measure(candidate, script, name, "sample", index, "candidate", expected)?
        measure(reference, script, name, "sample", index, "reference", expected)?
      }
      index = index + 1
    }
    offset = offset + 3
  }
}
