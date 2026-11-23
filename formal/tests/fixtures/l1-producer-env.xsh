stream values() [env, error] -> Stream[Int] {
  e"XSH_PA" = "before-scope"
  env ({XSH_PB: "scoped"}) {
    print ${env.get_or("XSH_PA", "unset")?} ${env.get_or("XSH_PB", "unset")?}
    yield 1
  }
  print ${env.get_or("XSH_PA", "unset")?} ${env.get_or("XSH_PB", "unset")?} ${env.get_or("XSH_PC", "unset")?}
  yield 2
  print ${env.get_or("XSH_PA", "unset")?} ${env.get_or("XSH_PB", "unset")?} ${env.get_or("XSH_PC", "unset")?}
  yield 3
}
for item in values() {
  print "consumer" $item ${env.get_or("XSH_PA", "unset")?}
  e"XSH_PC" = "loop"
}
