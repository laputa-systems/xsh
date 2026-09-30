proc greet(name: Str) -> Str {
  return name
}

let name = "world"
let options = {name: name}
let greeting = greet(...options)
let punned = greet(name:)
print $greeting
print $punned
