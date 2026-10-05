error JsonShape = NotScalar

pure scalar_label(v: Any) -> Result[Str, JsonShape] {
  match v {
    _ is Null => "null"
    b is Bool => if b { "true" } else { "false" }
    i is Int => f"integer {i}"
    f is Float => f"float {f}"
    s is Str => f"string of {s.count_chars()} characters"
    else => Err(.NotScalar("expected a scalar"))
  }
}
