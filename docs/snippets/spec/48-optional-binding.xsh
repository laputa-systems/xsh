type Executor = {name: Str, slots: Int}

type Context = {executor: Executor?, retries: Int?}

# begin example
pure executor_name(context: Context) -> Str {
  guard let executor = context.executor else {
    return "none"
  }

  executor.name
}

pure attempts(context: Context) -> Int {
  if let retries = context.retries { retries + 1 } else { 1 }
}

# end example
assert executor_name({executor: null, retries: null}) == "none"
assert attempts({executor: null, retries: 2}) == 3
