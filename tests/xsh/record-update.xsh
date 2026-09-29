test test_record_update_nested_spread_before_witness [error] {
  let config = {build: {jobs: 2, flags: {debug: false, optimize: true}}, name: "demo"}
  let updated = {
    ...config,
    build: {...config.build, jobs: 8, flags: {...config.build.flags, debug: true}},
  }
  test.eq(updated.build.jobs, 8)?
  test.eq(updated.build.flags.debug, true)?
  test.eq(updated.build.flags.optimize, true)?
  test.eq(config.build.jobs, 2)?
  test.eq(config.build.flags.debug, false)?
}

test test_record_update_disjoint_paths_match_nested_spreads [error] {
  let config = {build: {jobs: 2, flags: {debug: false, optimize: true}}, name: "demo"}
  let before = {...config, build: {...config.build, jobs: 8, flags: {...config.build.flags, debug: true}}}
  let after = {...config, build.jobs: 8, build.flags.debug: true}
  test.ok(after == before)?
  test.eq(config.build.jobs, 2)?
  test.eq(config.build.flags.debug, false)?
}

type RecordUpdateFlags = {debug: Bool, optimize: Bool}
type RecordUpdateBuild = {jobs: Int, flags: RecordUpdateFlags, tags: List[Str]}
type RecordUpdateConfig = {build: RecordUpdateBuild, name: Str = "default"}

test test_record_update_keeps_schema_and_contextual_replacements [error] {
  let config = RecordUpdateConfig(build: {jobs: 2, flags: {debug: false, optimize: true}, tags: ["old"]}, name: "kept")
  let name = "renamed"
  let updated: RecordUpdateConfig = {...config, build.jobs: 8, build.tags: [], name}
  test.eq(updated.build.tags.len(), 0)?
  test.eq(updated.build.jobs, 8)?
  test.eq(updated.name, "renamed")?
  test.eq(config.name, "kept")?
  test.eq(config.build.tags[0], "old")?
  let quoted = {"build.jobs": 7}
  test.eq(quoted.get("build.jobs")?, 7)?
}

test test_record_update_evaluates_snapshot_and_rhs_in_source_order [error] { |ctx|
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "base\njobs\noptimize\noriginal,8,false,true\noriginal,false,changed,true\n")?
}

test test_record_update_failure_stops_later_rhs_and_keeps_published_value [error] { |ctx|
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "first\nfailure\nclosed\ncaught=7\noriginal,2,false\n")?
}
