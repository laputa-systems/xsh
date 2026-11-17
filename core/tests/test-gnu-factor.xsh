use support.uu

pure shell_words(words: List[Path]) -> Str {
  ["'" + word.display().replace("'", with: "'\\''") + "'" for word in words].join(" ")
}

proc factor_parallel_count(ctx: TestContext, first: Int, last: Int) [fs, process, error] -> Result[Str] {
  test.timeout(ctx, 120s)
  let s = uu.scene(ctx)?
  let sequence = uu.argv(s, "seq", [fp"{first}", fp"{last}"])?.require(NonEmpty[Path])?
  let odd = uu.argv(s, "sed", [p"/[02468]$/d"])?.require(NonEmpty[Path])?
  let factor = shell_words(uu.argv(s, "factor", [])?)
  let split_argv = uu.argv(s, "split", [p"-nr/4", fp"--filter={factor}"])?
  let split = [p"/bin/sh", p"-c", p"cd \"$1\" || exit; shift; exec \"$@\"; ", p"factor-split", s.root].extend(split_argv).require(NonEmpty[Path])?
  let primes = uu.argv(s, "sed", [p"-e", p"s/^.*: //", p"-e", p"/ /d"])?.require(NonEmpty[Path])?
  let count = uu.argv(s, "wc", [p"-l"])?.require(NonEmpty[Path])?
  let result = run.text --timeout=120s @sequence | run @odd | run @split | run @primes | run @count
  Ok(result.trim())
}

# origin: gnu factor/factor-parallel.log
test test_gnu_factor_factor_parallel_log_1_to_31250 { |ctx|
  assert factor_parallel_count(ctx, 1, 31250)? == "3368"
}


test test_gnu_factor_targeted_exact_factorizations { |ctx|
  let s = uu.scene(ctx)?
  let expected = fp"{ctx.core_dir}/tests/data/factor/targeted.expected".read_text()?
  let input = bytes.from_text([line.split(":")[0] for line in expected.lines()].join("\n") + "\n")
  let result = uu.invoke(s, "factor", [], stdin: input)?
  uu.succeeds(result)
  uu.stdout_only(result, expected)
}
