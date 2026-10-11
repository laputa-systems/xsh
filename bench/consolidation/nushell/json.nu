def main [root: string] {
    let records = (open --raw ($root | path join 'index.json') | from json)
    let replacement = (open --raw ($root | path join 'replacement.json') | from json)
    let addition = (open --raw ($root | path join 'addition.json') | from json)
    let updated = ($records | each {|record|
        if $record.name == $replacement.name { $replacement } else { $record }
    } | append $addition | sort-by name)
    # Fixture names are ASCII identifiers; explicit fields give deterministic JSON key order.
    let encoded = ($updated | each {|record|
        '{"name":"' + $record.name + '","version":' + ($record.version | into string) + '}'
    } | str join ',')
    print ('[' + $encoded + ']')
}
