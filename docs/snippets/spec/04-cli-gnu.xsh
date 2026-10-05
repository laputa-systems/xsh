## An applet with GNU getopt_long grammar.
proc main(...argv: List[Str]) [error, io] {
  let opts = cli.applet(
    argv,
    {
      gnu: {prog: "demo", status: 2, unsupported: {"--selinux": "SELinux labels are not available"}},
      all: {form: "-a --all", default: false},
      almost_all: {form: "-A --almost-all", default: false},
      color: {form: "--color[=WHEN]", default: "never", optional_default: "always"},
      lines: {form: "-n --lines N", kind: "Int", default: 10, numeric: true},
      ignore: {form: "-I --ignore PATTERN", repeated: true},
      help: {form: "--help", default: false, stop: true},
      paths: {form: "...FILE"},
    },
  )?

  if opts.help {
    print "usage: demo [OPTION]... [FILE]..."
    return
  }

  print f"{opts.lines} lines, color {opts.color}, {opts.paths.len()} files"
}
