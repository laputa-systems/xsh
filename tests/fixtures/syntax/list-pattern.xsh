let values = ["build", "kernel"]
let exact = values is ["build", _]
let prefix = values is ["build", _, ..]
let other = values is ["clean", _]
let nested = [[1], [2]] is [[_], [_]]
let selected = match values {
  ["build", # Keep the selected command explanation.
    target, ..] => target
  _ => "other"
}
