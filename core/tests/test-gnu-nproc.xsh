use support.uu

type OverrideCase = {vars: Record, expected: Str}

# origin: gnu nproc/nproc-avail.log
test test_gnu_nproc_nproc_avail_log { |ctx|
  let s = uu.scene(ctx)?
  let all = uu.invoke(s, "nproc", ["--all"])?
  let available = uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: ""})?
  assert available.stdout.utf8()?.trim().parse_int()? <= all.stdout.utf8()?.trim().parse_int()?
}

# origin: gnu nproc/nproc-override.log
test test_gnu_nproc_nproc_override_log { |ctx|
  let s = uu.scene(ctx)?
  let baseline = uu.invoke(s, "nproc", [])?
  uu.succeeds(baseline)
  let available = baseline.stdout.utf8()?.trim().parse_int()?
  assert available > 0
  let rows: List[OverrideCase] = [
    {vars: {}, expected: f"{available}"},
    {vars: {OMP_THREAD_LIMIT: "1"}, expected: "1"},
    {vars: {OMP_THREAD_LIMIT: "1", OMP_NUM_THREADS: "0"}, expected: "1"},
    {vars: {OMP_NUM_THREADS: "0"}, expected: f"{available}"},
    {vars: {OMP_NUM_THREADS: "2,2,1"}, expected: "2"},
    {vars: {OMP_NUM_THREADS: "2,ignored"}, expected: "2"},
    {vars: {OMP_NUM_THREADS: "2bad"}, expected: f"{available}"},
    {vars: {OMP_NUM_THREADS: "-2"}, expected: f"{available}"},
    {vars: {OMP_THREAD_LIMIT: "1", OMP_NUM_THREADS: "2,2,1"}, expected: "1"},
    {vars: {OMP_THREAD_LIMIT: "0", OMP_NUM_THREADS: "2,2,1"}, expected: "2"},
    {vars: {OMP_THREAD_LIMIT: "1bad", OMP_NUM_THREADS: "2,2,1"}, expected: "2"},
    {vars: {OMP_NUM_THREADS: "18446744073709551616"}, expected: "18446744073709551615"},
    {vars: {OMP_THREAD_LIMIT: "1bad", OMP_NUM_THREADS: f"{available + 1},2,1"}, expected: f"{available + 1}"},
    {vars: {OMP_THREAD_LIMIT: "1", OMP_NUM_THREADS: f"{available + 1}"}, expected: "1"},
    {vars: {OMP_THREAD_LIMIT: f"{available + 2}", OMP_NUM_THREADS: f"{available + 1}"}, expected: f"{available + 1}"},
    {vars: {OMP_THREAD_LIMIT: f"{available + 1}", OMP_NUM_THREADS: f"{available + 2}"}, expected: f"{available + 1}"},
    {vars: {OMP_NUM_THREADS: f"{available + 1}"}, expected: f"{available + 1}"},
  ]
  var failures: List[Str] = []
  for index in range(rows.len()) {
    let item = rows[index]
    let r = uu.invoke(s, "nproc", [], vars: item.vars)?
    if r.stdout.utf8()?.trim() != item.expected { failures += [f"override row {index}: {r.stdout.utf8()?} expected {item.expected}"] }
  }
  assert failures.is_empty(), failures.join("\n")
}

# origin: gnu nproc/nproc-positive.log
test test_gnu_nproc_nproc_positive_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--all"], []] {
    let r = uu.invoke(s, "nproc", args)?
    assert r.stdout.utf8()?.trim().parse_int()? > 0
  }
  for value in ["-1000", "0", "1", "1000"] {
    let r = uu.invoke(s, "nproc", [], vars: {OMP_NUM_THREADS: value})?
    assert r.stdout.utf8()?.trim().parse_int()? > 0
  }
  for value in ["0", " 1", "1000"] {
    let r = uu.invoke(s, "nproc", ["--ignore=" + value])?
    assert r.stdout.utf8()?.trim().parse_int()? > 0
  }
  for value in ["-1", "N"] { uu.fails_with_code(uu.invoke(s, "nproc", ["--ignore=" + value])?, 1) }
  let subtract = uu.invoke(s, "nproc", ["--ignore=40"], vars: {OMP_NUM_THREADS: "42"})?
  assert subtract.stdout.utf8()?.trim().parse_int()? == 2
  let overflow = uu.invoke(s, "nproc", ["--ignore", "18446744073709551616"], vars: {OMP_NUM_THREADS: "42"})?
  assert overflow.stdout.utf8()?.trim() == "1"
}
