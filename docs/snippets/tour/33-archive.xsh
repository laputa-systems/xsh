let scratch = fs.tempdir()?
defer scratch.close()?
let dir = scratch.host_path()?

let release = fp"{dir}/app-1.4"
fs.mkdir(fp"{release}/bin")
fp"{release}/bin/app".write("#!/bin/xsh\nprint \"app 1.4\"\n")
fp"{release}/README".write("app 1.4\n")

let tarball = fp"{dir}/app-1.4.tar.gz"
archive.tar_create(tarball, dir, [p"app-1.4"])

for entry in archive.tar_list(tarball)? |> sort-by .path.display() {
  print f"{entry.kind} {entry.path}"
}

let install = fp"{dir}/opt/app"
archive.tar_extract(tarball, install, strip_components: 1)
print fp"{install}/README".read_text()?.trim()
print (fp"{install}/bin/app".exists()?)
