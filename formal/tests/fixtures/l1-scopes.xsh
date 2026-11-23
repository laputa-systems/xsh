var trace: List[Int] = []
env ({XSH_PROBE_A: "inner"}) {
  defer { trace += [1] }
  cd (p"/") {
    defer { trace += [2] }
    trace += [7]
  }
}
for value in trace { print $value }
