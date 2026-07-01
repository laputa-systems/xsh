#!/usr/bin/env -S xsh --
# Parse Log
# Parse structured log lines into typed records, count levels, and redact IP addresses.
# Usage: xsh showcase/parse-log.xsh -- [LOGFILE]
# Example: xsh showcase/parse-log.xsh -- app.log
type LogEntry = {timestamp: Str, level: Str, module: Str, message: Str}

type Match = {start: Int, end: Int, text: Str}

proc main(input: Str = "") [fs, error] {
  let sample = """2026-01-15T10:00:01Z INFO [auth] user login from 192.168.1.10
2026-01-15T10:00:02Z WARN [db] slow query from 10.0.0.5 took 2s
2026-01-15T10:00:03Z ERROR [auth] failed login attempt from 203.0.113.42
2026-01-15T10:00:04Z INFO [api] request 172.16.0.1 -> /health ok
2026-01-15T10:00:05Z INFO [db] connection pool ready
2026-01-15T10:00:06Z ERROR [api] upstream 198.51.100.7 unreachable
"""

  let source = if input == "" { sample } else { fp"${input}".read_text()? }

  # compile patterns once
  let log_re = regex.compile("^(\\S+)\\s+(INFO|WARN|ERROR|DEBUG)\\s+\\[(\\w+)\\]\\s+(.+)$")?
  let ip_re = regex.compile("\\b(\\d{1,3}\\.){3}\\d{1,3}\\b")?

  # parse each line into a typed record, dropping blanks and unparseable lines
  let entries: List[LogEntry] = source.lines()
    |> where .trim() != ""
    |> where log_re.captures(.).len() >= 5
    |> map { |line|
      let caps = log_re.captures(line)
      {timestamp: caps[1], level: caps[2], module: caps[3], message: caps[4]}
    }

  # count occurrences of each level using a map
  var counts: Map[Int] = {}

  for entry in entries {
    counts[entry.level] = counts.get(entry.level, 0) + 1
  }

  print f"parsed ${entries.len()} entries"

  for level in counts.keys() {
    let n = counts.get(level)?
    print f"  ${level}: ${n}"
  }

  # fold message char lengths into a total
  let total_chars = entries
    |> map .message.count_chars()
    |> fold(0) { |acc|
      acc + .
    }

  print f"total message chars: ${total_chars}"

  # check properties with any / all
  let has_errors = entries |> any .level == "ERROR"
  let all_timestamped = entries |> all .timestamp != ""
  print f"has errors: ${has_errors}  all timestamped: ${all_timestamped}"

  # find and redact IPs in each error message
  let errors: List[LogEntry] = entries |> where .level == "ERROR"

  for entry in errors {
    let hits: List[Match] = ip_re.find(entry.message)
    let redacted = ip_re.replace(entry.message, "<IP>")
    print f"error [${entry.module}] IPs found: ${hits.len()}  redacted: ${redacted}"
  }
}
