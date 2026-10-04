use fuzz_paths as fuzz

test test_fuzz_binary_uses_cargo_artifact_path {
  let messages = r"""{"reason":"compiler-message","message":{"rendered":"warning: example"}}
{"reason":"build-script-executed","package_id":"helper"}
{"reason":"compiler-artifact","target":{"name":"xsh_fuzz","kind":["lib"]},"executable":null}
{"reason":"compiler-artifact","target":{"name":"other","kind":["bin"]},"executable":"/tmp/other"}
{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["lib"]},"executable":null}
{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["bin"]},"executable":null}
{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["bin"]},"executable":"/tmp/custom target/aarch64-apple-darwin/release/xsh-fuzz","fresh":true}
{"reason":"build-finished","success":true}
"""
  assert fuzz.built_binary(messages)? == p"/tmp/custom target/aarch64-apple-darwin/release/xsh-fuzz"
}

test test_fuzz_binary_rejects_missing_or_ambiguous_artifacts {
  let artifact = r"""{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["bin"]},"executable":"/tmp/xsh-fuzz"}
"""
  for messages in ["", r"""{"reason":"build-finished","success":true}""", artifact + artifact] {
    match fuzz.built_binary(messages) {
      Ok(_) => test.fail("cargo output without one binary artifact was accepted")?
      Err(failure) => assert "expected one xsh-fuzz binary artifact" in failure.message, failure.message
    }
  }
}

test test_fuzz_binary_rejects_invalid_build_messages {
  for messages in [
    "not JSON",
    r"""{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["bin"]},"executable":17}""",
    r"""{"reason":"compiler-artifact","target":{"name":"xsh-fuzz","kind":["bin"]}}""",
  ] {
    match fuzz.built_binary(messages) {
      Ok(_) => test.fail("invalid cargo build output was accepted")?
      Err(_) => {}
    }
  }
}

test test_fuzz_failures_follow_binary_contents { |ctx|
  let root = test.temp_dir(ctx, name: "fuzz-address")?
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  first.write("same XSH implementation")?
  second.write("same XSH implementation")?
  let original = fuzz.failure_dir(first)?
  assert original == fuzz.failure_dir(second)?
  assert original == fp"target/fuzz/{hash.sha256(first)?.hex()}/failures"
  second.write("changed XSH implementation")?
  assert original != fuzz.failure_dir(second)?
  assert original == fuzz.failure_dir(first)?
}

test test_fuzz_failure_address_rejects_missing_binary { |ctx|
  let root = test.temp_dir(ctx, name: "fuzz-missing")?
  match fuzz.failure_dir(fp"{root}/missing") {
    Ok(_) => test.fail("missing binary was assigned a failure directory")?
    Err(_) => {}
  }
}
