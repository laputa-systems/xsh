var index = 0
while index < 128 {
  run /usr/bin/printf "probe %s\n" $index
  index += 1
}
