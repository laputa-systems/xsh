const options = {jobs: {kind: "Int", form: "-j --jobs N", default: 4, positive: true}}
const spread_options = {count: {kind: "Int", form: "-c --count N", default: 3}}

proc descriptor_values() [fs, error] -> Result[Str] {
  let strict = cli.parse(["--jobs", "6"], options)?
  let full = cli.parse_full([], options)?
  let parsed_applet = cli.applet(["-j2", "-j3"], options)?
  f"{strict.jobs}/{full.values.jobs}/{parsed_applet.jobs}"
}

proc descriptor_spread_values() [fs, error] {
  let _ = cli.applet(["-c8"], ...{schema: spread_options})?
}
