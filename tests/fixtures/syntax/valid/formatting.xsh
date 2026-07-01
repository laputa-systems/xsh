# formatter fixture
use fs

proc main(args: List[Str]) {
  # nested comment
  let config = {name: "demo", enabled: true}
  let values = ["one", "two"]

  if true {
    run echo "hello "${args[0]} ?
  } else {
    eprint "no"
  }
}

main(args)?
