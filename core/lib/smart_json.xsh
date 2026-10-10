##! An insertion-ordered JSON model and writer for smartctl's `--json` output.
##!
##! `json.encode` sorts object keys, but smartctl documents and emits its keys
##! in a fixed order (format version, smartctl block, device, then the data), so
##! the report is built from ordered members and written here with the same
##! two-space layout smartctl uses.

## One key and its value.
export type Member = {key: Str, value: Any}

## An object whose members keep the order they were given.
export type Object = {members: List[Member]}

## A member for an object fragment.
export pure m(key: Str, value: Any) -> Member {
  {key: key, value: value}
}

## An object value from ordered members.
export pure object(members: List[Member]) -> Object {
  {members: members}
}

pure quote(text: Str) -> Str {
  json.encode(text) ?? "\"\""
}

pure indent(depth: Int) -> Str {
  var out = ""
  for _ in range(depth) { out += "  " }
  out
}

## Renders a value with two-space indentation: one element or member per line,
## `[]` and `{}` for empty containers. `compact` writes one line without
## whitespace instead.
export pure render(value: Any, compact: Bool = false, depth: Int = 0) -> Str {
  if compact { return render_compact(value) }
  render_indented(value, depth)
}

pure render_compact(value: Any) -> Str {
  match value {
    _ is Null => "null"
    flag is Bool => if flag { "true" } else { "false" }
    number is Int => f"{number}"
    text is Str => quote(text)
    items is List[Any] => "[" + [render_compact(item) for item in items].join(",") + "]"
    node is Object => "{" + [quote(entry.key) + ":" + render_compact(entry.value) for entry in node.members].join(",") + "}"
    else => "null"
  }
}

pure render_indented(value: Any, depth: Int) -> Str {
  match value {
    _ is Null => "null"
    flag is Bool => if flag { "true" } else { "false" }
    number is Int => f"{number}"
    text is Str => quote(text)
    items is List[Any] => {
      if items.is_empty() { return "[]" }
      let inner = indent(depth + 1)
      var rows: List[Str] = []
      for item in items { rows += [inner + render_indented(item, depth + 1)] }
      "[\n" + rows.join(",\n") + "\n" + indent(depth) + "]"
    }
    node is Object => {
      if node.members.is_empty() { return "{}" }
      let inner = indent(depth + 1)
      var rows: List[Str] = []
      for entry in node.members { rows += [inner + quote(entry.key) + ": " + render_indented(entry.value, depth + 1)] }
      "{\n" + rows.join(",\n") + "\n" + indent(depth) + "}"
    }
    else => "null"
  }
}
