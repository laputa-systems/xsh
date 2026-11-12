import Xsh.Effects.Lattice
import Init.Data.Vector.OfFn
import Init.Data.Vector.DecidableEq

namespace Xsh.Core

open Xsh.Syntax Xsh.Semantics.Effects

def effectPosition (effect : Effect) : Fin allEffects.length :=
  ⟨allEffects.idxOf effect, by cases effect <;> decide⟩

structure EffectSet where
  membership : Vector Bool allEffects.length
  deriving DecidableEq

def EffectSet.contains (effects : EffectSet) (effect : Effect) : Bool :=
  effects.membership.get (effectPosition effect)

def EffectSet.ofList (effects : List Effect) : EffectSet :=
  ⟨Vector.ofFn fun position => decide (allEffects[position] ∈ effects)⟩

def EffectSet.denotation (effects : EffectSet) : FiniteEffectSet :=
  fun effect => effects.contains effect = true

theorem EffectSet.ofList_contains (effects : List Effect) (effect : Effect) :
    (EffectSet.ofList effects).contains effect = decide (effect ∈ effects) := by
  simp [EffectSet.ofList, EffectSet.contains, Vector.get, Fin.cast, effectPosition]

theorem EffectSet.ofList_denotation (effects : List Effect) :
    (EffectSet.ofList effects).denotation = Xsh.Semantics.Effects.ofList effects := by
  funext effect
  simp [EffectSet.denotation, EffectSet.ofList_contains, Xsh.Semantics.Effects.ofList]

inductive EffectBound where
  | known (required : EffectSet)
  | unrestricted
  | unknown
  deriving DecidableEq

def EffectBound.denotation : EffectBound → EffectContract
  | .known required => .checked required.denotation
  | .unrestricted => .unrestricted
  | .unknown => .unknown

end Xsh.Core
