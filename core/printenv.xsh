#!/bin/xsh
proc main(...names: List[Str]) [env, error] {
  if names.len() == 0 {
    for item in env.list() |> sort-by .name {
      print f"${item.name}=${item.value}"
    }

    return
  }

  var missing = false

  for name in names {
    match env.get(name) {
      Ok(value) => print $value
      Err(_) => missing = true
    }
  }

  if missing {
    abort(1)
  }
}
