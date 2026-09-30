type RecordBuild = {jobs: Int, target: Str}
type RecordConfig = {root: Str, build: RecordBuild}

test test_nested_renamed_record_binding_preserves_field_types [error] {
  let config: RecordConfig = {root: "src", build: {jobs: 3, target: "native"}}
  let {root, build: {jobs, target: target_name, ..}, ..} = config
  test.eq(root, "src")?
  test.eq(jobs + 1, 4)?
  test.eq(target_name.upper(), "NATIVE")?
}

test test_nested_record_var_values_preserve_source_aliases [error] {
  let config: RecordConfig = {root: "src", build: {jobs: 3, target: "native"}}
  var {build: selected_build, root: _, ..} = config
  selected_build.jobs = 9
  test.eq(config.build.jobs, 3)?
  var {build: {jobs: worker_count, target: _, ..}, ..} = config
  worker_count += 2
  test.eq(worker_count, 5)?
  test.eq(config.build.jobs, 3)?
}

test test_nested_record_iteration_and_comprehension_targets [error] {
  let configs: List[RecordConfig] = [
    {root: "src", build: {jobs: 3, target: "native"}},
    {root: "lib", build: {jobs: 1, target: "other"}},
  ]
  var total = 0
  for {build: {jobs: workers, target: _, ..}, root: _, ..} in configs {
    total += workers
  }
  test.eq(total, 4)?
  let selected = [target_name for {build: {jobs, target: target_name, ..}, ..} in configs if jobs > 1]
  test.eq(selected, ["native"])?
  let counts = {root: jobs for {root, build: {jobs, ..}, ..} in configs}
  test.eq((counts.get("src") ?? 0), 3)?
}

pure record_config_result() -> Result[RecordConfig] {
  return Ok({root: "src", build: {jobs: 3, target: "native"}})
}

test test_nested_record_guard_target [error] {
  guard let {root, build: {jobs, target: target_name, ..}, ..} = record_config_result() else {
    return
  }
  test.eq(root, "src")?
  test.eq(jobs, 3)?
  test.eq(target_name, "native")?
}

test test_nested_record_iteration_restores_shadowed_outer_bindings [error] {
  let jobs = 40
  let configs: List[RecordConfig] = [{root: "src", build: {jobs: 3, target: "native"}}]
  for {build: {jobs, ..}, ..} in configs {
    test.eq(jobs, 3)?
    for {build: {jobs, ..}, ..} in configs {
      test.eq(jobs, 3)?
    }
    test.eq(jobs, 3)?
  }
  test.eq(jobs, 40)?
  let selected = [jobs for {build: {jobs, ..}, ..} in configs]
  test.eq(selected, [3])?
  test.eq(jobs, 40)?
}

test test_nested_record_source_once_and_stream_cleanup [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
type Build = {jobs: Int, target: Str}
type Config = {root: Str, build: Build}
proc make_config() [io] -> Config {
  print "source"
  return {root: "src", build: {jobs: 3, target: "native"}}
}
let {root, build: {jobs, target: target_name, ..}, ..} = make_config()
print "\${root}:\${jobs}:\${target_name}"
proc close() [io] { print "closed" }
stream configs() [io, error] -> Stream[Config] {
  defer close()
  yield {root: "src", build: {jobs: 3, target: "native"}}
  print "unreached"
}
for {build: {target: target_name, ..}, ..} in configs() {
  print \${target_name}
  break
}
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "source\nsrc:3:native\nnative\nclosed\n")?
}

test test_nested_record_dynamic_stream_target_requires_validation [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
proc close() [io] { print "closed" }
stream records() [io, error] -> Stream[Record] {
  defer close()
  yield {first: 1, nested: {present: 2}}
  print "unreached"
}
for {first, nested: {missing, ..}, ..} in records() {
  print "body"
}
""",
  )?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "")?
  test.contains(output.stderr, "check.destructure-type")?
}


test test_nested_record_dynamic_stream_validation_failure_runs_cleanup [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
type Nested = {missing: Int}
type Selected = {first: Int, nested: Nested}
proc close() [io] { print "closed" }
stream records() [io, error] -> Stream[Record] {
  defer close()
  yield {first: 1, nested: {present: 2}}
  print "unreached"
}
for row in records() {
  let {first, nested: {missing, ..}, ..} = row.require(Selected)?
  print "body"
}
""",
  )?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "closed\n")?
  test.contains(output.stderr, "schema check failed at nested: missing required field missing")?
}
