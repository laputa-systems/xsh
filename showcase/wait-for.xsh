#!/usr/bin/env -S xsh --
# Wait For
# Poll an HTTP endpoint until it returns an acceptable status or times out.
# --timeout bounds requests and sleeps together.
# A timeout exits unsuccessfully so callers can stop dependent work.
# Usage: xsh showcase/wait-for.xsh -- URL [--timeout N] [--interval N] [--status CODE]
# Example: xsh showcase/wait-for.xsh -- http://localhost:8080/health --status 200
type Opts = {url: Str, timeout: Int, interval: Int, status: Int}

proc main(...argv: List[Str]) [net, time, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      url: {
        form: "URL",
      },
      timeout: {
        form: "--timeout N",
        kind: "UInt",
        default: 30,
        min: 1,
      },
      interval: {
        form: "--interval N",
        kind: "UInt",
        default: 2,
        min: 1,
      },
      status: {
        form: "--status CODE",
        kind: "UInt",
        default: 0,
        max: 599,
      },
    },
  )?

  let started = time.now()
  let deadline = started + opts.timeout * 1000
  print f"waiting for ${opts.url}  timeout=${opts.timeout}s  interval=${opts.interval}s"

  while true {
    let remaining = deadline - time.now()
    if remaining <= 0 {
      break
    }

    let request_timeout = if remaining < 2000 { time.millis(remaining) } else { 2s }
    match net.request({
      method: "GET",
      url: opts.url,
      timeout: request_timeout,
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

        let elapsed = (time.now() - started) / 1000
        if ok and time.now() <= deadline {
          print f"ready  status=${response.status}  elapsed=${elapsed}s"
          return
        }

        print f"  ${elapsed}s  status=${response.status}  retrying"
      }
      Err(_) => print f"  ${(time.now() - started) / 1000}s  no response  retrying"
    }

    let after_request = deadline - time.now()
    if after_request <= 0 {
      break
    }

    let sleep_ms = if opts.interval > after_request / 1000 {
      after_request
    } else {
      opts.interval * 1000
    }
    time.sleep(time.millis(sleep_ms))?
  }

  print f"timed out after ${opts.timeout}s"
  abort(1)
}
