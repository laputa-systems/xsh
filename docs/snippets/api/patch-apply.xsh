let patch_text = ""
let result = patch.apply(p"root", patch_text)?
print $result.files $result.hunks
