type TextRow = {value: Str}
type NumberRow = {value: Int}

pure text_values(rows: List[TextRow]) -> List[Str] { [row.value for row in rows] }
pure number_values(rows: List[NumberRow]) -> List[Int] { [row.value for row in rows] }
pure dynamic_values(rows: List[Record]) -> List[Record] { rows }

proc read_text_rows(stored: Record) -> Result[List[Str]] {
  let rows = if "rows" in stored { stored.get("rows")?.require(List[TextRow])? } else { [] }
  text_values(rows)
}

proc read_number_rows(stored: Record) -> Result[List[Int]] {
  let rows = if "rows" in stored { stored.get("rows")?.require(List[NumberRow])? } else { [] }
  number_values(rows)
}

proc read_dynamic_rows(stored: Record) -> Result[List[Record]] {
  let rows = if "rows" in stored { stored.get("rows")?.require(List[Record])? } else { [] }
  dynamic_values(rows)
}

proc record_list_conditionals() -> Result[Unit] {
  assert read_text_rows({rows: [{value: "kept"}]})? == ["kept"]
  assert read_text_rows({})? == []
  assert read_number_rows({rows: [{value: 7}]})? == [7]
  assert read_number_rows({})? == []
  let opaque: Result[Int, Str] = Err("opaque")
  let values: List[Record] = [{payload: opaque}]
  assert read_dynamic_rows({rows: values})? == values
  assert read_dynamic_rows({})? == []
  match try { read_number_rows({rows: [{value: "wrong"}]})? } {
    Err(_) => {}
    Ok(_) => assert false
  }
}
