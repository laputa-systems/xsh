test test_rg_reports_matches_with_line_numbers { |ctx|
  let file = test.temp_file(ctx, name: "notes.txt", contents: b"alpha\nbeta\nalphabet\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -n alpha $file ?
  assert "1:alpha" in output
  assert "3:alphabet" in output
}

test test_rg_count_and_filename { |ctx|
  let left = test.temp_file(ctx, name: "left.txt", contents: b"needle\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"needle\n")?
  let no_filename = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -h needle $left $right ?

  assert no_filename == """needle
needle
"""

  let count = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -c needle $left ?
  assert count.trim() == "1"
  let named_counts = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -c needle $left $right ?
  assert f"${left}:1" in named_counts
  assert f"${right}:1" in named_counts
}

test test_rg_word_line_pattern_and_globs { |ctx|
  let root = test.temp_dir(ctx, name: "rg-more")?
  let keep = fp"${root}/keep.txt"
  let drop = fp"${root}/drop.log"

  keep.write("""alpha
alphabet
needle
Needle
""")?

  drop.write("""alpha
""")?

  let word = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -w alpha $keep ?

  assert word == """alpha
"""

  let line = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -x needle $keep ?

  assert line == """needle
"""

  let fixed_case = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -F -i needle $keep ?
  assert "needle" in fixed_case
  assert "Needle" in fixed_case
  let globbed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -H -g "*.txt" -g "!*.log" alpha $root ?
  assert "keep.txt" in globbed
  assert ! ("drop.log" in globbed)
  let compact = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -eneedle $keep ?

  assert compact == """needle
"""
}
