proc stage(root: Path) [fs, error] -> Result[Str] {
  # begin example
  tempdir scratch at fp"{root}/stage" {
    fp"{scratch}/stamp".write("staged\n")
  }

  let stamp = tempdir scratch at fp"{root}/stage" {
    fp"{scratch}/stamp".write("staged again\n")
    fp"{scratch}/stamp".read_text()?
  }?
  # end example
  stamp
}

let root = fs.tempdir()?
defer root.close()?
print stage(root.host_path()?)?.trim()
