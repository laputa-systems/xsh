type RecordBuild = {jobs: Int, target: Str}
type RecordConfig = {root: Str, build: RecordBuild}

test test_nested_renamed_record_binding_preserves_field_types [error] {
  let config = RecordConfig(root: "src", build: {jobs: 3, target: "native"})
  let {root, build: {jobs, target: target_name, ..}, ..} = config
  (root) == ("src")
  (jobs + 1) == (4)
  (target_name.upper()) == ("NATIVE")
}

test test_nested_record_var_values_preserve_source_aliases [error] {
  let config = RecordConfig(root: "src", build: {jobs: 3, target: "native"})
  var {build: selected_build, root: _, ..} = config
  selected_build.jobs = 9
  (config.build.jobs) == (3)
  var {build: {jobs: worker_count, target: _, ..}, ..} = config
  worker_count += 2
  (worker_count) == (5)
  (config.build.jobs) == (3)
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
  (total) == (4)
  let selected = [target_name for {build: {jobs, target: target_name, ..}, ..} in configs if jobs > 1]
  (selected) == (["native"])
  let counts = {root: jobs for {root, build: {jobs, ..}, ..} in configs}
  ((counts.get("src") ?? 0)) == (3)
}

pure record_config_result() -> Result[RecordConfig] {
  Ok({root: "src", build: {jobs: 3, target: "native"}})
}

test test_nested_record_guard_target [error] {
  guard let {root, build: {jobs, target: target_name, ..}, ..} = record_config_result() else {
    return
  }
  (root) == ("src")
  (jobs) == (3)
  (target_name) == ("native")
}

test test_nested_record_iteration_restores_shadowed_outer_bindings [error] { |ctx|
  let output = test.run_script(ctx, r"""type RecordBuild = {jobs: Int, target: Str}
type RecordConfig = {root: Str, build: RecordBuild}
proc witness() [error] {
  let jobs = 40
  let configs: List[RecordConfig] = [{root: "src", build: {jobs: 3, target: "native"}}]
  for {build: {jobs, ..}, ..} in configs {
    (jobs) == (3)
    for {build: {jobs, ..}, ..} in configs {
      (jobs) == (3)
    }
    (jobs) == (3)
  }
  (jobs) == (40)
  let selected = [jobs for {build: {jobs, ..}, ..} in configs]
  (selected) == ([3])
  (jobs) == (40)
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  output.stdout == ""
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("source\nsrc:3:native\nnative\nclosed\n")
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
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("")
  ("check.destructure-type" in output.stderr)
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
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("closed\n")
  ("schema check failed at nested: missing required field missing" in output.stderr)
}
