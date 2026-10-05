#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tee [OPTION]... [FILE]...
Copy standard input to each FILE, and also to standard output.

  -a, --append              append to the given FILEs, do not overwrite
  -i, --ignore-interrupts   ignore interrupt signals
  -p                        operate in a more appropriate MODE with pipes.
      --output-error[=MODE]   set behavior on write error.  See MODE below
      --help        display this help and exit
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

  let kind = if found.len() == 0 { "invalid" } else { "ambiguous" }
  gnu.error(f"{kind} argument {gnu.quote(text)} for '--output-error'")
  eprint "Valid arguments are:"

  for mode in MODES {
    eprint f"  - '{mode}'"
  }

  gnu.try_help()
  exit 1
}

# Write `data` to one output. An existing regular file is extended in place
# for `--append`; anything else (new file, device, FIFO) is opened and written.
proc write_output(name: Str, data: Bytes, append: Bool) [fs, error] {
  let target = fp"{name}"

  if append {
    if let Ok(resolved) = target.resolve() {
      let entry = resolved.metadata()?

      if entry.mode / 4096 % 16 == 8 {
        let _ = bytes.write_at(resolved, entry.size, data, true)?
        return
      }
    }
  }

  target.write(data)
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
        form: "--help",
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

  guard let data = io.stdin_bytes() else { |failure|
    gnu.error(f"read error: {gnu.strerror(failure)}")
    exit 1
  }

  gnu.write_bytes(data)
  var failed = false

  for name in opts.files {
    if let Err(failure) = write_output(name, data, opts.append) {
      let ignored = gnu.errno(failure) == 32 and mode.ends_with("nopipe")

      if ! ignored {
        gnu.name_error(name, failure)
        failed = true
      }

      if mode.starts_with("exit") and ! ignored {
        exit 1
      }
    }
  }

  if failed {
    exit 1
  }
}
