type Config = {name: Str, enabled: Bool = true}

const name = "demo"
let config = Config(name:)
print $config.name

const settings = {build: {jobs: 2, enabled: true}}
let updated = {...settings, build.jobs: 4}
print $updated.build.jobs

type Observation[T] = {value: T? = null, samples: List[T] = []}

type CountObservation = Observation[Int]

let count = Observation(value: 7)
let direct = Observation(value: "demo", samples: ["demo"])
let absent: CountObservation = Observation(value: null)
print ${count.value ?? 0}
print direct.samples[0]
print absent.samples.len()
