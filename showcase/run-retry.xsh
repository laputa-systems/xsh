#!/usr/bin/env -S xsh --
# Run Retry
# Run a command with retries and short backoff for transient failures.
# Usage: xsh showcase/run-retry.xsh -- COMMAND [ARGS...]
# Example: xsh showcase/run-retry.xsh -- curl -fsS https://example.com
error RetryRunError = CommandFailed(message: Str)

proc run_attempt(argv: List[Str], try_num: Int, max_tries: Int) [process, error] {
  let command = process.command_argv(argv[0], argv)
  let status = process.run(command).context("run-failed", "failed to exec command")?

  if status.exited() {
    let code = status.exit_code()?

    if code == 0 {
      print f"ok (try ${try_num})"
      return
    }

    print f"exit ${code} (try ${try_num}/${max_tries})"
  } else if status.signaled() {
    print f"signal ${status.signal_number()?} (try ${try_num}/${max_tries})"
  } else {
    print f"failed (try ${try_num}/${max_tries})"
  }

  Err(RetryRunError.CommandFailed(message: f"command failed on try ${try_num}"))
}

proc main(...cmd: List[Str]) [process, time, error] {
  if cmd.len() == 0 {
    print "usage: xsh showcase/run-retry.xsh -- COMMAND [ARGS...]"
    print "       xsh showcase/run-retry.xsh -- \"COMMAND STRING\"  (parsed as argv)"
    return
  }

  let max_tries = 3
  let delay = 200ms

  # A single space-containing argument is treated as a shell command string.
  let argv = if cmd.len() == 1 {
    process.argv_words(cmd[0]).context("argv-parse", "failed to parse command string")?
  } else {
    cmd
  }

  var try_num = 0

  match retry [delay, delay] {
    try_num += 1
    run_attempt(argv, try_num, max_tries)?
  } {
    Ok(_) => {}
    Err(_) => print f"command failed after ${max_tries} tries"
  }
}
