type Plugin = module { export pure answer() -> Int }
test dynamic {
  let plugin = module.load(p"plugin.xsh")?.require(Plugin)?
  assert plugin.answer() == 42
}
