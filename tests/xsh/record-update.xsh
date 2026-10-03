test test_record_update_nested_spread_before_witness { |ctx|
  let output = test.run_script(ctx, r"""
proc witness() [error] {
  let config = {build: {jobs: 2, flags: {debug: false, optimize: true}}, name: "demo"}
  let updated = {
    ...config,
    build: {...config.build, jobs: 8, flags: {...config.build.flags, debug: true}},
  }
  (updated.build.jobs) == (8)
  (updated.build.flags.debug) == (true)
  (updated.build.flags.optimize) == (true)
  (config.build.jobs) == (2)
  (config.build.flags.debug) == (false)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_record_update_disjoint_paths_match_nested_spreads { |ctx|
  let output = test.run_script(ctx, r"""
proc witness() [error] {
  let config = {build: {jobs: 2, flags: {debug: false, optimize: true}}, name: "demo"}
  let before = {...config, build: {...config.build, jobs: 8, flags: {...config.build.flags, debug: true}}}
  let after = {...config, build.jobs: 8, build.flags.debug: true}
  (after == before)
  (config.build.jobs) == (2)
  (config.build.flags.debug) == (false)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_record_update_keeps_schema_and_contextual_replacements { |ctx|
  let output = test.run_script(ctx, r"""type RecordUpdateFlags = {debug: Bool, optimize: Bool}
type RecordUpdateBuild = {jobs: Int, flags: RecordUpdateFlags, tags: List[Str]}
type RecordUpdateConfig = {build: RecordUpdateBuild, name: Str = "default"}
proc witness() [error] {
  let config = RecordUpdateConfig(build: {jobs: 2, flags: {debug: false, optimize: true}, tags: ["old"]}, name: "kept")
  let name = "renamed"
  let updated: RecordUpdateConfig = {...config, build.jobs: 8, build.tags: [], name}
  (updated.build.tags.len()) == (0)
  (updated.build.jobs) == (8)
  (updated.name) == ("renamed")
  (config.name) == ("kept")
  (config.build.tags[0]) == ("old")
  let quoted = {"build.jobs": 7}
  (quoted.get("build.jobs")?) == (7)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_record_update_evaluates_snapshot_and_rhs_in_source_order { |ctx|
  let output = test.run_xsh(ctx, r"""
type Flags = {debug: Bool, optimize: Bool}
type Build = {jobs: Int, flags: Flags}
type Config = {build: Build, name: Str}
proc traced(value: Config) [io] -> Config { print "base"; return value }
proc inspect() [io] {
var source: Config = {build: {jobs: 2, flags: {debug: false, optimize: false}}, name: "original"}
let alias = source
let updated = {
  ...traced(source),
  build.jobs: if true { source = {...source, name: "changed", build: {...source.build, flags: {...source.build.flags, debug: true}}}; print "jobs"; 8 } else { 0 },
  build.flags.optimize: if true { print "optimize"; source.build.flags.debug } else { false },
}
print f"${updated.name},${updated.build.jobs},${updated.build.flags.debug},${updated.build.flags.optimize}"
print f"${alias.name},${alias.build.flags.debug},${source.name},${source.build.flags.debug}"
}
inspect()
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert (output.stdout) == ("base\njobs\noptimize\noriginal,8,false,true\noriginal,false,changed,true\n")
}

test test_record_update_failure_stops_later_rhs_and_keeps_published_value { |ctx|
  let output = test.run_xsh(ctx, r"""
error UpdateError = Failed(code: Int)
type Flags = {debug: Bool}
type Build = {jobs: Int, flags: Flags}
type Config = {build: Build, name: Str}
proc first() [io] -> Int { print "first"; return 8 }
proc failed() [io, error] -> Result[Bool, UpdateError] { print "failure"; return Err(UpdateError.Failed(7)) }
proc later() [io] -> Str { print "later"; return "changed" }
proc update(value: Config) [io, error] -> Result[Config, UpdateError] {
  defer { print "closed" }
  return {...value, build.jobs: first(), build.flags.debug: failed()?, name: later()}
}
var published: Config = {build: {jobs: 2, flags: {debug: false}}, name: "original"}
match update(published) {
  Ok(value) => published = value
  Err(UpdateError.Failed {code}) => print f"caught=${code}"
}
print f"${published.name},${published.build.jobs},${published.build.flags.debug}"
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert (output.stdout) == ("first\nfailure\nclosed\ncaught=7\noriginal,2,false\n")
}

test test_record_update_rejects_bases_targets_and_replacements { |ctx|
  let prefix = "let config = {build: {jobs: 2, flags: {debug: false}}, name: \"demo\"}\n"
  for {source, code} in [
    {source: "let updated = {build.jobs: 8}\n", code: "check.record-update-base"},
    {source: "let extra = {name: \"other\"}\nlet updated = {...config, ...extra, build.jobs: 8}\n", code: "check.record-update-base"},
    {source: "let dynamic: Any = config\nlet updated = {...dynamic, build.jobs: 8}\n", code: "check.record-update-shape"},
    {source: "let updated = {...config, build.workers: 8}\n", code: "check.record-update-field"},
    {source: "let updated = {...config, name.size: 8}\n", code: "check.record-update-field"},
    {source: "let updated = {...config, extra: 1, build.jobs: 8}\n", code: "check.record-update-field"},
    {source: "let updated = {...config, build: {jobs: 1, flags: {debug: true}}, build.jobs: 8}\n", code: "check.record-update-overlap"},
    {source: "let updated = {...config, build.jobs: 1, build.jobs: 2}\n", code: "check.record-update-overlap"},
    {source: "let updated = {...config, build.jobs: \"many\"}\n", code: "check.type-mismatch"},
  ] {
    let output = test.run_script(ctx, prefix + source)?
    assert ! output.success, source
    assert code in output.stderr
  }
}
