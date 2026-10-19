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
