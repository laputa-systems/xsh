type BuildOptions = {
  label: Str = "build",
  arguments: List[Str] = [],
}

error BuildError = Missing(message: Str) : NotFound

pure command_target(command: List[Str]) {
  if let ["build", target, ..rest] = command {
    [target, @rest]
  } else {
    ["ignored"]
  }
}

pure valid_name(name: Str) {
  rx"^[a-z]+$".matches(name)
}

stream child() [io] -> Stream[Int] {
  defer { print "child-close" }
  yield @[1, 2, 3]
}

stream parent() [io] -> Stream[Int] {
  defer { print "parent-close" }
  yield @child()
  yield 4 when true
}

let label = "ship"
let original = BuildOptions(label:)
var options = original
options.arguments += ["build", "app", "fast"]
print ${original.arguments.len()} ${valid_name(options.label)}
var commands: Map[List[Str]] = {}
commands["first"] = options.arguments
let selected = [argument for {value, ..} in commands for argument in command_target(value)]
print ${selected.join(",")}
var pending = [1, 2]
var total = 0
while let [head, ..tail] = pending {
  total += head
  pending = tail
}
print $total
let outcome: Result[Str, BuildError] = Err(BuildError.Missing(message: "absent"))
let recovered = outcome ?? { |failure|
  if failure is NotFound { failure.message } else { "other" }
}
print $recovered
let first = parent() |> take(1) |> collect()
print ${first[0]}
