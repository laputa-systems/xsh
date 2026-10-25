test test_checked_list_storage_accepts_conditional_arms_with_null_and_empty_lists { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    let nulls: List[Str?] = match -4 { 4 => [], 3 => ctx "label" { ["c"] }, _ => [null] }
    let mixed: List[Str?] = if nulls.len() == 1 { ["c"] } else { ["c", null] }
    let empty: List[Str] = ctx "label" { [] }
    let appended: List[Bool] = [true, false] + []
    let braced: List[Int?] = { [86549, null] }
    out += [f"{nulls.len()}", f"{mixed.len()}", f"{empty.len()}", f"{appended.len()}", f"{braced.len()}"]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\n1\n0\n2\n2\n"
}

test test_negated_float_literal_in_list_receiver_lowers_as_float { |ctx|
  let output = test.run_script(
    ctx,
    r"""const C0: Int = 18
enum E0 { E0V0(Int, Int), E0V1(Float, Int), E0V2 }
proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    let first: E0 = [E0V0(3, 15), E0V1(-7.75, 33)].get(1) ?? E0V2
    match first {
      E0V1(weight, _) => { out += [f"{weight}"] }
      _ => {}
    }
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "-7.75\n"
}

test test_optional_field_guard_inside_optional_record_guard_narrows_the_read { |ctx|
  let output = test.run_script(
    ctx,
    r"""type R0 = {f0: Int?, f1: Str}
proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    let counted: R0? = R0(f0: 4, f1: "a")
    let uncounted: R0? = R0(f0: null, f1: "b")
    let absent: R0? = null
    out += [if counted == null { "null" } else { if counted.f0 == null { "none" } else { f"{counted.f0}" } }]
    out += [if uncounted == null { "null" } else { if uncounted.f0 == null { "none" } else { f"{uncounted.f0}" } }]
    out += [if absent == null { "null" } else { if absent.f0 == null { "none" } else { f"{absent.f0}" } }]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\nnone\nnull\n"
}

test test_noncompleting_block_statements_discard_no_value { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    var count = 0
    while count < 3 {
      count += 1
      {
        break when count == 2
      }
    }
    out += [f"{count}"]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n"
}

test test_map_comprehension_arguments_and_membership_containers_have_a_producer_contract { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc measure(counts: Map[Int, Str], bonus: Int = 1) -> Int { counts.len() + bonus }
proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    out += [f"{measure(counts: {[7]: "a" for x in [1, 2, 3]})}"]
    out += [f"{measure(...{counts: {[x]: "b" for x in [1, 2, 3]}, bonus: 10})}"]
    out += [f"{2 in {[x]: x for x in [1, 2]}}"]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n13\ntrue\n"
}

test test_conditional_method_receiver_uses_each_arms_narrowed_type { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    let name: Str? = if true { "ab" } else { "cd" }
    if name != null {
      out += [(if false { name } else { "ß" }).upper()]
      out += [(if true { name } else { "ß" }).upper()]
    }
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "SS\nAB\n"
}

test test_empty_map_call_survives_argument_reordering_and_conditional_storage { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc keyed(counts: Map[Str], bonus: Int = 9) -> Int { counts.len() + bonus }
proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    out += [f"{keyed(map.empty(), bonus: 3)}"]
    let counts: Map[Int?] = match 6 { 3 => map.empty(), 4 => {["a"]: null}, _ => {["b"]: 5, ["c"]: null} }
    out += [f"{counts.len()}"]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "3\n2\n"
}

test test_type_tested_dynamic_receiver_resolves_its_method { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure refine(value: Any) -> Str {
  guard value is Str else {
    return "unknown"
  }
  value.trim()
}
proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    out += [refine(" a "), refine(3)]
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "a\nunknown\n"
}

test test_narrowed_optional_conditions_are_boolean_controls { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc fuzz_main() [] -> Result[List[Str]] {
  try {
    var out: List[Str] = []
    let flag: Bool? = if true { true } else { false }
    if flag != null {
      let direct: Int = if flag { 1 } else { 2 }
      let chosen: Int = if if false { flag } else { true } { 15 } else { 12 }
      out += [f"{direct}", f"{chosen}"]
    }
    out
  }
}
for line in fuzz_main()? { print ${line} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\n15\n"
}

test test_plain_run_value_is_a_status { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc check() [process] -> Result[Unit] {
  let status = run true
  guard status else {
    return error.fail("true failed")
  }
  print "ok"
}
check()?
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ok\n"
}
