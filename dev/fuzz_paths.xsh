##! Failure storage for fuzz campaigns, addressed by the implementation under test.

## Selects the executable Cargo actually built, including custom target directories.
export proc built_binary(messages: Str) [error] -> Result[Path] {
  var binaries: List[Path] = []
  for line in messages.lines() {
    let message = json.decode(line)?
    let reason = json.get(message, ["reason"])?.require(Str)?
    if reason != "compiler-artifact" { continue }
    let name = json.get(message, ["target", "name"])?.require(Str)?
    let kinds = json.get(message, ["target", "kind"])?.require(List[Str])?
    if name != "xsh-fuzz" or "bin" not in kinds { continue }
    let executable = json.get(message, ["executable"])?.require(Str?)?
    if executable != null { binaries += [Path(executable)] }
  }
  if binaries.len() != 1 {
    error.fail(f"expected one xsh-fuzz binary artifact, found {binaries.len()}")?
  }
  Ok(binaries[0])
}

## Keeps reproducers from different tested binaries in separate directories.
export proc failure_dir(binary: Path) [fs, error] -> Result[Path] {
  let digest = hash.sha256(binary)?.hex()
  Ok(fp"target/fuzz/{digest}/failures")
}
