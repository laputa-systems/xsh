type RequirementCount = {jobs: UInt}
pure inferred_requirement_parameter(value: RequirementCount) -> UInt { value.jobs }
pure inferred_requirement_via_parameter(raw: Any) -> Result[UInt] {
  let checked: RequirementCount = raw.require()?
  let _ = checked.jobs
  inferred_requirement_parameter(raw.require()?)
}
