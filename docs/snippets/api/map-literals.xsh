let name = "tasks"
let seed: Map[Int] = {total: 3}
let counts = {...seed, [name]: 2, total: 4}
print counts.keys().join(",")
print (counts.get(name) ?? 0)

let attempts: Map[Int, Str] = {[2]: "retry", [0]: "initial"}
for {key: attempt, value: label} in attempts {
  print f"${attempt}: ${label}"
}
