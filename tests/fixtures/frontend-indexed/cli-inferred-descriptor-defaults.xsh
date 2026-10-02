const inferred_options = {
  jobs: {form: "-j --jobs N", default: 4, env: "JOBS"},
  optional: "Str",
  tag: {repeated: true},
  verbose: "Bool",
}

proc inferred_descriptor_values() [fs, error] -> Result[Str] {
  let defaulted = cli.parse_full([], inferred_options)?
  let from_env = cli.parse_full([], inferred_options, {JOBS: "8"})?
  let from_argv = cli.parse_full(["--jobs", "9"], inferred_options, {JOBS: "8"})?
  let parsed_applet = cli.applet(["-j2", "-j3"], inferred_options)?
  let jobs: Int = defaulted.values.jobs
  let optional: Str? = defaulted.values.optional
  let tags: List[Str] = defaulted.values.tag
  let verbose: Bool = defaulted.values.verbose
  let default_source = defaulted.sources.get("jobs")?.require(Str)?
  let env_source = from_env.sources.get("jobs")?.require(Str)?
  let argv_source = from_argv.sources.get("jobs")?.require(Str)?
  f"${jobs}/${from_env.values.jobs}/${from_argv.values.jobs}/${parsed_applet.jobs}/${tags.len()}/${verbose}/${default_source}/${env_source}/${argv_source}"
}

proc inferred_descriptor_duplicate() [fs, error] -> Result[Str] {
  let parsed = cli.parse(["-j2", "-j3"], inferred_options)?
  f"${parsed.jobs}"
}

proc inferred_descriptor_absent() [fs, error] -> Result[Str?] {
  let parsed = cli.parse([], inferred_options)?
  parsed.optional
}
