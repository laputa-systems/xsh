proc meet(dir: Path) [fs, time, error] -> Result[List[Int]] {
  [0, 1] |> par-map(jobs: 2) { |i|
    fp"{dir}/{i}".touch()?
    wait until fp"{dir}/{1 - i}".exists()? within 5s every 5ms
    i
  } |> collect()
}
tempdir dir {
  let met = meet(dir)?
  print (f"met {met[0]} {met[1]}")
}?
