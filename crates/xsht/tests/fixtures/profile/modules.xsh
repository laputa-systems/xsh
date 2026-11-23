type Plugin = module { export pure answer() -> Int }
proc load(file: Path) [fs, error] -> Result[Int] {
  let plugin = module.load(file)?.require(Plugin)?
  plugin.answer()
}
for file in [p"one.xsh", p"two.xsh", p"three.xsh"] { print (load(file)?) }
