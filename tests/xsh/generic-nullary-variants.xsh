enum GenericVariantState: Str {
  GenericObserved = "observed",
  GenericMissing = "missing",
}

pure contextual_variant_keep(value, state: GenericVariantState) {
  if state == .GenericObserved { value } else { value }
}

pure named_variant_keep(value, state: GenericVariantState) {
  if state == GenericObserved { value } else { value }
}

pure shadowed_variant_keep(value, GenericObserved: GenericVariantState) {
  {value, observed: GenericObserved == .GenericObserved}
}

type VariantField = {GenericObserved: Str}

pure callback_variant_fields() -> List[VariantField] {
  [{GenericObserved: "b"}, {GenericObserved: "a"}] |> sort-by .GenericObserved
}

test generic_nullary_variants_preserve_nominal_values {
  assert contextual_variant_keep(7, GenericObserved) == 7
  assert contextual_variant_keep("kept", GenericMissing) == "kept"
  assert named_variant_keep(7, GenericObserved) == 7
  assert named_variant_keep("kept", GenericMissing) == "kept"
  assert shadowed_variant_keep(7, GenericMissing) == {value: 7, observed: false}
  assert shadowed_variant_keep("kept", GenericObserved) == {value: "kept", observed: true}
  assert callback_variant_fields()[0].GenericObserved == "a"
}
