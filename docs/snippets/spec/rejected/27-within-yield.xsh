# begin example
stream ticks() [time] -> Stream[Int] {
  within 5s {
    yield 1 # error: check.yield
  }
}
# end example
