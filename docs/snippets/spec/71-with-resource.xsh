proc publish(site: Path, page: Str) [fs, error] -> Result[Int] {
  # begin example
  with root = fs.open_root(site)?, held = fs.lock(fp"{site}/.lock")? {
    root.write(p"index.html", page)
    print f"published under {held.path.name()}"
  }

  let size = with root = fs.open_root(site)? { root.read_text(p"index.html")?.byte_len() }?
  # end example
  size
}

tempdir site {
  print f"{publish(site, "<h1>hello</h1>\n")?} bytes"
}
