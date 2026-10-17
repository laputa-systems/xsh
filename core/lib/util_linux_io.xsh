##! Output and write-failure behavior for the util-linux command cohort.

use lib.gnu

proc write_error() [process] {
  gnu.error("write error")
  exit 1
}

proc flush_output() [process, io] {
  if let Err(_) = io.flush_stdout() { write_error() }
}

## Writes bytes and flushes stdout; a failed write reports NAME: write error and exits 1.
export proc write_bytes(data: Bytes) [process, io] {
  if let Err(_) = io.write_stdout_bytes(data) { write_error() }
  flush_output()
}

## Writes text and flushes stdout with the util-linux write-failure diagnostic.
export proc write_text(text: Str) [process, io] {
  if let Err(_) = io.write_stdout(text) { write_error() }
  flush_output()
}

## Writes help text with exactly one required final newline.
export proc help(text: Str) [process, io] {
  write_text(if text.ends_with("\n") { text } else { f"{text}\n" })
}

## Writes the independent XSH identity banner through the same failure handling.
export proc version(name: Str) [process, io] {
  write_text(gnu.version_text(name) + "\n")
}
