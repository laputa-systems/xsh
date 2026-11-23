##! Skips for native tests whose fixture needs a host facility macOS or a
##! restricted Linux host lacks. Each skip names the missing facility.

## Skips the calling test where the filesystem refuses file names that are
## not valid UTF-8 (EILSEQ, as APFS does).
export proc raw_filenames(ctx: TestContext) [fs, error] {
  let dir = test.temp_dir(ctx, name: "raw-filename-probe")?
  let probe = bytes.concat([dir.bytes(), b"/probe-\xff"]) as Path
  if let Err(failure) = probe.write(b"") {
    assert (failure.errno ?? 0) in [84, 92], failure.message
    test.skip("the filesystem refuses file names that are not valid UTF-8")
  }
}

## Skips the calling test on a host whose kernel is not Linux.
export proc linux_kernel(reason: Str) [env, error] {
  if system.uname()?.sysname != "Linux" { test.skip(reason) }
}

## Skips the calling test where the host lacks `required`.
export proc host_path(required: Path, reason: Str) [fs, error] {
  if ! required.exists() { test.skip(reason) }
}
