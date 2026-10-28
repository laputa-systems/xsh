##! Bounded execution through the existing owned process lifecycle.
use stage_contract as contract

## A child status or a structured process failure, after owned cleanup finishes.
export enum RunOutcome { Finished(Status), TimedOut(ProcessError), CouldNotExecute(ProcessError) }

## An execution limit that cannot admit a process.
export error LimitError = Expired(remaining_ns: Int) | InvalidCaseTimeout(seconds: Int)

## Caps a declared case timeout by the remaining automatic runtime budget.
export pure limit_ms(case_timeout_seconds: Int, remaining_ns: Int) -> Result[Int, Error] {
  return Err(LimitError.InvalidCaseTimeout(seconds: case_timeout_seconds)) when case_timeout_seconds <= 0
  return Err(LimitError.Expired(remaining_ns: remaining_ns)) when remaining_ns < 1000000
  let remaining_ms = remaining_ns / 1000000
  let case_ms = case_timeout_seconds * 1000
  if remaining_ms < case_ms { remaining_ms } else { case_ms }
}

## Runs one explicit command and consumes timeout and cancellation cleanup.
export proc execute(spec: contract.CommandSpec, timeout_ms: Int, output: Path? = null) [process, error, io] -> Result[RunOutcome, Error] {
  return Err(LimitError.Expired(remaining_ns: timeout_ms * 1000000)) when timeout_ms <= 0
  print f"[{spec.stage} target={spec.target} timeout_ms={timeout_ms}] {spec.argv.join(" ")}"
  let command = if let destination = output {
    process.command_argv(spec.executable, spec.argv, cwd: spec.cwd,
      env: spec.environment, stdout: destination, timeout: time.millis(timeout_ms))
  } else {
    process.command_argv(spec.executable, spec.argv, cwd: spec.cwd,
      env: spec.environment, timeout: time.millis(timeout_ms))
  }
  match process.run(command) {
    Ok(status) => Finished(status)
    Err(cause) => match cause {
      ProcessError.Timeout {..} => TimedOut(cause)
      else => CouldNotExecute(cause)
    }
  }
}
