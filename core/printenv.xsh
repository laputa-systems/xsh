#!/bin/xsh
proc main(...names: List[Str]) [env, error] {
  if names.is_empty() {
    for item in env.list() |> sort-by .name {
      print f"{item.name}={item.value}"
    }

    return
  }

  var missing = false

  for name in names {
    if let Ok(value) = env.get(name) {
      print $value
    } else {
      missing = true
    }
  }

  if missing {
    exit 1
  }
}
