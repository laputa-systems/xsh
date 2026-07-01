var tries = 0

loop {
  tries += 1
  break when tries >= 3
}

print $tries
var i = 0

while i < 5 {
  i += 1
  continue when i == 3
  print $i
}

let found = loop {
  i += 1

  if i >= 8 {
    break i
  }
}

print $found
