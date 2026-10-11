type DecoderCommandManifest = {name: Str, version: Str}

test test_indexed_proc_json_write_dispatches_and_short_circuits_argument_results { |ctx|
  let root = test.temp_dir(ctx, name: "decoder-command")?
  let manifest = fp"{root}/manifest.json"
  let write_source = r"""let manifest = p""" + json.encode(manifest.display())? + "\n" + r"""
let name = "demo"
let version = "1.0"
json.write manifest ({name, version}) ?
"""
  let output = test.expect(ctx, write_source, status: 0)?
  assert output.stdout == ""
  let written = json.read(manifest)?.require(DecoderCommandManifest)?
  assert written.name == "demo"
  assert written.version == "1.0"

  let skipped = fp"{root}/skipped.json"
  let skipped_source = r"""let target = p""" + json.encode(skipped.display())? + "\n" + r"""
json.write target (json.decode("{")?) ?
"""
  let failed = test.expect(ctx, skipped_source, status: 3)?
  assert failed.stdout == ""
  assert ! skipped.exists()?
}
