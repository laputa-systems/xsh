#!/bin/xsh
use lib.gnu

const USAGE = """Usage: readlink [OPTION]... FILE...
Print value of a symbolic link or canonical file name.

  -f, --canonicalize           canonicalize by following every symlink
  -e, --canonicalize-existing  canonicalize, requiring every component to exist
  -m, --canonicalize-missing   canonicalize, allowing missing components
  -n, --no-newline             do not output the trailing delimiter
  -q, -s, --quiet, --silent    suppress most error messages
  -v, --verbose                report errors
  -z, --zero                   end each output line with NUL, not newline
      --help                   display this help and exit
      --version                output version information and exit
"""

type ReadlinkOptions = {canonicalize: Bool, existing: Bool, missing: Bool, no_newline: Bool, quiet: Bool, verbose: Bool, zero: Bool, help: Bool, version: Bool, paths: List[Str]}

pure path_join(base: Path, parts: List[Str]) -> Path {
  var text = base.display()
  for part in parts {
    text = if text == "/" { f"/{part}" } else { f"{text}/{part}" }
  }
  fp"{text}"
}

proc canonical(target_arg: Path, cwd: Path, allow_missing: Bool, allow_all_missing: Bool) [fs] -> Path? {
  let original = target_arg.display()
  let trailing = original.ends_with("/")
  let absolute = if original.starts_with("/") { original } else { f"{cwd.display()}/{original}" }
  var parts = [part for part in absolute.split("/") if part != "" and part != "."]
  var current = p"/"
  var pending: List[Str] = []
  var index = 0
  var followed = 0
  while index < parts.len() {
    let part = parts[index]
    if part == ".." {
      if pending.len() > 0 { pending = pending |> take(pending.len() - 1) } else if current.display() != "/" { current = current.parent() }
      index += 1
      continue
    }
    let candidate = path_join(current, pending + [part])
    let resolution = candidate.resolve()
    if let Ok(resolved) = resolution {
      current = resolved
      pending = []
    } else {
      if let Err(failure) = resolution { return null when gnu.errno(failure) == 40 }
      if let Ok(target) = candidate.readlink() {
        followed += 1
        return null when followed > 40
        let parent = path_join(current, pending)
        let target_text = target.display()
        let expanded = if target_text.starts_with("/") { target_text } else { f"{parent.display()}/{target_text}" }
        parts = [item for item in expanded.split("/") if item != ""] + parts[index + 1..]
        current = p"/"
        pending = []
        index = 0
        continue
      }
      if allow_all_missing {
        pending += [part]
      } else if allow_missing and index == parts.len() - 1 {
        let parent = path_join(current, pending)
        if let Ok(meta) = fs.stat(parent) {
          return null when meta.kind != "dir"
          pending += [part]
        } else {
          return null
        }
      } else {
        return null
      }
    }
    index += 1
  }
  let result = path_join(current, pending)
  if trailing and ! allow_all_missing and pending.len() == 0 {
    if let Ok(meta) = fs.stat(result) { return null when meta.kind != "dir" } else { return null }
  }
  return null when trailing and pending.len() > 0 and ! (allow_missing and ! allow_all_missing)
  result
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: ReadlinkOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      canonicalize: {form: "-f --canonicalize", default: false},
      existing: {form: "-e --canonicalize-existing", default: false},
      missing: {form: "-m --canonicalize-missing", default: false},
      no_newline: {form: "-n --no-newline", default: false},
      quiet: {form: "-q -s --quiet --silent", default: false},
      verbose: {form: "-v --verbose", default: false},
      zero: {form: "-z --zero", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("readlink"); return }
  if opts.paths.len() == 0 { gnu.missing_operand() }

  let canonicalize = opts.canonicalize or opts.existing or opts.missing
  let cwd = fs.cwd()?
  let posix_correct = (env.get_or("POSIXLY_CORRECT", "") ?? "") != ""
  var verbose = posix_correct
  var quiet = false
  for arg in argv {
    if arg == "--verbose" { verbose = true; quiet = false }
    if arg == "--quiet" or arg == "--silent" { quiet = true; verbose = false }
    if arg.starts_with("-") and ! arg.starts_with("--") {
      var at = 1
      while at < arg.byte_len() {
        let option = arg.byte_slice(at, length: 1)
        if option == "v" { verbose = true; quiet = false }
        if option == "q" or option == "s" { quiet = true; verbose = false }
        at += 1
      }
    }
  }
  if posix_correct { verbose = true; quiet = false }
  let no_newline = opts.no_newline and opts.paths.len() == 1
  if opts.no_newline and opts.paths.len() > 1 {
    gnu.error("ignoring --no-newline with multiple arguments")
  }
  let ending = if no_newline { "" } else if opts.zero { "\0" } else { "\n" }
  var failed = false
  for name in opts.paths {
    let link_path = fp"{name}"
    var output: Path? = null
    if canonicalize {
      output = canonical(link_path, cwd, opts.canonicalize, opts.missing)
    } else if let Ok(target) = link_path.readlink() {
      output = target
    }
    if output == null {
      failed = true
      if verbose and ! quiet {
        let message = if let Err(failure) = link_path.resolve() { gnu.strerror(failure) } else if canonicalize { "No such file or directory" } else { "Invalid argument" }
        gnu.error(f"{gnu.quote_maybe(name)}: {message}")
      }
    } else {
      let result = output ?? p"/"
      gnu.write_text(f"{result.display()}{ending}")
    }
  }
  if failed { exit 1 }
}
