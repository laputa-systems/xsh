let scratch = fs.tempdir()?
let dir = scratch.host_path()?

let release = fp"{dir}/app-1.4"
fp"{release}/bin".mkdir()
fp"{release}/bin/app".write("#!/bin/xsh\nprint \"app 1.4\"\n")
fp"{release}/README".write("app 1.4\n")

let tarball = fp"{dir}/app-1.4.tar.gz"
archive.create(tarball, dir, [p"app-1.4"], .Tar)

for entry in archive.list(tarball, .Tar)? |> sort-by .path.display() {
  let kind = if entry.kind == .Directory { "dir" } else { "file" }
  print f"{kind} {entry.path}"
}

let install = fp"{dir}/opt/app"
archive.extract(tarball, install, .Tar, strip_components: 1)
print fp"{install}/README".read_text()?.trim()
print (fp"{install}/bin/app".exists()?)
