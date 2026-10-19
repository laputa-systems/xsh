use typing-capability-import-int as integer

let value: Int = integer.value
let name: Int = integer.name
let marker: Int = integer.source_marker
assert value == 42
assert name == 7
assert marker == 17
print $value
print $name
print $marker
