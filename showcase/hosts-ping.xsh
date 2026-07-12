#!/usr/bin/env -S xsh --
# Hosts Ping
# Probe hosts with ping and summarize latency and packet-loss results.
# Usage: xsh showcase/hosts-ping.xsh -- [--count N] HOST [HOST...]
# Example: xsh showcase/hosts-ping.xsh -- --count 2 1.1.1.1 example.com
type PingResult = {host: Str, avg: Str, ok: Bool}

type Opts = {count: Int, hosts: List[Str]}

proc main(...argv: List[Str]) [process, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      count: {
        form: "--count N",
        kind: "UInt",
        default: 3,
        min: 1,
      },
      hosts: {
        form: "...HOST",
        repeated: true,
        required: true,
      },
    },
  )?

  # Matches the avg field from: min/avg/max/stddev = 0.5/1.2/2.0/0.1 ms
  let ms_re = regex.compile("min/avg/max/[a-z]+ = [0-9.]+/([0-9.]+)/")?

  let results: List[PingResult] = opts.hosts
    |> par-map { |host|
      let ping_out = run.text "ping" "-c" $opts.count "-q" $host
      var result: PingResult = {host: host, avg: "--", ok: false}

      match ping_out {
        Ok(output) => {
          let caps = ms_re.captures(output)

          if caps.len() >= 2 {
            result = {host: host, avg: f"${caps[1]}ms", ok: true}
          }
        }
        Err(_) => {}
      }

      result
    }

  print f"${"host":<40} ${"avg rtt":>10}  status"
  print f"${"----":<40} ${"-------":>10}  ------"

  for r in results {
    let status = if r.ok { "ok" } else { "unreachable" }
    print f"${r.host:<40} ${r.avg:>10}  ${status}"
  }

  let ok_n = results
    |> where .ok
    |> count()

  print f"""
${ok_n}/${results.len()} hosts reachable"""
}
