import Xsh.Semantics.Connected.SafetyValueBlockProjection

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Syntax.Foundation

theorem read_preserves_callable_provenance (functions : FunctionTable) (store : Store) (cells : ActiveCells)
    (safe : StoreCallables functions store) (binder : Xsh.Foundation.BinderId) (value : Value)
    (found : Connected.read store cells binder = some value) : ValueCallables functions value := by
  unfold Connected.read at found
  cases active : cells binder with
  | none => simp [active] at found
  | some cell =>
    cases present : store cell with
    | none => simp [active, present] at found
    | some entry =>
      simp [active, present] at found
      subst value
      exact safe cell entry present

theorem instantiated_scheme_is_callable (functions : FunctionTable) (declaration : DeclarationId)
    (scheme : Scheme) (captured : Substitution) (arguments : List (Variable × MonoType))
    (parameters result : RuntimeType) (normalized : List (Variable × MonoType))
    (safe : ValueCallables functions (.schemeFunction declaration scheme)) (sorted : WellSorted captured)
    (instantiated : instantiateFunction captured scheme arguments = some (parameters, result, normalized)) :
    ValueCallables functions (.instantiatedFunction declaration parameters result captured arguments) := by
  cases safe with
  | scheme correct =>
    rcases correct with ⟨function, found, sameScheme⟩ | ⟨found, shape⟩ | ⟨found, shape⟩ | ⟨found, shape⟩
    · cases sameScheme
      exact .instantiatedFunction found sorted instantiated
    · obtain ⟨success, failure, parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedOk found
    · obtain ⟨success, failure, parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedError found
    · obtain ⟨parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedTime found

end Xsh.Semantics.Connected
