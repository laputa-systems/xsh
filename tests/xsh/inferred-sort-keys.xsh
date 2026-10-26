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

type DynamicSortRow = {value: Any, revision: Str?, rank: Int}

pure sorted_dynamic_rows(value: Any) -> List[DynamicSortRow] {
  var rows = []
  for rank in [2, 1] {
    rows += [{value, revision: null, rank}]
  }
  rows |> sort-by .rank
}

test inferred_sort_keys_preserve_nullable_siblings_of_any_fields {
  let rows = sorted_dynamic_rows("kept")
  assert rows.len() == 2
  assert rows[0].rank == 1
  assert rows[1].rank == 2
  assert rows[0].revision == null
  assert rows[1].revision == null
  assert rows[0].value.require(Str)? == "kept"
  assert rows[1].value.require(Str)? == "kept"
}
