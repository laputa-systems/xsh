type BuildEntry = {name: Str, outputs: List[Path], argv: List[Union[Str, Path]]}
var build_trace = ""
pure retain_build_rows(value) { value }
pure build_nonempty_marker(marker, values: NonEmpty[Path]) { let _ = values; marker }
proc build_word(value: Str) [] -> Str { build_trace += value; value }
proc build_paths(values: List[Path], failing: Bool) [error] -> List[Path] {
  defer { build_trace += "d" }
  build_trace += "s"
  if failing { assert false }
  values
}
proc rewritten_build_rows(rows: List[BuildEntry], failing: Bool) [error] -> List[BuildEntry] {
  let items: List[BuildEntry] = [
    {...row, argv: [p"/head", build_word("a"), @build_paths(row.outputs, failing), build_word("z")]}
    for row in rows
  ]
  retain_build_rows(items)
}
proc verify_generic_comprehension_list_builds() [error] -> Result[Unit] {
  let empty: List[Path] = []
  let rows: List[BuildEntry] = [
    {name: "one", outputs: [p"/one", p"/two"], argv: ["old"]},
    {name: "empty", outputs: empty, argv: ["old"]},
  ]
  assert build_nonempty_marker(7, [p"/head", @empty]) == 7
  let bounded: NonEmpty[Path] = [p"/bounded"]
  assert build_nonempty_marker(7, [@bounded]) == 7
  let actual = rewritten_build_rows(rows, false)
  assert actual[0].name == "one"
  let first_expected: List[Union[Str, Path]] = [p"/head", "a", p"/one", p"/two", "z"]
  assert actual[0].argv == first_expected
  assert actual[1].name == "empty"
  let empty_expected: List[Union[Str, Path]] = [p"/head", "a", "z"]
  assert actual[1].argv == empty_expected
  assert build_trace == "asdzasdz"
  build_trace = ""
  let failure = try { rewritten_build_rows(rows, true) }
  assert failure is Err(_)
  assert build_trace == "asd"
}
