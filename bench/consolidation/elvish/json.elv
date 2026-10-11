fn main {|root|
    var records = (from-json < $root'/index.json')
    var replacement = (from-json < $root'/replacement.json')
    var addition = (from-json < $root'/addition.json')
    var updated = [(each {|record|
        if (eq $record[name] $replacement[name]) {
            put $replacement
        } else {
            put $record
        }
    } $records)]
    set updated = (conj $updated $addition)
    var sorted = [(order &key={|record| put $record[name] } $updated)]
    # Fixture names are ASCII identifiers; explicit fields give deterministic JSON key order.
    print '['
    var separator = ''
    for record $sorted {
        print $separator
        printf '{"name":"%s","version":%d}' $record[name] $record[version]
        set separator = ','
    }
    echo ']'
}

main $@args
