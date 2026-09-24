# Diagnostic for B07, outside the fixed acceptance workload set. Each phase
# repeats the operation 200 times against the same pinned /proc/modules text.
proc main() [fs, io, env, error, time] {
  var sink = 0
  var round = 0
  let read_start = time.now()
  while round < 200 {
    sink = sink + p"/proc/modules".read_text()?.byte_len()
    round = round + 1
  }
  print f"read_text ${time.now() - read_start} ms sink=${sink}"

  round = 0
  let call_start = time.now()
  while round < 200 {
    let modules = linux.modules()?
    sink = sink + 1
    round = round + 1
  }
  print f"modules_call ${time.now() - call_start} ms sink=${sink}"

  round = 0
  let first_start = time.now()
  while round < 200 {
    let first = linux.modules()? |> first()?
    sink = sink + first.size
    round = round + 1
  }
  print f"first_record ${time.now() - first_start} ms sink=${sink}"

  round = 0
  let full_start = time.now()
  while round < 200 {
    let modules = linux.modules()?.collect()
    sink = sink + modules.len() + modules[0].size
    round = round + 1
  }
  print f"full_records ${time.now() - full_start} ms sink=${sink}"
}
