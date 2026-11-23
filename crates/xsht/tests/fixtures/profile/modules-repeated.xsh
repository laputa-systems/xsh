type Plugin = module { export pure answer() -> Int }
proc load(file: Path) [fs, error] -> Result[Int] {
  let plugin = module.load(file)?.require(Plugin)?
  plugin.answer()
}
for i in range(3) { print (load(p"one.xsh")?) }
