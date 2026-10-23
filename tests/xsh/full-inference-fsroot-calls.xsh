test test_full_inference_fsroot_calls_keep_selected_native_contracts { |ctx|
  let output = test.run_xsh(
    ctx,
    p"tests/fixtures/runtime/generic-fsroot-calls.xsh".read_text()?,
  )?
  assert output.success, output.stderr
  assert output.stdout == "rooted calls preserved\n"
}

test test_full_inference_fsroot_calls_reject_invalid_receiver_and_effects { |ctx|
  for source in [
    "proc listed(marker, root: Str) [fs, error] { let _ = marker; root.children(p\".\")? }",
    "proc listed(marker, root: FsRoot) [] { let _ = marker; root.children(p\".\")? }",
    "pure listed(marker, root: FsRoot) { let _ = marker; root.children(p\".\") }",
    "proc listed(marker, root: FsRoot) [fs, error] { let _ = marker; root.children(p\".\", max_entries: true)? }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "err[check." in output.stderr
  }
}
