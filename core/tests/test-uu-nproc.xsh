##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_nproc.rs.

use support.uu as uu

# The upstream count assertions parse an unsigned byte after trimming whitespace.
proc count(r: uu.Ran) [error] -> Result[Int, Error] {
  uu.succeeds(r)
  let value = r.stdout.utf8()?.trim().parse_int()?
  assert value >= 0 and value <= 255, f"nproc count outside u8 range: {value}"
  Ok(value)
}

# origin: uutils test_nproc::test_invalid_arg
test test_uu_nproc_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "nproc", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_nproc::test_nproc
test test_uu_nproc_nproc { |ctx|
  let s = uu.scene(ctx)?
  assert count(uu.invoke(s, "nproc", [])?)? > 0
}

# origin: uutils test_nproc::test_nproc_all_omp
test test_uu_nproc_nproc_all_omp { |ctx|
  let s = uu.scene(ctx)?
  let total = count(uu.invoke(s, "nproc", ["--all"])?)?
  assert total > 0
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "60"})?)? == 60
  assert count(uu.invoke(s, "nproc", ["--all"], vars: {OMP_NUM_THREADS: "1"})?)? == total
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "incorrectnumber"})?)? == total
  let overflow = uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "99999999999999999999"})?
  uu.succeeds(overflow)
  uu.stdout_only(overflow, "18446744073709551615\n")
}

# origin: uutils test_nproc::test_nproc_ignore
test test_uu_nproc_nproc_ignore { |ctx|
  let s = uu.scene(ctx)?
  let total = count(uu.invoke(s, "nproc", [])?)?
  if total > 1 {
    assert count(uu.invoke(s, "nproc", ["--ignore", f"{total - 1}"])?)? == 1
    assert count(uu.invoke(s, "nproc", ["--ignore= 1"])?)? == total - 1
    let overflow = uu.invoke(s, "nproc", ["--ignore=99999999999999999999"])?
    uu.succeeds(overflow)
    uu.stdout_only(overflow, "1\n")
  }
}

# origin: uutils test_nproc::test_nproc_ignore_all_omp
test test_uu_nproc_nproc_ignore_all_omp { |ctx|
  let s = uu.scene(ctx)?
  assert count(uu.invoke(s, "nproc", ["--ignore=40"], vars: {OMP_NUM_THREADS: "42"})?)? == 2
}

# origin: uutils test_nproc::test_nproc_omp_limit
test test_uu_nproc_nproc_omp_limit { |ctx|
  let s = uu.scene(ctx)?
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "42", OMP_THREAD_LIMIT: "0"})?)? == 42
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "42", OMP_THREAD_LIMIT: "2"})?)? == 2
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "42", OMP_THREAD_LIMIT: "2bad"})?)? == 42
  let total = count(uu.invoke(s, "nproc", ["--all"])?)?
  assert total > 0
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_THREAD_LIMIT: "1"})?)? == 1
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "0", OMP_THREAD_LIMIT: ""})?)? == total
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "", OMP_THREAD_LIMIT: ""})?)? == total
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "2,2,1", OMP_THREAD_LIMIT: ""})?)? == 2
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "2,ignored", OMP_THREAD_LIMIT: ""})?)? == 2
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "2,2,1", OMP_THREAD_LIMIT: "0"})?)? == 2
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "2,2,1", OMP_THREAD_LIMIT: "1bad"})?)? == 2
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: "29,2,1", OMP_THREAD_LIMIT: "1bad"})?)? == 29
}

# origin: uutils test_nproc::test_nproc_omp_num_threads_with_whitespace
test test_uu_nproc_nproc_omp_num_threads_with_whitespace { |ctx|
  let s = uu.scene(ctx)?
  assert count(uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: " 42 "})?)? == 42
}
