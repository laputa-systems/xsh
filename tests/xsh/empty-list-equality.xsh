test test_empty_list_equality_uses_the_existing_counterpart_type [error] {
  "".wrap(5) == []
  [] == "".wrap(5)
  "".fields() == []
  [] == "".fields()
  ["word"] != []
  [] != ["word"]
  !(["word"] == [])
  !([] == ["word"])
  let empty: List[Str] = []
  empty == []
  [] == empty
}

test test_empty_list_equality_preserves_incompatible_and_unresolved_sources [error] { |ctx|
  for source in [
    "print \"executed\"\nlet compared = [1] == [\"one\"]\n",
    "print \"executed\"\nlet empty: List[Int] = []\nlet compared = empty == [\"one\"]\n",
    "pure compared() -> Bool { [] == [] }\nprint \"executed\"\n",
  ] {
    let output = test.run_script(ctx, source)?
    output.status == 2
    "check." in output.stderr
    output.stdout == ""
  }
  let independent = test.run_script(ctx, "pure compared(value) { value == [] }\nprint \"executed\"\n")?
  independent.status == 2
  "empty list equality needs an independently established List operand" in independent.stderr
  independent.stdout == ""
}
