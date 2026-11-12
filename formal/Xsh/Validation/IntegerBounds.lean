import Xsh.Values.Scalars

namespace Xsh.Validation

inductive IntegerBase where
  | int
  | uint
  deriving DecidableEq

def IntegerBase.accepts : IntegerBase → Int → Prop
  | .int, value => Xsh.Values.signed64Min ≤ value ∧ value < Xsh.Values.signed64Limit
  | .uint, value => 0 ≤ value ∧ value < Xsh.Values.signed64Limit

structure BoundedSpec where
  base : IntegerBase
  low : Int
  high : Int
  nonempty : low ≤ high
  low_in_base : base.accepts low
  high_in_base : base.accepts high
  deriving DecidableEq

def BoundedSpec.accepts (spec : BoundedSpec) (value : Int) : Prop :=
  spec.low ≤ value ∧ value ≤ spec.high ∧ spec.base.accepts value

instance (spec : BoundedSpec) (value : Int) : Decidable (spec.accepts value) := by
  cases spec with
  | mk base low high nonempty low_in_base high_in_base =>
      cases base <;> dsimp [BoundedSpec.accepts, IntegerBase.accepts] <;> infer_instance

end Xsh.Validation
