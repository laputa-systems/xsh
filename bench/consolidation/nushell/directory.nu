# Recurse only into actual directories, keeping symlink targets outside traversal.
def walk [directory: string, prefix: string] {
    mut rows = []
    for entry in (ls --all --short-names $directory) {
        let relative = if $prefix == '' { $entry.name } else { $prefix + '/' + $entry.name }
        let kind = match $entry.type {
            dir => 'dir'
            file => 'file'
            symlink => 'link'
            _ => { error make {msg: ('unsupported file type for ' + $relative)} }
        }
        $rows = ($rows | append {name: $relative, kind: $kind})
        if $kind == 'dir' {
            $rows = ($rows | append (walk ($directory | path join $entry.name) $relative))
        }
    }
    $rows
}

def main [root: string] {
    for entry in (walk ($root | path join 'tree') '' | sort-by name) {
        print ($entry.name + "\t" + $entry.kind)
    }
}
