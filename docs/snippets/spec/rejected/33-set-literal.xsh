const name = "core"
# begin example
let entry = {"kind", name: name} # error: parse.set-literal
let words = {"a", "b"} # error: parse.set-literal
let lengths = {word.len() for word in words} # error: parse.set-literal
# end example
