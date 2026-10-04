const root = p"."
const expected = 2
let actual = 1 + 1
let entries = fs.files(root)?.collect()
# begin example
assert actual == expected
assert entries.len() > 0, f"no entries under {root}"
# end example
