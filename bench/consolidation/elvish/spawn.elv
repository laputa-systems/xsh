fn main {|root|
    for i [(range 128)] {
        /usr/bin/printf 'probe %s\n' $i
    }
}

main $@args
