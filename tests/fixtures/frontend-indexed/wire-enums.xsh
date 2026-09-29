enum WireState: Str { Ready = "ready", Empty = "" }
type WireRow = {state: WireState, values: List[WireState], optional: WireState?}

pure wire_round_trip(source: Str) -> Result[Str] {
  let row = json.decode(source)?.require(WireRow)?
  return json.encode(row)
}

pure wire_direct() -> Result[Str] {
  return json.encode(Ready)
}
