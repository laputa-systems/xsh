proc stage(root: Path) -> Result[Str] {
  # begin example
  tempdir scratch at fp"{root}/stage" {
    fp"{scratch}/stamp".write("staged\n")?
    fp"{scratch}/stamp".read_text()?
  }
  # end example
}

let root = fs.tempdir()?
defer root.close()?
print stage(root.host_path()?)?.trim()
