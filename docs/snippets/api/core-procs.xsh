const defaults = {name: "world"}

proc greet(name: Str) -> Str {
  name
}

let options = defaults
let greeting = greet(...options)
let name = options.name
let punned = greet(name:)
print $greeting
print $punned
