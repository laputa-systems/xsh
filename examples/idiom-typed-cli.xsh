type Opts = {pattern: Str, verbose: Bool, limit: Int}

let argv = ["--pattern", "proc", "--verbose", "--limit", "20"]

let opts: Opts = cli.parse(
  argv,
  {
    pattern: {
      form: "--pattern PATTERN",
      required: true,
    },
    verbose: {
      form: "--verbose",
      default: false,
    },
    limit: {
      form: "--limit N",
      default: 50,
    },
  },
)?

print f"${opts.pattern} ${opts.verbose} ${opts.limit}"
