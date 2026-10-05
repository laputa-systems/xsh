const debug = true
const workspace = p"."

proc debug_build(root: Path) [fs, error] {
  fp"{root}/debug".mkdir()
}

proc release_build(root: Path) [fs, process, error] {
  fp"{root}/release".mkdir()
}

# begin example
type Builder = proc(root: Path) [fs, process, error] -> Result[Unit]

type Step = {name: Str, build: Builder}

proc run_step(step: Step, root: Path) [fs, process, error] {
  print f"building {step.name}"
  step.build(root)
}

let build: Builder = if debug { debug_build } else { release_build }
build(workspace)
run_step(Step(name: "release", build: release_build), workspace)
# end example
