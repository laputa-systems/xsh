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
