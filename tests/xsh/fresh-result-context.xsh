test fresh_result_context { |ctx|
  let source = p"tests/fixtures/runtime/fresh-result-context.xsh".read_text()?
  test.expect(ctx, source + "\nfresh_result_context()?\n", status: 0)?
}

test unresolved_propagated_result_context_reports_a_receiver_annotation { |ctx|
  test.expect(ctx, r"""pure fail() -> Result[Unit] {
  Err(error.failure("failed"))?.context("stage")
}
""", status: 2, stderr: ["[check.local-inference]", "method `context` receiver needs an annotation"])?
}
