#!/usr/bin/env -S xsh --
# Env Diff
# Compare two env-style files and report added, removed, and changed variables.
# Usage: xsh showcase/env-diff.xsh -- --a FILE --b FILE
# Example: xsh showcase/env-diff.xsh -- --a dev.env --b prod.env
type Opts = {a: Path, b: Path}

# Parse key=value pairs from a .env file into a Map, ignoring comments and blanks.
pure parse_env(content: Str) -> Map[Str] {
  var values: Map[Str] = {}

  for line in content.lines() {
    let t = line.trim()
    continue when t == "" or t.starts_with("#")
    let parts = t.fields("=")
    continue when parts.len() < 2
    let key = parts[0].trim()

    if key != "" {
      values[key] = parts[1].trim()
    }
  }

  return values
}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      a: {form: "--a FILE", kind: "Path", file: true, required: true},
      b: {form: "--b FILE", kind: "Path", file: true, required: true},
    },
  )?

  let env_a = parse_env(opts.a.read_text()?)
  let env_b = parse_env(opts.b.read_text()?)
  let keys_a = env_a.keys() |> sort
  let keys_b = env_b.keys() |> sort
  var only_a = 0
  var only_b = 0
  var changed = 0

  for k in keys_a {
    if ! env_b.has(k) {
      print f"- ${k}=${env_a.get(k, "")}"
      only_a += 1
    }
  }

  for k in keys_b {
    if ! env_a.has(k) {
      print f"+ ${k}=${env_b.get(k, "")}"
      only_b += 1
    }
  }

  for k in keys_a {
    if env_b.has(k) and env_a.get(k, "") != env_b.get(k, "") {
      print f"~ ${k}"
      print f"  - ${env_a.get(k, "")}"
      print f"  + ${env_b.get(k, "")}"
      changed += 1
    }
  }

  let unchanged = keys_a
    |> where env_b.has(.) and env_a.get(., "") == env_b.get(., "")
    |> count()

  print ""
  print f"${only_a} removed  ${only_b} added  ${changed} changed  ${unchanged} unchanged"
}
