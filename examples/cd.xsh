let before = run.text pwd ?

cd examples {
  let inside = run.text pwd ?
  let changed = inside != before
  print $changed
}

let after = run.text pwd ?
let same = after == before
print $same
