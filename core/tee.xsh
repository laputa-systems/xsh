#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tee [OPTION]... [FILE]...
Copy standard input to each FILE, and also to standard output.

  -a, --append              append to the given FILEs, do not overwrite
  -i, --ignore-interrupts   ignore interrupt signals
  -p                        operate in a more appropriate MODE with pipes.
      --output-error[=MODE]   set behavior on write error.  See MODE below
  -h, --help        display this help and exit
      --version     output version information and exit

MODE determines behavior with write errors on the outputs:
  'warn'         diagnose errors writing to any output
  'warn-nopipe'  diagnose errors writing to any output not a pipe
  'exit'         exit on error writing to any output
  'exit-nopipe'  exit on error writing to any output not a pipe
The default MODE for the -p option is 'warn-nopipe'.
With "nopipe" MODEs, exit immediately if all outputs become broken pipes.
The default operation when --output-error is not specified, is to
exit immediately on error writing to a pipe, and diagnose errors
writing to non pipe outputs.
"""

const MODES = ["warn", "warn-nopipe", "exit", "exit-nopipe"]

type TeeOptions = {
  append: Bool,
  pipe: Bool,
  output_error: Str,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# GNU `argmatch`: an exact name wins, otherwise a unique prefix.
proc resolve_mode(text: Str) [process, env] -> Str {
  return text when text in MODES

  let found = [mode for mode in MODES if mode.starts_with(text)]

  return found[0] when found.len() == 1

  let kind = if found.is_empty() { "invalid" } else { "ambiguous" }
  gnu.error(f"{kind} argument {gnu.quote_value(text)} for '--output-error'")
  eprint "Valid arguments are:"

  for mode in MODES {
    eprint f"  - '{mode}'"
  }

  gnu.try_help()
  exit 1
}

# Each output keeps one open descriptor for the entire copy. Rebinding
# stdout lets the checked byte flush preserve short writes, append offsets,
# FIFO readers and device behavior without reopening an operand per chunk.
type Output = {name: Str, fd: Int, active: Bool, pollable: Bool}

# Only pipes and sockets use hangup/error readiness for broken readers.
# Device errors are reported by the actual write, even in nopipe modes.
proc output_pollable(fd: Int) [fs, error] -> Result[Bool] {
  let metadata = fs.stat(fp"/dev/fd/{fd}", follow_symlinks: true)?
  Ok(metadata.kind in ["fifo", "socket"])
}

proc write_output(output: Output, data: Bytes) [process, error, io] -> Result[Unit] {
  unix.dup_fd(output.fd, 1)?
  io.write_stdout_bytes(data)?
  io.flush_stdout()
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: TeeOptions = cli.applet(
    argv,
    {
      gnu: {
        status: 1,
        unsupported: {
          "-i": "interrupts cannot be ignored",
          "--ignore-interrupts": "interrupts cannot be ignored",
        },
      },
      append: {
        form: "-a --append",
        default: false,
      },
      pipe: {
        form: "-p",
        default: false,
        conflicts: [
          "output_error",
        ],
      },
      output_error: {
        form: "--output-error[=MODE]",
        default: "",
        optional_default: "warn-nopipe",
        conflicts: [
          "pipe",
        ],
      },
      help: {
        form: "-h --help",
        default: false,
        stop: true,
      },
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      files: {
        form: "...FILE",
      },
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("tee")
    return
  }

  let mode = if opts.pipe {
    "warn-nopipe"
  } else if opts.output_error == "" {
    ""
  } else {
    resolve_mode(opts.output_error)
  }

  let original = unix.open_fd(/dev/null)?
  unix.dup_fd(1, original)?
  var outputs: List[Output] = [{name: "standard output", fd: original, active: true, pollable: output_pollable(original)?}]
  var failed = false

  for name in opts.files {
    let descriptor = unix.open_fd(/dev/null)?
    if let Err(failure) = unix.redirect_fd(descriptor, fp"{name}", write: true, append: opts.append) {
      unix.close_fd(descriptor)?
      gnu.name_error(name, failure)
      failed = true
      if mode.starts_with("exit") { exit 1 }
    } else {
      outputs += [{name: name, fd: descriptor, active: true, pollable: output_pollable(descriptor)?}]
    }
  }

  loop {
    if mode.ends_with("nopipe") {
      for index in range(outputs.len()) {
        let output = outputs[index]
        continue when ! output.active or ! output.pollable
        let events = unix.poll_fd(output.fd, [])?
        if "invalid" in events {
          gnu.error(f"{gnu.quote(output.name)}: Bad file descriptor")
          failed = true
          outputs[index] = {...output, active: false}
        } else if "error" in events or "hangup" in events {
          outputs[index] = {...output, active: false}
          if index > 0 { unix.close_fd(output.fd)? }
        }
      }
    }
    break when [output for output in outputs if output.active].is_empty()
    # A bounded readiness wait keeps checking outputs while the producer is
    # idle; a blocking read alone would miss a pipe that loses its reader.
    if mode.ends_with("nopipe") and unix.poll_fd(0, ["readable"], timeout_ms: 50)?.is_empty() { continue }
    guard let data = io.stdin_read(32768) else { |failure|
      gnu.error(f"read error: {gnu.strerror(failure)}")
      failed = true
      break
    }
    break when data.is_empty()

    # Exit modes report failures across the current chunk before stopping.
    var stop_after_chunk = false
    for index in range(outputs.len()) {
      let output = outputs[index]
      continue when ! output.active
      if let Err(failure) = write_output(output, data) {
        let pipe = gnu.errno(failure) == 32
        if pipe and mode == "" { exit 141 }
        let ignored = pipe and mode.ends_with("nopipe")
        if ! ignored {
          if index == 0 { gnu.error(f"'standard output': {gnu.strerror(failure)}") } else { gnu.name_error(output.name, failure) }
          failed = true
        }
        outputs[index] = {...output, active: false}
        if index > 0 { unix.close_fd(output.fd)? }
        if mode.starts_with("exit") and ! ignored { stop_after_chunk = true }
      }
    }
    break when stop_after_chunk
  }

  unix.dup_fd(original, 1)?
  for output in outputs {
    if output.active and output.fd != original { unix.close_fd(output.fd)? }
  }
  unix.close_fd(original)?
  if failed { exit 1 }
}
