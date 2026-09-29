type Inner = {value: Str?, count: Int}
type Outer = {inner: Inner}
pure unreachable() -> Str { let _ = 1 / 0; "unreachable" }
pure verified() -> Str {
  var report: Outer = {inner: {value: "ready", count: 1}}
  let available = report.inner.value != null
  let retained = available
  report.inner.count = 2
  guard retained else { return "missing" }
  report.inner.value ?? unreachable()
}
