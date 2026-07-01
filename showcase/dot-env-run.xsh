#!/usr/bin/env -S xsh --
# Dot Env Run
# Load key/value pairs from a .env file and run a command with that environment.
# Usage: xsh showcase/dot-env-run.xsh -- ENVFILE COMMAND [ARGS...]
# Example: xsh showcase/dot-env-run.xsh -- .env printenv DATABASE_URL
type KV = {key: Str, val: Str}

proc main(...argv: List[Str]) [fs, process, error] {
  if argv.len() == 0 {
    print "usage: xsh showcase/dot-env-run.xsh -- ENVFILE COMMAND [ARGS...]"
    return
  }

  let file = fp"${argv[0]}"
  let cmd_args = argv |> drop(1)

  if cmd_args.len() == 0 {
    print "error: no command specified"
    return
  }

  let content = file.read_text()?

  # Captures key and raw value; value gets everything after the first =
  let kv_re = regex.compile("^([A-Za-z_][A-Za-z0-9_]*)=(.*)")?
  let comment_re = regex.compile("^\\s*#")?
  let dquote_re = regex.compile("^\"(.*)\"$")?
  let squote_re = regex.compile("^'(.*)'$")?
  var pairs: List[KV] = []

  for line in content.lines() {
    continue when line.trim() == ""
    continue when comment_re.matches(line)
    let caps = kv_re.captures(line)
    continue when caps.len() < 3
    let key = caps[1]
    let raw = caps[2].trim()
    let dquote = dquote_re.captures(raw)
    let squote = squote_re.captures(raw)
    let val = if dquote.len() >= 2 { dquote[1] } else { if squote.len() >= 2 { squote[1] } else { raw } }
    pairs = pairs.push({key: key, val: val})
  }

  print f"loaded ${pairs.len()} var(s) from ${file.display()}"
  let env_args = ["env"].extend([f"${kv.key}=${kv.val}" for kv in pairs]).extend(cmd_args)
  let command = process.command_argv("env", env_args)
  let status = process.run(command)?

  if ! status.exited_with(0) {
    print f"exit ${status.exit_code()?}"
  }
}
