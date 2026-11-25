proc sleeper() [time, error] -> Result[Int] {
  time.sleep(200ms)
  1
}
print (sleeper()?)
