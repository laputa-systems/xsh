##! Failure storage for fuzz campaigns, addressed by the implementation under test.

## Selects the executable Cargo actually built, including custom target directories.
export proc built_binary(messages: Str) [error] -> Result[Path, Error] {
  let binaries = collect {
    for line in messages.lines() {
      let message = json.decode(line)?
      let reason = json.get(message, ["reason"])?.require(Str)?
      continue when reason != "compiler-artifact"
      let name = json.get(message, ["target", "name"])?.require(Str)?
      let kinds = json.get(message, ["target", "kind"])?.require(List[Str])?
      continue when name != "xsh-fuzz" or "bin" not in kinds
      let executable = json.get(message, ["executable"])?.require(Str?)?
      yield fp"{executable}" when executable != null
    }
  }

  if binaries.len() != 1 {
    error.fail(f"expected one xsh-fuzz binary artifact, found {binaries.len()}")
  }

  Ok(binaries[0])
}

## Keeps reproducers from different tested binaries in separate directories.
export proc failure_dir(binary: Path) [fs, error] -> Result[Path, Error] {
  let digest = hash.sha256(binary)?.hex()
  Ok(fp"target/fuzz/{digest}/failures")
}
