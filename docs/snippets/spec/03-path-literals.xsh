proc build() [process, error] {
  # begin example
  let cc = /usr/bin/cc
  let out = ./target/build
  # end example
  run $cc -o fp"{out}/main" main.c
}
