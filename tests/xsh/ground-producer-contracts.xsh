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
