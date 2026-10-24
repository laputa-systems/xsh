pure null_test(value) -> Bool {
  if value == null { true } else { false }
}

proc verify_guarded_benign() -> Result[Unit] {
  let opaque: Any = {hidden: true}
  assert null_test(null)
  assert !null_test("plain")
  assert !null_test(opaque)
  Ok()
}
