#!/usr/bin/env -S xsh --
# Bench
# Measure command runtime over repeated runs and report simple latency percentiles.
# Usage: xsh showcase/bench.xsh -- [--runs N] [--warmup N] COMMAND [ARGS...]
# Example: xsh showcase/bench.xsh -- --runs 5 grep needle file.txt
type Opts = {runs: Int, warmup: Int, argv: List[Str]}

proc main(...cmd: List[Str]) [time, error] {
  let opts: Opts = cli.parse(
    cmd,
    {
      runs: {
        form: "--runs N",
        kind: "UInt",
        default: 10,
        min: 1,
      },
      warmup: {
        form: "--warmup N",
        kind: "UInt",
        default: 0,
      },
      argv: {
        form: "...COMMAND",
        repeated: true,
        required: true,
      },
    },
  )?

  let command = process.command_argv(opts.argv.get(0)?, opts.argv)
  var times: List[Int] = []

  for _ in range(opts.warmup) {
    let result = time.measure(command)?

    if ! result.status.exited_with(0) {
      print f"command failed during warmup with exit code ${result.status.exit_code()?}"
      return
    }
  }

  for _ in range(opts.runs) {
    let result = time.measure(command)?

    if ! result.status.exited_with(0) {
      print f"command failed with exit code ${result.status.exit_code()?}"
      return
    }

    times = times.push(result.duration_ms)
  }

  let n = times.len()
  let sorted = times |> sort
  let total = times |> sum
  let mean = total / n
  let min_ms = (times |> min)?
  let max_ms = (times |> max)?
  let p50 = sorted.get(n / 2, 0)
  let p75 = sorted.get(n * 3 / 4, 0)
  let p90 = sorted.get(n * 9 / 10, 0)
  let p99 = sorted.get(n * 99 / 100, 0)
  let now = time.format(time.now(), "%Y-%m-%d %H:%M:%S", utc: true)?
  print f"bench ${now} n=${n} warmup=${opts.warmup}"
  print f"  mean=${mean}ms min=${min_ms}ms max=${max_ms}ms p50=${p50}ms p75=${p75}ms p90=${p90}ms p99=${p99}ms"
}
