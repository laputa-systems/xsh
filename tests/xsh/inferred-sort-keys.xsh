type SortRow = {name: Str, revision: Str?}

pure sorted_rows_typed() -> List[SortRow] {
  var rows: List[SortRow] = []
  for name in ["b", "a"] {
    rows += [{name, revision: null}]
  }
  rows |> sort-by .name
}

proc sorted_rows() -> Result[List[SortRow], Error] {
  var rows = []
  for name in ["b", "a"] {
    rows += [{name, revision: null}]
  }
  rows |> sort-by .name
}

pure sorted_rows_by_record() -> List[SortRow] {
  var rows = []
  for name in ["b", "a"] {
    rows += [{name, revision: null}]
  }
  rows |> sort-by { {name: .name} }
}

test inferred_sort_keys_wait_for_the_body_result_context {
  let rows = sorted_rows()?
  assert rows == sorted_rows_typed()
  assert rows == sorted_rows_by_record()
  assert rows[0].name == "a"
  assert rows[1].name == "b"
  assert rows[0].revision == null
  assert rows[1].revision == null
}
