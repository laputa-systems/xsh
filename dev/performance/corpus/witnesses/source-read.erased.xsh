type TextObservation = {state: Str, value: Str?}
type SourceRead = {observation: TextObservation, errno: Int?, error_kind: Str?}
type BoundedNumber = {value: Str?}

pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumber {
  {value: source.observation.value}
}

pure cgroup_maximum(source) -> BoundedNumber {
  if source.observation.state == "Observed" and source.observation.value == "max" {
    return {value: null}
  }
  bounded_number(source, true)
}

let source: SourceRead = {
  observation: {state: "Observed", value: "8"},
  errno: null,
  error_kind: null,
}
print ${cgroup_maximum(source).value ?? "none"}
