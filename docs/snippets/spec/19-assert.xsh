const root = p"."
const expected = 2
let actual = 1 + 1
let entries = fs.files(root)?.collect()
# begin example
assert actual == expected
assert ! entries.is_empty(), f"no entries under {root}"
# end example
