test test_full_inference_explicit_ok_preserves_context_and_scope_results { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc render() [error] -> Result[Str] { Ok(ctx "rendering" { "page" }) }

proc render_from_directory() [env, fs, error] -> Result[Str] {
  let original = fs.cwd()?
  let text = cd (p"/") {
    assert fs.cwd()? == p"/"
    "directory page"
  }?
  assert fs.cwd()? == original
  Ok(text)
}

proc render_from_environment() [env, error] -> Result[Str] {
  let original = env.get_or("XSH_FULL_INFERENCE_SCOPE", "unset")?
  let text = env ({XSH_FULL_INFERENCE_SCOPE: "inner"}) {
    assert env.get_or("XSH_FULL_INFERENCE_SCOPE", "unset")? == "inner"
    "environment page"
  }?
  assert env.get_or("XSH_FULL_INFERENCE_SCOPE", "unset")? == original
  Ok(text)
}

assert render()? == "page"
assert render_from_directory()? == "directory page"
assert render_from_environment()? == "environment page"
print "scope results preserved"
""",
    env: {XSH_FULL_INFERENCE_SCOPE: "outer"},
  )?
  assert output.success, output.stderr
  assert output.stdout == "scope results preserved\n"
}

test test_full_inference_explicit_ok_preserves_host_result_payloads { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc text_result() [process, error] -> Result[Str] {
  Ok(run.text printf hello ?)
}

proc bytes_result() [process, error] -> Result[Bytes] {
  Ok(run.bytes printf bytes ?)
}

proc captured_text_result() [process, error] -> Result[Str] {
  let capture = run.capture --text printf "captured text"
  Ok(capture.stdout)
}

proc captured_bytes_result() [process, error] -> Result[Bytes] {
  let capture = run.capture --bytes printf "captured bytes"
  Ok(capture.stdout)
}

proc pipeline_capture_result() [process, error] -> Result[Str] {
  let capture = run.capture --text printf hello | run cat
  Ok(capture.stdout)
}

assert text_result()? == "hello"
assert bytes_result()? == b"bytes"
assert captured_text_result()? == "captured text"
assert captured_bytes_result()? == b"captured bytes"
assert pipeline_capture_result()? == "hello"
print "host results preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "host results preserved\n"
}

test test_full_inference_result_bindings_discard_only_without_propagation { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure failure() -> Result[Unit, Str] { Err("dropped") }

proc discard() [] -> Unit {
  let _ = failure()
  print "after discard"
}

proc discard_mutable() [] -> Unit {
  var _ = failure()
  print "after mutable discard"
}

proc propagate() [error] -> Result[Unit, Str] {
  let _ = failure()?
  print "after propagation"
}

discard()
discard_mutable()
let propagated = propagate()
assert propagated is Err("dropped")
print "after explicit propagation"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "after discard\nafter mutable discard\nafter explicit propagation\n"
}

test test_full_inference_late_byte_receivers_preserve_optional_comparisons { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure bytes_length(raw: Bytes) -> Int { raw.len() }
pure text_length(raw: Str) -> Int { raw.byte_len() }
pure bytes_probe(raw) {
  let equal = raw.byte_at(0) == 65
  let missing = raw.byte_at(9) == null
  let size = bytes_length(raw)
  {equal, missing, size}
}
pure text_probe(raw) {
  let equal = raw.byte_at(0) == 65
  let missing = raw.byte_at(9) == null
  let size = text_length(raw)
  {equal, missing, size}
}
assert bytes_probe(b"A") == {equal: true, missing: true, size: 1}
assert bytes_probe(b"B") == {equal: false, missing: true, size: 1}
assert bytes_probe(b"") == {equal: false, missing: true, size: 0}
assert text_probe("A") == {equal: true, missing: true, size: 1}
assert text_probe("B") == {equal: false, missing: true, size: 1}
assert text_probe("") == {equal: false, missing: true, size: 0}
""",
  )?
  assert output.success, output.stderr
}
