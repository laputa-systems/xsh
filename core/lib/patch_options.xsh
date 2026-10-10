##! Option interpretation GNU patch does beyond getopt: the order of its
##! long options, version-control style names, and which format switch wins.

# The order GNU patch declares its long options in. getopt lists the
# candidates of an ambiguous abbreviation in this order.
const LONG_OPTION_ORDER = ["backup", "prefix", "context", "directory", "ifdef", "ed", "remove-empty-files", "force", "fuzz", "get", "input", "ignore-whitespace", "normal", "forward", "output", "strip", "reject-file", "reverse", "quiet", "silent", "batch", "set-time", "unified", "version", "version-control", "debug", "basename-prefix", "suffix", "set-utc", "dry-run", "verbose", "binary", "help", "backup-if-mismatch", "no-backup-if-mismatch", "posix", "quoting-style", "reject-format", "read-only", "follow-symlinks", "merge"]

## Reorder the candidates in an `is ambiguous; possibilities:` message.
export pure reorder_candidates(message: Str) -> Str {
  let marker = "; possibilities: "
  let at = message.find(marker) ?? -1
  if at < 0 { return message }
  let head = message.byte_slice(0, at + marker.byte_len())
  var names: List[Str] = []
  for part in message.byte_slice(at + marker.byte_len()).split(" ") {
    if part != "" { names += [part] }
  }
  var ordered: List[Str] = []
  for known in LONG_OPTION_ORDER {
    for name in names { if name == f"'--{known}'" { ordered += [name] } }
  }
  for name in names { if !(name in ordered) { ordered += [name] } }
  head + ordered.join(" ")
}

## Resolve a `-V` argument to none, simple, existing, or numbered; abbreviations
## are accepted when they name one style. Anything else is "".
export pure version_style(text: Str) -> Str {
  let table = [
    {name: "none", style: "none"}, {name: "off", style: "none"},
    {name: "simple", style: "simple"}, {name: "never", style: "simple"},
    {name: "existing", style: "existing"}, {name: "nil", style: "existing"},
    {name: "numbered", style: "numbered"}, {name: "t", style: "numbered"},
  ]
  for entry in table { if entry.name == text { return entry.style } }
  var found = ""
  for entry in table {
    if text != "" and entry.name.starts_with(text) {
      if found != "" and found != entry.style { return "" }
      found = entry.style
    }
  }
  found
}

## The diff format the switches select: the last of -u, -c, -n, and -e wins, as
## with getopt processing in order; "any" when none was given.
export pure chosen_format(argv: List[Str]) -> Str {
  var format = "any"
  for argument in argv {
    if argument == "--" { break }
    if argument.starts_with("--") {
      let name = argument.byte_slice(2).split("=")[0]
      if name != "" {
        for long in ["unified", "context", "normal", "ed"] {
          if long.starts_with(name) and (name == long or (name.byte_len() > 1 and name != "no")) {
            format = if long == "unified" { "unified" } else if long == "context" { "context" } else if long == "normal" { "normal" } else { "ed" }
          }
        }
      }
    } else if argument.starts_with("-") and argument != "-" {
      for letter in argument.byte_slice(1) {
        if letter == "u" { format = "unified" } else if letter == "c" { format = "context" } else if letter == "n" { format = "normal" } else if letter == "e" { format = "ed" }
        # Letters that take a value end the cluster.
        if letter in ["p", "F", "d", "D", "i", "o", "r", "B", "Y", "z", "V", "g", "x"] { break }
      }
    }
  }
  format
}

