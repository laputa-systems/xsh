# begin example
pure boxed(value) { [value] }
pure successful(value) { Ok(value) }
pure named(value) { {value: value} }
pure choose(flag, first, second) {
  if flag { first } else { second }
}
assert boxed(42) == [42]
assert boxed("demo") == ["demo"]
assert successful(42) == Ok(42)
assert successful("demo") == Ok("demo")
assert named(false).value == false
assert choose(true, 42, 0) == 42
assert choose(false, "first", "second") == "second"
# end example
