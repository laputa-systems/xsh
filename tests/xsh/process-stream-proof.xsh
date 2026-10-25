test test_default_process_stream_retains_its_initializer_proof { |ctx|
  let output = test.expect(ctx, r"""let rows = run.stream --text sh -c "printf row; exit 0" ?
for row in rows { print $row }
""", status: 0)?
  assert output.stdout == "row\n"
}

test test_default_process_stream_reports_completion_after_its_rows { |ctx|
  let output = test.expect(ctx, r"""let rows = run.stream --text sh -c "printf row; exit 7" ?
for row in rows { print $row }
""", status: 3)?
  assert output.stdout == "row\n"
  assert "err: `sh` exited 7" in output.stderr
}
