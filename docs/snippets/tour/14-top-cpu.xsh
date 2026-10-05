# platform: linux
# Top CPU consumers over one second, from /proc/PID/stat (USER_HZ = 100).
const stat_line = rx"^(\d+) \((.*)\) \S+ (.*)$"

type Sample = {pid: Int, comm: Str, ticks: Int}

pure parse_stat(line: Str) -> Sample? {
  if let [_, pid, comm, rest] = stat_line.captures(line) {
    let fields = rest.fields()
    let ticks = (fields[10].parse_int() ?? 0) + (fields[11].parse_int() ?? 0)
    Sample(pid: pid.parse_int() ?? 0, comm:, ticks:)
  } else {
    null
  }
}

proc snapshot() -> Result[Map[Int, Sample]] {
  var samples: Map[Int, Sample] = {}

  for entry in process.list()? {
    # A process can exit between listing and reading; skip it.
    guard let text = fp"/proc/{entry.pid}/stat".read_text() else {
      continue
    }

    let sample = parse_stat(text.trim())
    if sample != null {
      samples[sample.pid] = sample
    }
  }

  samples
}

let before = snapshot()?
time.sleep(1s)
let after = snapshot()?

let busiest = after.values()
  |> map { |now|
    let prev = before.get(now.pid) ?? now
    {now, delta: now.ticks - prev.ticks}
  }
  |> sort-by(desc: true) .delta
  |> take(5)

for {now, delta} in busiest {
  print f"{now.pid:>7} {delta:>4}% {now.comm}"
}
