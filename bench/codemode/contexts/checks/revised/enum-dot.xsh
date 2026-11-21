tempdir dir {
  fp"{dir}/src/a.txt".parent().mkdir()
  fp"{dir}/src/a.txt".write("a\n")
  let dest = fp"{dir}/out.tar.gz"
  archive.create(dest, fp"{dir}/src", [p"a.txt"], .Tar)
  archive.create(dest, fp"{dir}/src", [p"a.txt"], .Tar, collision: .Replace)
  let names = archive.list(dest, .Tar)? |> map { |e| e.path.display() }
  print (names.join(" "))
}
