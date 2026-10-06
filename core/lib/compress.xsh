##! Compression applets share byte streaming and publish files only after a
##! complete codec operation. Source removal follows successful publication.
use gnu

type OptionToken = {kind: Str, name: Str, value: Str}

# Compression levels begin with digits, which generic flag tokenization treats
# as negative-number operands. Interpret them as options until an explicit --.
pure option_tokens(argv: List[Str], format: Str) -> List[OptionToken] {
  var operands_only = false
  collect {
    for arg in argv {
      if ! operands_only and arg == "--" { operands_only = true; continue }
      if operands_only or arg == "-" or ! arg.starts_with("-") {
        yield {kind: "operand", name: arg, value: ""}
      } else if arg.starts_with("--") {
        let raw = arg.byte_slice(2)
        let equals = raw.find("=")
        if equals != null {
          yield {kind: "long", name: raw.byte_slice(0, length: equals), value: raw.byte_slice(equals + 1)}
        } else {
          yield {kind: "long", name: raw, value: ""}
        }
      } else {
        var at = 1
        while at < arg.byte_len() {
          let start = at
          at += 1
          if format == "zstd" and "0123456789".find(arg.byte_slice(start, length: 1)) != null {
            while at < arg.byte_len() and "0123456789".find(arg.byte_slice(at, length: 1)) != null { at += 1 }
          }
          yield {kind: "short", name: arg.byte_slice(start, length: at - start), value: ""}
        }
      }
    }
  }
}

pure suffix(format: Str) -> Str {
  match format { "gz" => ".gz", "bz2" => ".bz2", "xz" => ".xz", "zstd" => ".zst", _ => ".lzma" }
}

