def main [root: string] {
    let matches = (open --raw ($root | path join 'capability.h')
        | lines
        | parse --regex '^#define[ \t]+(?P<cap>CAP_[A-Z0-9_]+)[ \t]+(?P<digits>[0-9]+)[ \t]*$')
    for entry in $matches {
        print ('{"' + ($entry.cap | str downcase) + '",' + $entry.digits + '},')
    }
}
