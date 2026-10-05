pure beneath(root: Path, rel: RelPath) -> Path {
  fp"{root}/{rel}"
}

let name = "notes.txt"
let computed = Path(name)
let dir: RelPath = "srv/data"
let absolute: RelPath = "/etc/passwd" # error: check.validated-literal
let escaping: RelPath = "a/../../b" # error: check.validated-literal
let text: RelPath = fp"{dir}/{name}" # error: check.validated-literal
let glued: RelPath = fp"{dir}-old" # error: check.validated-literal
let above: RelPath = fp"{dir}/../.." # error: check.validated-literal
let plain: RelPath = computed # error: check.type-mismatch
let widened: RelPath = dir.with_ext("bak") # error: check.type-mismatch
print ${beneath(p"/", computed)} # error: check.type-mismatch
print $absolute $escaping $text $glued $above $plain $widened
