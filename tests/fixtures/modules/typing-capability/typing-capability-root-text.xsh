use typing-capability-import-text as text

let value: Str = text.value
let name: Str = text.name
let marker: Int = text.source_marker
assert value == "demo"
assert name == "demo"
assert marker == 17
print $value
print $name
print $marker
