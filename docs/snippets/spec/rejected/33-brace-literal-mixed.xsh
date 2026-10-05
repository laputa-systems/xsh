const name = "core"
# begin example
let entry = {"kind", name: name} # error: parse.brace-literal-mixed
let later = {name: name, "kind"} # error: parse.brace-literal-mixed
# end example
