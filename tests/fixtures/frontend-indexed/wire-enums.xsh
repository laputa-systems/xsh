enum WireState: Str { Ready = "ready", Empty = "" }
type WireRow = {state: WireState, values: List[WireState], optional: WireState?}

pure wire_round_trip(source: Str) -> Result[Str] {
  let row = json.decode(source)?.require(WireRow)?
  return json.encode(row)
}

pure wire_direct() -> Result[Str] {
  return json.encode(Ready)
}

type WireBox[T] = {packet: T}
type WireNested = WireBox[WireRow]
pure wire_nested(source: Str) -> Result[Str] {
  let nested = json.decode(source)?.require(WireNested)?
  return json.encode(nested)
}

const prepared_wire_state = Ready
pure wire_prepared() -> Result[Str] {
  return json.encode(prepared_wire_state)
}
