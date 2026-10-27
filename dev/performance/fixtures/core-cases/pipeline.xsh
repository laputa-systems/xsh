cli main(count: Int) {
  var index = 0
  while index < count {
    run /usr/bin/printf "probe %s\n" $index | run cat
    index += 1
  }
}
