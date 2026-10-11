def main [root: string] {
    for i in 0..127 {
        let result = (do { ^/usr/bin/printf 'probe %s\n' ($i | into string) } | complete)
        print --no-newline $result.stdout
        if $result.exit_code != 0 {
            print --stderr --no-newline $result.stderr
            exit $result.exit_code
        }
    }
}
