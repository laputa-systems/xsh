type Options = {xsht: Path, test_filter: Str, repeat: Int, min_duration_ms: Int}

let opts: Options = cli.parse(
  args,
  {
    xsht: {form: "--xsht PATH", kind: "Path", required: true},
    test_filter: {form: "--filter FILTER", default: ""},
    repeat: {form: "--repeat N", default: 1},
    min_duration_ms: {form: "--min-duration-ms N", default: 0},
  },
)?

if opts.repeat < 1 {
  eprint "--repeat must be at least 1"
  abort(2)
}

if opts.min_duration_ms < 0 {
  eprint "--min-duration-ms must be non-negative"
  abort(2)
}

let xsht = opts.xsht.resolve()?
let started = time.now()
var runs = 0

while runs < opts.repeat or opts.min_duration_ms > 0 and time.now() - started < opts.min_duration_ms {
  if opts.test_filter == "" {
    run $xsht test ?
  } else {
    run $xsht test $opts.test_filter ?
  }

  runs += 1
}

let duration_ms = time.now() - started
eprint f"xsh perf repeat: runs=${runs} duration_ms=${duration_ms}"
