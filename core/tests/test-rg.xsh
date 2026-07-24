proc test_rg_reports_matches_with_line_numbers(ctx: TestContext) [fs, process, env, error] {
  let file = test.temp_file(ctx, name: "notes.txt", contents: b"alpha\nbeta\nalphabet\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -n alpha $file ?
  test.contains(output, "1:alpha")?
  test.contains(output, "3:alphabet")?
}

proc test_rg_count_and_filename(ctx: TestContext) [fs, process, env, error] {
  let left = test.temp_file(ctx, name: "left.txt", contents: b"needle\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"needle\n")?
  let no_filename = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -h needle $left $right ?

  test.eq(
    no_filename,
    """needle
needle
""",
  )?

  let count = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -c needle $left ?
  test.eq(count.trim(), "1")?
  let named_counts = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -c needle $left $right ?
  test.contains(named_counts, f"${left}:1")?
  test.contains(named_counts, f"${right}:1")?
}

proc test_rg_word_line_pattern_and_globs(ctx: TestContext) [fs, process, env, error] {
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

  test.eq(
    word,
    """alpha
""",
  )?

  let line = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -x needle $keep ?

  test.eq(
    line,
    """needle
""",
  )?

  let fixed_case = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -F -i needle $keep ?
  test.contains(fixed_case, "needle")?
  test.contains(fixed_case, "Needle")?
  let globbed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -H -g "*.txt" -g "!*.log" alpha $root ?
  test.contains(globbed, "keep.txt")?
  test.ok(! ("drop.log" in globbed))?
  let compact = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rg.xsh" -- -eneedle $keep ?

  test.eq(
    compact,
    """needle
""",
  )?
}
