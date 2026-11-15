proc draw(count: Int) [env] -> Result[Bytes] {
  random.bytes(count)
}

test test_random_bytes {
  assert (draw(0)?).len() == 0
  assert (draw(1)?).len() == 1
  assert (draw(4096)?).len() == 4096
  assert draw(32)? != draw(32)?
  match draw(-1) {
    Ok(_) => assert false
    Err(error) => assert error.message == "count cannot be negative"
  }
}

test test_random_effect { |ctx|
  let rejected = test.run_script(ctx, "pure draw() -> Result[Bytes] { random.bytes(1) }\n")?
  assert rejected.status == 2
  assert "check.pure-effect" in rejected.stderr, rejected.stderr
  let accepted = test.run_script(ctx, "proc draw() [env] -> Result[Bytes] { random.bytes(1) }\nlet _ = draw()\n")?
  assert accepted.status == 0, accepted.stderr
}
