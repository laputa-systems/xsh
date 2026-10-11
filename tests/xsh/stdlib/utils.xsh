pure cached_label(name: Str) -> Str {
  f"cached {name}"
}

test test_utils_cache {
  assert utils.cache(cached_label, ["value"]) == "cached value"
  assert utils.cache(cached_label, ["value"]) == "cached value"
}

test test_utils_cache_runs_a_proc_once_for_a_miss_and_hit { |ctx|
  let output = test.expect(
    ctx,
    """
var calls = 0
proc counted(value: Int) -> Str {
  calls += 1
  f"cached {value}"
}
let first = utils.cache(counted, [7]).require(Str)?
let second = utils.cache(counted, [7]).require(Str)?
assert first == "cached 7"
assert second == first
assert calls == 1
print f"{first} {second} {calls}"
""",
    status: 0,
  )?
  assert output.stdout == "cached 7 cached 7 1\n"
}

test test_utils_cache_memoizes_a_result_error { |ctx|
  let output = test.expect(
    ctx,
    """
var calls = 0
proc rejected(value: Int) -> Result[Int, Str] {
  calls += 1
  Err("rejected")
}
let first = utils.cache(rejected, [7]).require(Result[Int, Str])?
let second = utils.cache(rejected, [7]).require(Result[Int, Str])?
assert first == Err("rejected")
assert second == first
assert calls == 1
print $calls
""",
    status: 0,
  )?
  assert output.stdout == "1\n"
}

test test_utils_cache_recurses_on_the_calling_frame_machine { |ctx|
  let output = test.expect(
    ctx,
    """
proc cached_depth(value: Int) -> Int {
  if value == 0 { 0 } else { utils.cache(cached_depth, [value - 1]).require(Int)? + 1 }
}
assert utils.cache(cached_depth, [160]).require(Int)? == 160
print "completed"
""",
    status: 0,
  )?
  assert output.stdout == "completed\n"
}

test test_utils_cache_preserves_trace_events_for_hits_and_misses { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    """
pure identity(value: Str) -> Str { value }
let first = utils.cache(identity, ["same"]).require(Str)?
let second = utils.cache(identity, ["same"]).require(Str)?
assert first == second
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: succeeded, stderr: details, ..} = traced
    assert succeeded, details
  }
  assert traced.stderr.split("\"kind\":\"module.call\"").len() == 3, traced.stderr
  assert traced.stderr.split("\"kind\":\"module.result\"").len() == 3, traced.stderr
  assert traced.stderr.split("\"kind\":\"pure.enter\"").len() == 2, traced.stderr
  assert traced.stderr.split("\"kind\":\"pure.exit\"").len() == 2, traced.stderr
}

test test_utils_cache_closes_trace_events_when_the_callback_fails { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    """
proc failing(value: Int) -> Int { 1 / (value - value) }
let _ = utils.cache(failing, [1])
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  assert ! traced.success, traced.stderr
  assert traced.stderr.split("\"kind\":\"module.call\"").len() == 2, traced.stderr
  assert traced.stderr.split("\"kind\":\"module.result\"").len() == 2, traced.stderr
}

test test_utils_cache_keeps_a_returned_resource_root_live { |ctx|
  let output = test.expect(
    ctx,
    """
var calls = 0
proc create_root(label: Str) [fs, error] -> FsRoot {
  calls += 1
  fs.tempdir()?
}
let first_root = utils.cache(create_root, ["root"]).require(FsRoot)?
let second_root = utils.cache(create_root, ["root"]).require(FsRoot)?
first_root.write(p"cached", "value")?
assert second_root.read_text(p"cached")? == "value"
assert calls == 1
first_root.close()?
""",
    status: 0,
  )?
  assert output.success, output.stderr
}

test test_utils_cache_rejects_uncacheable_arguments_before_calling { |ctx|
  test.expect(
    ctx,
    """
proc callback(value: Result[Int]) [io] -> Int {
  print "called"
  1
}
let _ = utils.cache(callback, [Ok(1)])
""",
    status: 3,
    stdout: [],
    stderr: ["cache-key-error", "Result"],
  )?
}
