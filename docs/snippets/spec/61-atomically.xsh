proc save_image(tag: Str, image: Path) [fs, process, error] {
  # begin example
  atomically replace image as partial {
    run docker save --output $partial $tag
    fs.fsync(partial)
  }
  # end example
}

let root = fs.tempdir()?
defer root.close()
let image = fp"{root.host_path()?}/image.tar"
print f"{image.exists()?}"
