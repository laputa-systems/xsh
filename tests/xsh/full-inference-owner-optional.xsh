test test_optional_api_argument_uses_fallback_evidence { |ctx|
  let root = test.temp_dir(ctx, name: "optional-owner")?
  let file = fp"{root}/file"
  file.write("contents")?
  for declaration in [
    r"""type Options = {owner: Int?}
proc change(opts: Options, fallback: Int, file: Path) -> Result[Unit] {
  let uid = opts.owner ?? fallback
  fs.set_owner(file, uid: uid)?
}""",
    r"""type Options = {owner: Int?}
proc change(opts, fallback: Int, file: Path) -> Result[Unit] {
  let uid = opts.owner ?? fallback
  fs.set_owner(file, uid: uid)?
}""",
  ] {
    let result = test.expect(ctx, declaration + r"""
proc main(...argv: List[Str]) [fs, error] {
  let file = Path(argv[0])
  let owner = fs.stat(file)?.uid
  let absent: Options = {owner: null}
  let present: Options = {owner: owner}
  change(absent, owner, file)?
  change(present, owner, file)?
  assert fs.stat(file)?.uid == owner
}
""", status: 0, args: [f"{file}"], env: {})?
    assert result.stderr == "", result.stderr
  }
}
