use re
use str

fn main {|root|
    from-lines < $root'/capability.h' | each {|line|
        for match [(re:find '^#define[ \t]+(CAP_[A-Z0-9_]+)[ \t]+([0-9]+)[ \t]*$' $line)] {
            var groups = $match[groups]
            echo '{"'(str:to-lower $groups[1][text])'",'$groups[2][text]'},'
        }
    }
}

main $@args
