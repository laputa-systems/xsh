##! Output and write-failure behavior for the util-linux command cohort.

use lib.gnu

## Whether musl's character-type locale is byte-oriented: the first non-empty of
## LC_ALL, LC_CTYPE and LANG decides, only C and POSIX count bytes, and an unset
## locale is C.UTF-8.
export proc byte_locale() [env] -> Bool {
  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let value = env.get(name) ?? ""
    return value in ["C", "POSIX"] when value != ""
  }
  false
}

proc write_error() [process] -> Unit {
  gnu.error("write error")
  exit 1
}

proc flush_output() [process, io] -> Unit {
  if let Err(_) = io.flush_stdout() { write_error() }
}

## Names a relative path through the kernel's working directory, which still
## reaches a directory whose mount was later covered; the evaluator's own
## working directory is resolved by name.
export pure process_cwd_path(relative: Bytes) -> Result[Path, Error] {
  Path.parse_bytes(bytes.concat([b"/proc/self/cwd/", relative]))
}

## Writes bytes and flushes stdout; a failed write reports NAME: write error and exits 1.
export proc write_bytes(data: Bytes) [process, io] -> Unit {
  if let Err(_) = io.write_stdout_bytes(data) { write_error() }
  flush_output()
}

## Writes text and flushes stdout with the util-linux write-failure diagnostic.
export proc write_text(text: Str) [process, io] -> Unit {
  if let Err(_) = io.write_stdout(text) { write_error() }
  flush_output()
}

proc diagnostic_write_error(failure: Error) [process, env] -> Unit {
  gnu.error(f"write error: {gnu.strerror(failure)}")
  exit 1
}

## Writes a diagnostic to stderr; a failed write reports NAME: write error: REASON and exits 1.
export proc write_diagnostic_bytes(data: Bytes) [process, env, io] -> Unit {
  if let Err(failure) = io.write_stderr_bytes(data) { diagnostic_write_error(failure) }
  if let Err(failure) = io.flush_stderr() { diagnostic_write_error(failure) }
}

## Writes help text, adding a final newline when absent.
export proc help(text: Str) [process, io] -> Unit {
  write_text(if text.ends_with("\n") { text } else { f"{text}\n" })
}

## Writes the independent XSH identity banner through the same failure handling.
export proc version(name: Str) [process, io] -> Unit {
  write_text(gnu.version_text(name) + "\n")
}
