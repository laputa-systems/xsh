const options = {jobs: {kind: "Int", form: "-j --jobs N", default: 4, positive: true}}

proc descriptor_values() [fs, error] -> Result[Str] {
  let strict = cli.parse(["--jobs", "6"], options)?
  let full = cli.parse_full([], options)?
  let parsed_applet = cli.applet(["-j2", "-j3"], options)?
  f"${strict.jobs}/${full.values.jobs}/${parsed_applet.jobs}"
}
