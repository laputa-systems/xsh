var tries = 0

while tries < 4 {
  tries += 1

  if tries == 2 {
    print "two"
    continue
  }

  break when tries == 4
  print $tries
}
