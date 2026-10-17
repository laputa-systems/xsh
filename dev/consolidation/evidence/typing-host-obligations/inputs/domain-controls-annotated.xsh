type Row = {value: Int}
pure record_keys(value: Row) -> List[Str] { value.keys() }
pure map_keys(value: Map[Str, Int]) -> List[Str] { value.keys() }
pure signed(value: Int) -> Float { value.float() }
pure unsigned(value: UInt) -> Float { value.float() }
assert record_keys({value: 1}) == ["value"]
assert map_keys(map.empty()).len() == 0
assert signed(2) == 2.0
assert unsigned(2) == 2.0
