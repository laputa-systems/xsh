use support.uu as uu

pure shell_words(words: List[Path]) -> Str {
  ["'" + word.display().replace("'", with: "'\\''") + "'" for word in words].join(" ")
}

# origin: gnu factor/factor-parallel.log
test test_gnu_factor_factor_parallel_log { |ctx|
  test.timeout(ctx, 600s)
  let s = uu.scene(ctx)?
  let sequence = uu.argv(s, "seq", [p"1e6"])?
  let odd = uu.argv(s, "sed", [p"/[02468]$/d"])?
  let factor = shell_words(uu.argv(s, "factor", [])?)
  let split_argv = uu.argv(s, "split", [p"-nr/4", Path("--filter=" + factor)])?
  let split = [p"/bin/sh", p"-c", Path(r"""cd "$1" || exit; shift; exec "$@"; """), p"factor-split", s.root].extend(split_argv)
  let primes = uu.argv(s, "sed", [p"-e", p"s/^.*: //", p"-e", p"/ /d"])?
  let count = uu.argv(s, "wc", [p"-l"])?
  let result = run.text --timeout=600s @sequence | run @odd | run @split | run @primes | run @count
  assert result.trim() == "78498"
}
