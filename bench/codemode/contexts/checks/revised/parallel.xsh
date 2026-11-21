tempdir dir {
  let log = fp"{dir}/log"
  let script = fp"{dir}/step.sh"
  log.write("")
  script.write("printf 'start\\n' >> \"$1\"\nsleep 0.3\nprintf 'end\\n' >> \"$1\"\n")
  let done = [1, 2, 3, 4, 5, 6] |> par-map(jobs: 3) { |n|
    run sh $script $log
    n
  }

  var running = 0
  var peak = 0
  for line in log.read_text()?.lines() {
    running += if line == "start" { 1 } else { -1 }
    peak = if running > peak { running } else { peak }
  }

  print f"{json.encode(done)?} peak={peak}"
}
