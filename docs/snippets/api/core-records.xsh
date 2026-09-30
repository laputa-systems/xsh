type Config = {name: Str, enabled: Bool = true}

let name = "demo"
let config = Config(name:)
print $config.name

let settings = {build: {jobs: 2, enabled: true}}
let updated = {...settings, build.jobs: 4}
print $updated.build.jobs

type Observation[T] = {value: T? = null, samples: List[T] = []}
type CountObservation = Observation[Int]
let count = CountObservation(value: 7)
let direct = Observation(value: "demo", samples: ["demo"])
let absent: Observation[Int] = Observation(value: null)
print ${count.value ?? 0}
print ${direct.samples[0]}
print ${absent.samples.len()}
