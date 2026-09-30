const defaults = {jobs: 4}
pure maybe_name() -> Str? { "label" }
pure name(value = maybe_name()) -> Str? { value }
pure config(value = defaults) -> Int { value.jobs }
pure outcome() -> Result[Int] { Ok(7) }
pure result(value = outcome()) -> Result[Int] { value }
print ${name() ?? "missing"} ${name(null) ?? "missing"} ${config()} ${result()?}
