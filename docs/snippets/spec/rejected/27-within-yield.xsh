# begin example
stream ticks() [time, error] -> Stream[Int] {
  within 5s {
    yield 1 # error: check.yield
  }
}
# end example
