type Config = {name: Str, enabled: Bool = true}

let name = "demo"
let config = Config(name:)
print $config.name

let settings = {build: {jobs: 2, enabled: true}}
let updated = {...settings, build.jobs: 4}
print $updated.build.jobs
