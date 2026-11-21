error E = A | B

proc f() -> Result[Int] {
  Err(E.A("x"))
}

let r = retry [10ms] on (is E.A or is E.B) { f()? }
