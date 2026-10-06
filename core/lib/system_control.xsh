##! Shared control policy over typed kernel operations.
use gnu

error ControlError = Invalid | Unsupported

## A validated sysctl write, with configuration-specific error suppression.
export type Assignment = {key: Str, value: Str, ignore_failure: Bool}

## Parse a sysctl assignment without performing any writes.
export pure assignment(text: Str, config: Bool = false) -> Result[Assignment, Error] {
  let fields = text.split("=", maxsplit: 1)
  if fields.len() != 2 { return Err(ControlError.Invalid(f"expected key=value: {text}")) }
  var key = fields[0].trim()
  let ignore_failure = config and key.starts_with("-")
  if ignore_failure { key = key.byte_slice(1) }
  key = key.replace("/", with: ".")
  if key == "" or key.starts_with(".") or key.ends_with(".") or ".." in key {
    return Err(ControlError.Invalid(f"invalid sysctl key: {key}"))
  }
  if "*" in key or "?" in key or "[" in key {
    return Err(ControlError.Unsupported("sysctl glob assignments are not available"))
  }
  Ok({key: key, value: fields[1].trim(), ignore_failure: ignore_failure})
}

## Parse all named configuration files before applying their assignments.
export proc load_assignments(files: List[Path]) -> Result[List[Assignment], Error] {
  var result: List[Assignment] = []
  for file in files {
    for line in file.read_text()?.lines() {
      let text = line.trim()
      continue when text == "" or text.starts_with("#") or text.starts_with(";")
      result += [assignment(text, true)?]
    }
  }
  Ok(result)
}

## Preserve child exit codes and map signal deaths to shell status codes.
export proc status_code(status: Status) -> Result[Int, Error] {
  if status.exited() { return status.exit_code() }
  Ok(128 + status.signal_number()?)
}

## Resolve an executable before any locking or session transition.
export proc command(argv: List[Str], new_session: Bool = false, detach: Bool = false) -> Result[Command, Error] {
  if argv.is_empty() { return Err(ControlError.Invalid("missing command")) }
  let executable = process.which(argv[0])?
  Ok(process.command_argv(executable, argv, new_session: new_session, detach: detach))
}

## Require explicit direct-kernel shutdown and sync unless disabled.
export proc power(action: Str, no_sync: Bool, force: Bool) {
  if ! force { return Err(ControlError.Unsupported("service manager shutdown is not available; use --force for the direct kernel operation")) }
  if ! no_sync { fs.sync() }
  match action {
    "reboot" => linux.reboot()
    "poweroff" => linux.poweroff()
    "halt" => linux.halt()
    else => return Err(ControlError.Invalid(f"unknown power action {action}"))
  }
}

## Convert a kernel millisecond timestamp without wrapping the formatter's nanoseconds.
export pure epoch_nanoseconds(epoch_ms: Int) -> Result[Int, Error] {
  if epoch_ms > 9223372036854 or epoch_ms < -9223372036854 {
    return Err(ControlError.Invalid("clock value exceeds the formatter timestamp range"))
  }
  Ok(epoch_ms * 1000000)
}

## Remove only the kernel's leading monotonic timestamp when requested.
export pure kernel_message(text: Str, no_time: Bool) -> Str {
  if no_time { rx"^\[\s*[0-9]+(?:\.[0-9]+)?\]\s*".replace(text, with: "") } else { text }
}