pure decoded_name(name: Str, format: Str) -> Str? {
  let ending = suffix(format)
  if name.ends_with(ending) and name.byte_len() > ending.byte_len() {
    return name.byte_slice(0, length: name.byte_len() - ending.byte_len())
  }
  if format == "gz" and name.ends_with(".tgz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "bz2" and name.ends_with(".tbz2") { return name.byte_slice(0, length: name.byte_len() - 5) + ".tar" }
  if format == "bz2" and name.ends_with(".tbz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "bz2" and name.ends_with(".bz") { return name.byte_slice(0, length: name.byte_len() - 3) }
  if format == "xz" and name.ends_with(".txz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "lzma" and name.ends_with(".tlz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "zstd" and name.ends_with(".tzst") { return name.byte_slice(0, length: name.byte_len() - 5) + ".tar" }
  if format == "bz2" { return name + ".out" }
  null
}

pure combined_status(current: Int, incoming: Int, format: Str) -> Int {
  if format == "bz2" {
    if incoming > current { incoming } else { current }
  } else if current == 1 or incoming == 1 {
    1
  } else {
    if incoming > current { incoming } else { current }
  }
}

## Run one compression applet with the supplied codec and alias defaults.
export proc execute(argv: List[Str], format: Str, decoding: Bool, cat: Bool) [fs, io, error, process, env] {
  var decode = decoding
  var stdout = cat
  var keep = format == "zstd"
  var force = false
  var testing = false
  var quiet = false
  var verbose = false
  var name_mode: Bool? = null
  var level = if format == "bz2" { 9 } else if format == "zstd" { 3 } else { 6 }
  var ultra = false
  var names = []
  for token in option_tokens(argv, format) {
    if token.kind == "operand" { names += [token.name]; continue }
    let option = token.name
    if token.value != "" and ! (format == "zstd" and option == "fast") { gnu.usage_error(f"option '--{option}' does not allow an argument") }
    if option in ["c", "stdout", "to-stdout"] {
      stdout = true
    } else if option in ["d", "decompress", "uncompress"] {
      decode = true
    } else if option in ["z", "compress"] {
      decode = false
    } else if option in ["k", "keep"] {
      keep = true
    } else if option == "rm" and format == "zstd" {
      keep = false
    } else if option in ["f", "force"] {
      force = true
    } else if option in ["t", "test"] {
      testing = true
      decode = true
    } else if option in ["n", "no-name"] and format == "gz" {
      name_mode = false
    } else if option in ["N", "name"] and format == "gz" {
      name_mode = true
    } else if option in ["q", "quiet"] {
      quiet = true
    } else if option in ["v", "verbose"] {
      verbose = true
    } else if option == "fast" {
      if format == "zstd" {
        let speed = if token.value == "" { 1 } else { token.value.parse_int()? }
        if speed < 1 or speed > 131072 { gnu.usage_error("fast level must be between 1 and 131072") }
        level = -speed
      } else { level = 1 }
    } else if option == "ultra" and format == "zstd" {
      ultra = true
    } else if option == "best" {
      level = 9
    } else if token.kind == "short" and rx"^[0-9]+$".matches(option) {
      level = option.parse_int()?
      if level > (if format == "zstd" { 22 } else { 9 }) or (format in ["gz", "bz2"] and level == 0) { gnu.usage_error("invalid compression level") }
    } else if option in ["h", "help"] {
      gnu.help(f"Usage: {gnu.prog()} [OPTION]... [FILE]...\nCompress or decompress files; no FILE or '-' reads standard input.\n  -c, --stdout      write to standard output\n  -d, --decompress  decompress\n  -k, --keep        keep input files\n  -f, --force       overwrite output files\n  -t, --test        check compressed data integrity\n  -1 .. -9          compression level\n")
      return
    } else if option in ["V", "version"] { gnu.version(gnu.prog()); return } else { gnu.usage_error(f"unrecognized option '{if token.kind == "short" { "-" } else { "--" }}{option}'") }
  }
  if format == "zstd" and level > 19 and ! ultra { gnu.usage_error("levels above 19 require --ultra") }
  let metadata = name_mode ?? ! decode
  if names.is_empty() { names = ["-"] }
  var status = 0
  for name in names {
    let to_stdout = stdout or name == "-"
    let source: Path? = if name == "-" { null } else { fp"{name}" }
    var destination: Path? = null
    if ! to_stdout and ! testing {
      guard let info = fs.stat(fp"{name}", follow_symlinks: force) else { |failure|
        if ! quiet { gnu.name_error(name, failure) }
        status = combined_status(status, 1, format)
        continue
      }
      if info.kind != "file" or (! force and info.nlink > 1) {
        if ! quiet { gnu.error(f"{name}: not a regular file with one link -- skipped") }
        status = combined_status(status, if format in ["bz2", "zstd"] { 1 } else { 2 }, format)
        continue
      }
      if decode {
        let output = decoded_name(name, format)
        if output == null {
          if ! quiet { gnu.error(f"{name}: unknown suffix -- ignored") }
          status = combined_status(status, if format in ["bz2", "zstd"] { 1 } else { 2 }, format)
          continue
        }
        destination = fp"{output}"
        if format == "gz" and metadata {
          match compression.gzip_name(fp"{name}") {
            Ok(original) => { if original != null { destination = fp"{fp"{name}".parent()}/{original}" } }
            Err(failure) => { if ! quiet { gnu.name_error(name, failure) }; status = combined_status(status, 1, format); continue }
          }
        }
      } else {
        if name.ends_with(suffix(format)) and (format != "gz" or ! force) {
          if ! quiet { gnu.error(f"{name} already has {suffix(format)} suffix -- unchanged") }
          status = combined_status(status, if format in ["bz2", "zstd"] { 1 } else { 2 }, format)
          continue
        }
        destination = fp"{name}{suffix(format)}"
      }
    }
    if source == null and ! testing and ! decode and unix.isatty(0) and ! force {
      if ! quiet { gnu.error("compressed data not read from a terminal; use -f to force") }
      status = combined_status(status, 1, format)
      continue
    }
    if to_stdout and ! testing and ! decode and unix.isatty(1) and ! force {
      if ! quiet { gnu.error("compressed data not written to a terminal; use -f to force") }
      status = combined_status(status, 1, format)
      continue
    }
    match compression.transform(source, destination, format, decode: decode, level: level, test: testing, metadata: metadata, overwrite: force, pass_through: force and decode and to_stdout and ! testing) {
      Ok(_) => {
        if destination != null and ! keep {
          if let Err(failure) = fp"{name}".remove(missing_ok: false) {
            if ! quiet { gnu.name_error(name, failure) }
            status = combined_status(status, 1, format)
          }
        }
        if verbose and ! quiet { gnu.error(f"{name}: {if testing { "OK" } else { "done" }}") }
      }
      Err(failure) => {
        if ! quiet { gnu.name_error(name, failure) }
        status = combined_status(status, if format == "bz2" and decode and gnu.errno(failure) == 0 { 2 } else { 1 }, format)
      }
    }
  }
  exit status
}
