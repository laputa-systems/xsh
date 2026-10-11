use os
use str

fn main {|root|
    var tree = $root'/tree'
    # Native globbing silently omits filesystem errors; find reports traversal failures.
    # Find's default traversal does not follow symlinks, including directory links.
    var paths = [(/usr/bin/find $tree -mindepth 1 -print | from-lines | order)]
    for entry $paths {
        var metadata = (os:stat &follow-symlink=$false $entry)
        var kind = ''
        if (eq $metadata[type] dir) {
            set kind = dir
        } elif (eq $metadata[type] regular) {
            set kind = file
        } elif (eq $metadata[type] symlink) {
            set kind = link
        } else {
            fail 'unsupported file type for '$entry
        }
        echo (str:trim-prefix $entry $tree'/')"\t"$kind
    }
}

main $@args
