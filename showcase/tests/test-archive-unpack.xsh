test test_archive_unpack [fs, process, error] { |ctx|
  let src = test.temp_dir(ctx, name: "arc-src")?
  fp"${src}/a.txt".write("alpha")?
  fp"${src}/b.txt".write("beta")?
  let tarball = test.temp_path(ctx, name: "test.tar.gz")
  run.text "tar" "czf" $tarball "-C" $src "." ?
  let out = test.temp_path(ctx, name: "arc-out")
  let extract_out = run.text "xsh" "showcase/archive-unpack.xsh" -- $tarball --out $out --dry-run=false ?
  "entries in test.tar.gz" in extract_out
  "extracted to" in extract_out
  fp"${out}/a.txt".exists()?
  let usage = run.text "xsh" "showcase/archive-unpack.xsh" -- --help ?
  "usage:" in usage
}

test test_archive_unpack_failure_leaves_existing_destination_untouched [fs, process, error] { |ctx|
  let src = test.temp_dir(ctx, name: "partial-src")?
  fp"${src}/a.txt".write("first")?
  fp"${src}/b.txt".write("second")?
  let tarball = test.temp_path(ctx, name: "partial.tar")
  archive.tar_create(tarball, src, [p"a.txt", p"b.txt"])?

  let out = test.temp_dir(ctx, name: "partial-out")?
  fp"${out}/b.txt".mkdir()?
  fp"${out}/b.txt/marker".write("untouched")?
  let status = run.status "xsh" "showcase/archive-unpack.xsh" -- $tarball --out $out --dry-run=false
  test.ok(! status.exited_with(0), "conflicting extraction must fail")?
  test.ok(! fp"${out}/a.txt".exists()?, "failed extraction must not leave earlier files")?
  fp"${out}/b.txt/marker".read_text()? == "untouched"
}

test test_archive_unpack_cleans_partial_staging_after_unsafe_member [fs, process, error] { |ctx|
  let src = test.temp_dir(ctx, name: "unsafe-src")?
  fp"${src}/a.txt".write("first")?
  fs.symlink(../outside, fp"${src}/bad")?
  let tarball = test.temp_path(ctx, name: "unsafe.tar")
  archive.tar_create(tarball, src, [p"a.txt", p"bad"])?

  let out = test.temp_path(ctx, name: "unsafe-out")
  let pending = fp"${out.parent}/.${out.name()}.xsh-stage"
  let status = run.status "xsh" "showcase/archive-unpack.xsh" -- $tarball --out $out --dry-run=false
  test.ok(! status.exited_with(0), "unsafe member must fail extraction")?
  test.ok(! out.exists()?, "failed extraction must not publish partial output")?
  test.ok(! pending.exists()?, "failed extraction must clean its staging directory")?
}

test test_archive_unpack_compress_and_decompress_publish_files [fs, process, error] { |ctx|
  let source = test.temp_file(ctx, name: "compress.txt", contents: b"round trip")?
  run.text "xsh" "showcase/archive-unpack.xsh" -- --compress $source --dry-run=false ?
  let compressed = fp"${source}.gz"
  compressed.exists()?
  let restored = test.temp_path(ctx, name: "restored.txt")
  run.text "xsh" "showcase/archive-unpack.xsh" -- --decompress $compressed --out $restored --dry-run=false ?
  restored.read_text()? == "round trip"
  ! fp"${restored.parent}/.${restored.name()}.xsh-stage".exists()?
}

test test_archive_unpack_cancellation_during_compression_cleans_staging [fs, process, time, error] { |ctx|
  let source = test.temp_path(ctx, name: "compress-fifo")
  fs.mkfifo(source, 0o600)?
  let dest = fp"${source}.gz"
  let pending = fp"${dest.parent}/.${dest.name()}.xsh-stage"
  let writer_ready = test.temp_path(ctx, name: "writer-ready")
  let executable = fp"${fs.cwd()?}/target/debug/xsh"
  let archive_child = spawn process.command_argv(
    executable,
    [executable.display(), "showcase/archive-unpack.xsh", "--", "--compress", source, "--dry-run=false"],
  )?
  let writer = spawn process.command_argv(
    "sh",
    ["sh", "-c", "exec 3>\"$1\"; printf payload >&3; touch \"$2\"; sleep 60", "sh", source, writer_ready],
  )?

  for _ in range(0, 500) {
    break when writer_ready.exists()?

    time.sleep(10ms)?
  }

  test.ok(writer_ready.exists()?, "writer must hold the FIFO open after sending data")?
  test.ok(pending.exists()?, "compression must be in its staging directory")?
  process.kill(archive_child.pid, signal: "TERM")?
  time.sleep(50ms)?
  writer.cancel(signal: "TERM", kill_after: 0ms)?
  let status = wait archive_child?
  test.ok(status.exited_with(3), f"canceled archive must report runtime cancellation: ${status.exit_code() ?? -1}")?
  test.ok(! dest.exists()?, "canceled compression must not publish output")?
  test.ok(! pending.exists()?, "canceled compression must clean staged output")?
}
