#!/usr/bin/env -S xsh --
# Wait For
# Poll an HTTP endpoint until it returns an acceptable status or times out.
# Usage: xsh showcase/wait-for.xsh -- URL [--timeout N] [--interval N] [--status CODE]
# Example: xsh showcase/wait-for.xsh -- http://localhost:8080/health --status 200
type Opts = {url: Str, timeout: Int, interval: Int, status: Int}

proc main(...argv: List[Str]) [net, time, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      url: {form: "URL"},
      timeout: {form: "--timeout N", kind: "UInt", default: 30, min: 1},
      interval: {form: "--interval N", kind: "UInt", default: 2, min: 1},
      status: {form: "--status CODE", kind: "UInt", default: 0, max: 599},
    },
  )?

  let interval = if opts.interval < 1 { 1 } else { opts.interval }
  var elapsed = 0
  print f"waiting for ${opts.url}  timeout=${opts.timeout}s  interval=${interval}s"

  while elapsed <= opts.timeout {
    match net.request({
      method: "GET",
      url: opts.url,
      timeout: 2s,
      redirects: 3,
      fail_status: false,
      max_body_bytes: 256,
    }) {
      Ok(response) => {
        let ok = if opts.status > 0 {
          response.status == opts.status
        } else {
          response.status >= 200 and response.status < 500
        }

        if ok {
          print f"ready  status=${response.status}  elapsed=${elapsed}s"
          return
        }

        print f"  ${elapsed}s  status=${response.status}  retrying"
      }
      Err(_) => print f"  ${elapsed}s  no response  retrying"
    }

    break when elapsed == opts.timeout

    for _ in range(interval) {
      time.sleep(1s)?
    }

    elapsed += interval
  }

  print f"timed out after ${opts.timeout}s"
}
