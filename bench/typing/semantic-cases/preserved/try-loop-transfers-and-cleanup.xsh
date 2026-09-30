var rounds = 0
while rounds < 3 {
  rounds += 1
  let captured: Result[Unit] = try {
    defer { print "cleanup" }
    continue when rounds < 3
    break
  }
}
print $rounds
