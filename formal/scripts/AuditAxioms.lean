private def isAllowedAxiom (name : Lean.Name) : Bool :=
  name == `propext || name == `Quot.sound || name == `Classical.choice

private def isProjectDeclaration
    (env : Lean.Environment) (projectModules : Array Lean.Name) (name : Lean.Name) : Bool :=
  match env.getModuleIdxFor? name with
  | some index =>
      let moduleName := env.header.moduleNames[index.toNat]!
      projectModules.contains moduleName || moduleName == env.mainModule
  | none => true

open Lean

run_cmd do
  let env ← getEnv
  for moduleName in xshAuditProjectModules do
    unless env.header.moduleNames.contains moduleName do
      throwError m!"project module was not imported: {moduleName}"

  let mut checkedDeclarations := 0
  for (name, info) in env.constants.toList do
    if isProjectDeclaration env xshAuditProjectModules name then
      checkedDeclarations := checkedDeclarations + 1
      if info.isUnsafe then
        throwError m!"unsafe project declaration rejected: {name}"
      if info matches .axiomInfo _ then
        throwError m!"project axiom declaration rejected: {name}"
      let axioms ← collectAxioms name
      let forbidden := axioms.filter (fun axiomName => !isAllowedAxiom axiomName)
      if !forbidden.isEmpty then
        throwError m!"{name} depends on forbidden axioms {forbidden}"
  if checkedDeclarations == 0 then
    throwError "no project declarations found"
  logInfo m!"Axiom allowlist passed for {checkedDeclarations} project declaration(s)"
