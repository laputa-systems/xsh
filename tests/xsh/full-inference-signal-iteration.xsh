test test_generic_iteration_keeps_diverging_pattern_value { |ctx|
  for declaration in [
    r"""proc validate(names: List[Str]) [process, error] {
  for name in names {
    let signal = if let Ok(signal) = process.signal(name) { signal } else { exit 125 }
    let _: Int = signal.number
  }
}""",
    r"""proc validate(names) [process, error] {
  for name in names {
    let signal = if let Ok(signal) = process.signal(name) { signal } else { exit 125 }
    let _: Int = signal.number
  }
}""",
  ] {
    let result = test.expect(ctx, declaration + "\nvalidate([\"TERM\"])\n", status: 0)?
    assert result.stderr == "", result.stderr
    let rejected = test.expect(ctx, declaration + "\nvalidate([\"NOT_A_SIGNAL\"])\n", status: 125)?
    assert rejected.stderr == "", rejected.stderr
  }
}

test test_diverging_pattern_success_does_not_supply_a_value { |ctx|
  for declaration in [
    r"""proc validate(names: List[Str]) [process, error] {
  for name in names {
    let value = if let Ok(signal) = process.signal(name) { let _ = signal; exit 125 } else { 7 }
    let _: Int = value
  }
}""",
    r"""proc validate(names) [process, error] {
  for name in names {
    let value = if let Ok(signal) = process.signal(name) { let _ = signal; exit 125 } else { 7 }
    let _: Int = value
  }
}""",
  ] {
    let result = test.expect(ctx, declaration + "\nvalidate([\"NOT_A_SIGNAL\"])\n", status: 0)?
    assert result.stderr == "", result.stderr
    let exited = test.expect(ctx, declaration + "\nvalidate([\"TERM\"])\n", status: 125)?
    assert exited.stderr == "", exited.stderr
  }
}

test test_diverging_return_branch_keeps_its_owner { |ctx|
  for declaration in [
    r"""pure first(values: List[Int]) -> Int {
  for item in values { let value = if item == 0 { return 9 } else { item }; return value }
  0
}""",
    r"""pure first(values) -> Int {
  for item in values { let value = if item == 0 { return 9 } else { item }; return value }
  0
}""",
  ] {
    let result = test.expect(ctx, declaration + "\nassert first([0]) == 9\nassert first([4]) == 4\n", status: 0)?
    assert result.stderr == "", result.stderr
  }
}
