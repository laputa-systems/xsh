type TypeContractNested = {values: List[UInt?]}
type TypeContractEntry = {path: Path, name: Str, ext: Str, kind: Str}
type TypeContractMetadata = {size: Int}
type TypeContractCounts = {blanks: Int, blobs: Map[Int], code: Int, comments: Int}
type TypeContractWrongCounts = {blobs: Map[Str]}
type TypeContractModule = module {
  export let name: Str
  export optional let description: Str
  export pure label(value: Str) -> Str
}
type TypeContractExactModule = exact module {
  export let name: Str
  export pure label(value: Str) -> Str
}
type TypeContractSmallModule = exact module { export let name: Str }
type TypeContractWrongModule = module { export let label: Str }

test test_runtime_type_tests_check_nested_records_maps_and_unsigned_domains {
  let fields: Any = {first: 1, second: 2}
  assert [fields is Map[Int], fields is Map[Int, Int]] == [true, false]
  let mixed: Any = {first: 1, second: "two"}
  assert ! (mixed is Map[Int])
  let positive: Any = {[1]: "one", [2]: "two"}
  let negative: Any = {[-1]: "negative"}
  assert positive is Map[UInt, Str]
  assert ! (negative is Map[UInt, Str])
  let unique: Any = set.from([1, 2])
  let signed: Any = set.from([-1, 2])
  assert unique is Set[UInt]
  assert ! (signed is Set[UInt])

  let values: List[UInt?] = [1, null, 2]
  let accepted: Any = [{values, extra: "kept"}]
  let rejected: Any = [{values: [1, -2]}]
  assert [accepted is List[TypeContractNested]] == [true]
  assert ! (rejected is List[TypeContractNested])
  assert accepted.require(List[TypeContractNested]) is Ok(_)
}

test test_runtime_stream_type_tests_and_dynamic_arguments_do_not_pull { |ctx|
  let root = test.temp_dir(ctx, name: "type-stream")?
  let marker = fp"{root}/pulled"
  let executed = test.run_xsh(
    ctx,
    r"""stream values(marker: Path) [fs, error] -> Stream[Int] {
  marker.write("pulled")
  yield 7
}
proc hold(value: Stream[Int]) [] -> Int { 1 }
let marker = env.Path.XSH_TYPE_STREAM_MARKER?
let held: Any = values(marker)
print (held is Stream[Int])
print (held is Stream[Str])
let callable: Proc = hold
print (callable.call(held)?.require(Int)?)
print marker.exists()?
""",
    env: {XSH_TYPE_STREAM_MARKER: marker},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == "true\ntrue\n1\nfalse\n", executed.stdout
  assert ! marker.exists()?
}

test test_runtime_filesystem_entry_checks_keep_metadata_unavailable { |ctx|
  let root = test.temp_dir(ctx, name: "type-entry")?
  fp"{root}/file.txt".write("entry")
  let entry: Any = fs.files(root, stat: false, gitignore: false) |> first()?
  assert [
    entry is Record,
    entry is TypeContractEntry,
    entry is TypeContractMetadata,
    entry is Map[Any],
    entry is Map[Int, Any],
  ] == [true, true, false, false, false]
}

test test_runtime_optimized_record_checks_borrow_stats_fields {
  let empty: Map[Int] = {}
  let stats: Any = {blanks: 1, blobs: empty, code: 2, comments: 3}
  let populated: Map[Int] = {text: 4}
  let stats_blob: Any = {blanks: 1, blobs: populated, code: 2, comments: 3}
  assert [
    stats is TypeContractCounts,
    stats_blob is TypeContractCounts,
    stats is Record,
    stats_blob is Record,
    stats_blob is TypeContractWrongCounts,
  ] == [true, true, false, false, false]
}

test test_runtime_mock_type_contract_accepts_process_errors_as_errors { |ctx|
  let failure = try run.text false
  test.mock(ctx, "dns.resolve_host", {name: "borrowed.test"}, failure)
  let response = dns.resolve_host("borrowed.test")
  assert response is Err(_ is ProcessError)
}

test test_runtime_module_type_tests_keep_optional_exact_and_callable_kinds { |ctx|
  let root = test.temp_dir(ctx, name: "type-module")?
  let file = fp"{root}/module.xsh"
  file.write("""##! A runtime type-check fixture.
## The module name.
export let name = "borrowed"
## Labels a value.
export pure label(value: Str) -> Str { value }
""")
  let loaded: Any = module.load(file)?
  assert [
    loaded is TypeContractModule,
    loaded is TypeContractExactModule,
    loaded is TypeContractSmallModule,
    loaded is TypeContractWrongModule,
  ] == [true, true, false, false]
}

test test_runtime_environment_path_view_checks_the_materialized_list {
  env PATH="/one:/two" {
    let paths: Any = env.PATH
    assert [paths is EnvPathList, paths is List[Path]] == [false, true]
  }
}
