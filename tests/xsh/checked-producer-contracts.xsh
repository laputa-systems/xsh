test test_empty_list_yielded_into_a_typed_collect_has_a_producer_contract { |ctx|
  let output = test.expect(
    ctx,
    r"""type Entry = {raw: Bool, rest: List[Str]}
pure parse(lines: List[Str]) -> List[Entry] {
  let entries: List[Entry] = collect {
    for line in lines {
      yield {raw: true, rest: []}
    }
  }
  entries
}
pure nested(lines: List[Str]) -> List[List[Str]] {
  let rows: List[List[Str]] = collect {
    for line in lines {
      yield []
    }
  }
  rows
}
print ${parse(["a", "b"]).len()} ${nested(["a"]).len()}
""",
    status: 0,
  )?
  assert output.stdout == "2 1\n"
}

test test_typed_list_binding_accepts_conditional_branches_with_differing_item_types { |ctx|
  let output = test.expect(
    ctx,
    r"""pure targets(users: List[Str]) -> List[Str?] {
  let targets: List[Str?] = if users.is_empty() { [null] } else { [name for name in users] }
  targets
}
print ${targets([]).len()} ${targets(["a", "b"]).len()}
""",
    status: 0,
  )?
  assert output.stdout == "1 2\n"
}

test test_conditional_value_blocks_narrowed_by_a_null_test_keep_their_record_contract { |ctx|
  let output = test.expect(
    ctx,
    r"""type Count = {saturated: Bool, value: Int}
pure pick(all: Bool, requested: Count?) -> Int {
  var count = {saturated: false, value: 4}
  if requested != null {
    count = if all { requested } else { requested }
  }
  if count.saturated { 0 } else { count.value }
}
print ${pick(true, null)} ${pick(false, {saturated: false, value: 9})} ${pick(true, {saturated: true, value: 9})}
""",
    status: 0,
  )?
  assert output.stdout == "4 9 0\n"
}

test test_error_transfer_in_a_proc_with_a_plain_return_type_is_bounded_by_its_error_effect { |ctx|
  test.expect(
    ctx,
    r"""proc read(flag: Bool) [error] -> Int {
  if ! flag { Err(error.failure("refused"))? }
  1
}
print ${read(true)}
read(false)
""",
    status: 3,
    stdout: ["1"],
    stderr: ["refused"],
  )?
}

test test_method_on_an_empty_list_literal_takes_its_receiver_contract_from_the_result { |ctx|
  let output = test.expect(
    ctx,
    r"""pure merge(names: List[Str]) -> List[Str] {
  let merged = [].extend(names)
  merged
}
print ${merge(["x", "y"]).len()}
""",
    status: 0,
  )?
  assert output.stdout == "2\n"
}

test test_equality_between_dynamic_record_fields_has_a_producer_contract { |ctx|
  let output = test.expect(
    ctx,
    r"""proc main() [error] {
  let decoded = json.decode("{\"a\": {\"A\": 5, \"B\": 5}}")?
  let entry = json.get(decoded, ["a"])?.require(Record)?
  assert entry.B == entry.A
  print "equal"
}
""",
    status: 0,
  )?
  assert output.stdout == "equal\n"
}

test test_exit_inside_a_captured_match_arm_has_no_result_to_propagate { |ctx|
  let output = test.expect(
    ctx,
    r"""proc main() [fs, process, error] {
  let outcome = try {
    match fs.stat(p"/", follow_symlinks: true) {
      Ok(_) => {}
      Err(problem) => { print "cannot stat"; exit 1 }
    }
    1
  }
  print "done"
}
""",
    status: 0,
  )?
  assert output.stdout == "done\n"
}

test test_aggregate_arguments_without_an_item_type_take_the_imported_parameter_contract { |ctx|
  let root = test.temp_dir(ctx, name: "aggregate-arguments")?
  fp"{root}/layout.xsh".write_atomic(r"""##! Tab layout helpers.
## Tab stop layout.
export type Tabs = {stops: List[Int], interval: Int, relative: Bool}
## One report row.
export type Row = {name: Str, value: Int}
## Rows grouped for one report.
export type Report = {schema: Int, rows: List[Row]}
## Column of the next stop.
export pure next_stop(spec: Tabs, column: Int, unit = 1) -> Int {
  spec.interval * unit + column
}
""")
  let output = test.expect(
    ctx,
    r"""use layout
let width = 8
print ${layout.next_stop({stops: [], interval: width, relative: false}, 2, unit: width / 8)}
let report = layout.Report(schema: 1, rows: collect {})
print ${report.rows.len()}
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == "10\n0\n"
}

test test_constructor_list_field_of_dynamic_records_accepts_an_empty_literal { |ctx|
  let output = test.expect(
    ctx,
    r"""type Member = {key: Str, value: Any}
type Session = {members: List[Member], notes: List[Str]}
pure make() -> Int {
  var session: Session = Session(members: [], notes: [])
  session.notes.len()
}
print ${make()}
""",
    status: 0,
  )?
  assert output.stdout == "0\n"
}
