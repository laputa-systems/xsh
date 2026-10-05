# begin example
error BuildError = Failed(stage: Str, message: Str) | Exited(code: Int, tool: Str, log: Path)

let swapped = BuildError.Failed("link", "undefined symbol")  # error: check.positional-error-arguments
let named = BuildError.Failed(stage: "link", message: "undefined symbol")
let leading = BuildError.Exited(1, "ld", log: p"build/ld.log")
let late = BuildError.Exited(tool: "ld", log: p"build/ld.log", 1)  # error: check.positional-error-arguments
# end example
