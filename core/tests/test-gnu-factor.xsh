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
test test_gnu_factor_factor_parallel_log_1_to_31250 { |ctx|
  assert factor_parallel_count(ctx, 1, 31250)? == "3368"
}

test test_gnu_factor_factor_parallel_log_31251_to_62500 { |ctx|
  assert factor_parallel_count(ctx, 31251, 62500)? == "2907"
}

test test_gnu_factor_factor_parallel_log_62501_to_93750 { |ctx|
  assert factor_parallel_count(ctx, 62501, 93750)? == "2772"
}

test test_gnu_factor_factor_parallel_log_93751_to_125000 { |ctx|
  assert factor_parallel_count(ctx, 93751, 125000)? == "2687"
}

test test_gnu_factor_factor_parallel_log_125001_to_156250 { |ctx|
  assert factor_parallel_count(ctx, 125001, 156250)? == "2642"
}

test test_gnu_factor_factor_parallel_log_156251_to_187500 { |ctx|
  assert factor_parallel_count(ctx, 156251, 187500)? == "2588"
}

test test_gnu_factor_factor_parallel_log_187501_to_218750 { |ctx|
  assert factor_parallel_count(ctx, 187501, 218750)? == "2542"
}

test test_gnu_factor_factor_parallel_log_218751_to_250000 { |ctx|
  assert factor_parallel_count(ctx, 218751, 250000)? == "2538"
}

test test_gnu_factor_factor_parallel_log_250001_to_281250 { |ctx|
  assert factor_parallel_count(ctx, 250001, 281250)? == "2493"
}

test test_gnu_factor_factor_parallel_log_281251_to_312500 { |ctx|
  assert factor_parallel_count(ctx, 281251, 312500)? == "2455"
}

test test_gnu_factor_factor_parallel_log_312501_to_343750 { |ctx|
  assert factor_parallel_count(ctx, 312501, 343750)? == "2488"
}

test test_gnu_factor_factor_parallel_log_343751_to_375000 { |ctx|
  assert factor_parallel_count(ctx, 343751, 375000)? == "2424"
}

test test_gnu_factor_factor_parallel_log_375001_to_406250 { |ctx|
  assert factor_parallel_count(ctx, 375001, 406250)? == "2415"
}

test test_gnu_factor_factor_parallel_log_406251_to_437500 { |ctx|
  assert factor_parallel_count(ctx, 406251, 437500)? == "2437"
}

test test_gnu_factor_factor_parallel_log_437501_to_468750 { |ctx|
  assert factor_parallel_count(ctx, 437501, 468750)? == "2375"
}

test test_gnu_factor_factor_parallel_log_468751_to_500000 { |ctx|
  assert factor_parallel_count(ctx, 468751, 500000)? == "2407"
}

test test_gnu_factor_factor_parallel_log_500001_to_531250 { |ctx|
  assert factor_parallel_count(ctx, 500001, 531250)? == "2384"
}

test test_gnu_factor_factor_parallel_log_531251_to_562500 { |ctx|
  assert factor_parallel_count(ctx, 531251, 562500)? == "2335"
}

test test_gnu_factor_factor_parallel_log_562501_to_593750 { |ctx|
  assert factor_parallel_count(ctx, 562501, 593750)? == "2374"
}

test test_gnu_factor_factor_parallel_log_593751_to_625000 { |ctx|
  assert factor_parallel_count(ctx, 593751, 625000)? == "2355"
}

test test_gnu_factor_factor_parallel_log_625001_to_656250 { |ctx|
  assert factor_parallel_count(ctx, 625001, 656250)? == "2298"
}

test test_gnu_factor_factor_parallel_log_656251_to_687500 { |ctx|
  assert factor_parallel_count(ctx, 656251, 687500)? == "2342"
}

test test_gnu_factor_factor_parallel_log_687501_to_718750 { |ctx|
  assert factor_parallel_count(ctx, 687501, 718750)? == "2314"
}

test test_gnu_factor_factor_parallel_log_718751_to_750000 { |ctx|
  assert factor_parallel_count(ctx, 718751, 750000)? == "2298"
}

test test_gnu_factor_factor_parallel_log_750001_to_781250 { |ctx|
  assert factor_parallel_count(ctx, 750001, 781250)? == "2321"
}

test test_gnu_factor_factor_parallel_log_781251_to_812500 { |ctx|
  assert factor_parallel_count(ctx, 781251, 812500)? == "2314"
}

test test_gnu_factor_factor_parallel_log_812501_to_843750 { |ctx|
  assert factor_parallel_count(ctx, 812501, 843750)? == "2291"
}

test test_gnu_factor_factor_parallel_log_843751_to_875000 { |ctx|
  assert factor_parallel_count(ctx, 843751, 875000)? == "2284"
}

test test_gnu_factor_factor_parallel_log_875001_to_906250 { |ctx|
  assert factor_parallel_count(ctx, 875001, 906250)? == "2275"
}

test test_gnu_factor_factor_parallel_log_906251_to_937500 { |ctx|
  assert factor_parallel_count(ctx, 906251, 937500)? == "2283"
}

test test_gnu_factor_factor_parallel_log_937501_to_968750 { |ctx|
  assert factor_parallel_count(ctx, 937501, 968750)? == "2255"
}

test test_gnu_factor_factor_parallel_log_968751_to_1000000 { |ctx|
  assert factor_parallel_count(ctx, 968751, 1000000)? == "2237"
}
