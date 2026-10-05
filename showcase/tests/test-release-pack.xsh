test test_release_pack { |ctx|
  let root = test.temp_dir(ctx, name: "release-src")?
  let out = test.temp_path(ctx, name: "release-out")
  fp"{root}/bin".mkdir()
  fp"{root}/bin/tool".write("tool")
  let output = run.text "xsh" "showcase/release-pack.xsh" -- $root $out --dry-run=false
  assert "archive " in output
  assert fp"{out}/release.tar".exists()?
  assert ! fp"{out.parent}/.{out.name()}.xsh-stage".exists()?
}

test test_release_pack_refuses_existing_output { |ctx|
  let source = test.temp_dir(ctx, name: "release-source")?
  fp"{source}/input".write("new release")
  let out = test.temp_dir(ctx, name: "existing-release")?
  let old_archive = fp"{out}/release.tar"
  old_archive.write("previous release")

  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  assert ! status.exited_with(0), "existing output must not be replaced"
  assert old_archive.read_text()? == "previous release"
}

test test_release_pack_cleans_failed_staging { |ctx|
  let source = test.temp_file(ctx, name: "not-a-directory", contents: b"invalid source")?
  let out = test.temp_path(ctx, name: "new-release")
  let pending = fp"{out.parent}/.{out.name()}.xsh-stage"
  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  assert ! status.exited_with(0), "invalid source must fail"
  assert ! out.exists()?
  assert ! pending.exists()?
}

test test_release_pack_rejects_output_inside_input { |ctx|
  let source = test.temp_dir(ctx, name: "nested-source")?
  fp"{source}/input".write("unchanged")
  let out = fp"{source}/release"
  let status = run.status "xsh" "showcase/release-pack.xsh" -- $source $out --dry-run=false
  assert ! status.exited_with(0), "output inside source would recurse during copy"
  assert fp"{source}/input".read_text()? == "unchanged"
  assert ! out.exists()?
}
