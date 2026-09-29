let name = "tasks"
let seed: Map[Int] = {total: 3}
let counts = {...seed, [name]: 2, total: 4}
print counts.keys().join(",")
print (counts.get(name) ?? 0)
