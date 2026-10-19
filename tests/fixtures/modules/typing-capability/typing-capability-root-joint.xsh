use typing-capability-import-int as integer
use typing-capability-import-text as text

let marker = 99

let integer_value: Int = integer.value
let integer_name: Int = integer.name
let text_value: Str = text.value
let text_name: Str = text.name
let integer_marker: Int = integer.source_marker
let text_marker: Int = text.source_marker
assert integer_value == 42
assert integer_name == 7
assert text_value == "demo"
assert text_name == "demo"
assert integer_marker == 17
assert text_marker == 17
print $integer_value
print $integer_name
print $text_value
print $text_name
print $integer_marker
print $text_marker
