proc package(source: Path, tarball: Path) [fs, process, error] -> Result[Int] {
  # begin example
  tempdir stage {
    let _ = fs.copy_tree(source, fp"{stage}/payload")?
    run tar -C $stage -czf $tarball payload
  }

  let files = tempdir scratch {
    run tar -C $scratch -xzf $tarball
    fs.files(scratch)? |> count()
  }?
  # end example
  files
}
