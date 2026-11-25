pure uneven(n: Int) -> Int {
  var total = 0
  for i in range(n * 10000) { total += i }
  total
}

proc fail_later(root: Path) [fs, time, error] -> Result[List[Int]] {
  [20, 1, 2, 3] |> par-map(jobs: 2) { |n|
    if n == 20 {
      fp"{root}/started".touch()
      time.sleep(2s)
    }
    if n == 1 {
      wait until fp"{root}/started".exists() within 5s every 5ms
      fail "later item"
    }
    uneven(n)
  } |> collect()
}

test test_par_map_claim_order {
  let values = [20, 1, 3, 2] |> par-map(jobs: 2) { |n| uneven(n) } |> collect()
  assert values == [uneven(20), uneven(1), uneven(3), uneven(2)]
}

test test_par_map_claim_failure { |ctx|
  let root = test.temp_dir(ctx, name: "par-map-claim")?
  match fail_later(root) {
    Err(failure) => assert "later item" in failure.message, failure.message
    Ok(_) => assert false, "expected failure"
  }
}
