pure pipeline_join(prefix: Str, value: Str, suffix: Str) -> Str {
  prefix + value + suffix
}

let wrapped = "middle" |> pipeline_join("[", _, "]")
print $wrapped
