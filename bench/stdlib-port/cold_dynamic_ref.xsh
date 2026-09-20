proc main() [io, error] {
  let loaded = module.load(p"dynmod/mod.xsh")?.require(Mod)?
  print "${loaded.answer()}"
}

type Mod = module {
  export pure answer() -> Int
}
