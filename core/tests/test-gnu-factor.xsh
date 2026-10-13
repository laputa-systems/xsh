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

# These contiguous intervals cover 1 through 1,000,000 exactly once.
# origin: gnu factor/factor-parallel.log
test test_gnu_factor_factor_parallel_log_1_to_62500 { |ctx|
  assert factor_parallel_count(ctx, 1, 62500)? == "6275"
}

test test_gnu_factor_factor_parallel_log_62501_to_125000 { |ctx|
  assert factor_parallel_count(ctx, 62501, 125000)? == "5459"
}

test test_gnu_factor_factor_parallel_log_125001_to_187500 { |ctx|
  assert factor_parallel_count(ctx, 125001, 187500)? == "5230"
}

test test_gnu_factor_factor_parallel_log_187501_to_250000 { |ctx|
  assert factor_parallel_count(ctx, 187501, 250000)? == "5080"
}

test test_gnu_factor_factor_parallel_log_250001_to_312500 { |ctx|
  assert factor_parallel_count(ctx, 250001, 312500)? == "4948"
}

test test_gnu_factor_factor_parallel_log_312501_to_375000 { |ctx|
  assert factor_parallel_count(ctx, 312501, 375000)? == "4912"
}

test test_gnu_factor_factor_parallel_log_375001_to_437500 { |ctx|
  assert factor_parallel_count(ctx, 375001, 437500)? == "4852"
}

test test_gnu_factor_factor_parallel_log_437501_to_500000 { |ctx|
  assert factor_parallel_count(ctx, 437501, 500000)? == "4782"
}

test test_gnu_factor_factor_parallel_log_500001_to_562500 { |ctx|
  assert factor_parallel_count(ctx, 500001, 562500)? == "4719"
}

test test_gnu_factor_factor_parallel_log_562501_to_625000 { |ctx|
  assert factor_parallel_count(ctx, 562501, 625000)? == "4729"
}

test test_gnu_factor_factor_parallel_log_625001_to_687500 { |ctx|
  assert factor_parallel_count(ctx, 625001, 687500)? == "4640"
}

test test_gnu_factor_factor_parallel_log_687501_to_750000 { |ctx|
  assert factor_parallel_count(ctx, 687501, 750000)? == "4612"
}

test test_gnu_factor_factor_parallel_log_750001_to_812500 { |ctx|
  assert factor_parallel_count(ctx, 750001, 812500)? == "4635"
}

test test_gnu_factor_factor_parallel_log_812501_to_875000 { |ctx|
  assert factor_parallel_count(ctx, 812501, 875000)? == "4575"
}

test test_gnu_factor_factor_parallel_log_875001_to_937500 { |ctx|
  assert factor_parallel_count(ctx, 875001, 937500)? == "4558"
}

test test_gnu_factor_factor_parallel_log_937501_to_1000000 { |ctx|
  assert factor_parallel_count(ctx, 937501, 1000000)? == "4492"
}
