let before = run.text pwd ?

cd tests {
  let inside = run.text pwd ?
  print $inside != $before
} ?

let after = run.text pwd ?
print $after == $before
