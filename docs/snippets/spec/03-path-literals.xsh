proc build() [process, error] {
  # begin example
  let cc = p"/usr/bin/cc"
  let out = p"./target/build"
  # end example
  run $cc -o fp"{out}/main" main.c
}
