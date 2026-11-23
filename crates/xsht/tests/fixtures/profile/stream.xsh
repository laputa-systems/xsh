proc consumer() [process, error] -> Result[Int] {
  var total = 0
  for line in run.stream --text sh -c "printf 'a\\nb\\nc\\n'" {
    for i in range(30000) { total += i }
    total += line.byte_len()
  }
  total
}
print (consumer()?)
