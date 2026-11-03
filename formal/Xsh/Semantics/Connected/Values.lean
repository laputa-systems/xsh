import Xsh.Values.Composite
import Xsh.Validation.Nominal
import Xsh.Semantics.Environments
import Xsh.Syntax.Foundation.ResolvedIdentity
import Xsh.Typing.Connected.Types

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Values
open Xsh.Validation.Nominal
open Xsh.Syntax.Foundation
open Xsh.Semantics.Environments

inductive RuntimeType where
  | integer
  | boolean
  | string
  | error
  | record (fields : RuntimeType)
  | rowEmpty
  | rowField (name : NameId) (value tail : RuntimeType)
  | argumentsEmpty
  | argument (value tail : RuntimeType)
  | result (value error : RuntimeType)
  | function (arguments result : RuntimeType)
  | schemeFunction
  | nominal (identity : NominalId) (base : RuntimeType)
  deriving DecidableEq

inductive Value where
  | scalar (value : CompositeValue)
  | validationError
  | record (fields : List (NameId × Value))
  | ok (value : Value) (errorType : RuntimeType)
  | error (value : Value) (valueType : RuntimeType)
  | function (declaration : DeclarationId) (arguments result : RuntimeType)
  | schemeFunction (declaration : DeclarationId) (scheme : Xsh.Typing.Connected.Scheme)
  | instantiatedFunction (declaration : DeclarationId) (arguments result : RuntimeType)
      (solution : Xsh.Typing.Connected.Substitution)
      (typeArguments : List (Xsh.Typing.Connected.Variable × Xsh.Typing.Connected.MonoType))
  | nominal (identity : NominalId) (base : RuntimeType) (value : Value)


def insertField (field : NameId × RuntimeType) : List (NameId × RuntimeType) → List (NameId × RuntimeType)
  | [] => [field]
  | head :: rest =>
    if field.1.value ≤ head.1.value then field :: head :: rest
    else head :: insertField field rest

def canonicalFields : List (NameId × RuntimeType) → List (NameId × RuntimeType)
  | [] => []
  | field :: rest => insertField field (canonicalFields rest)

def rowFieldType (name : NameId) : RuntimeType → Option RuntimeType
  | .rowField other value tail => if name = other then some value else rowFieldType name tail
  | _ => none

mutual
  def typeFits : RuntimeType → RuntimeType → Bool
    | .record expected, .record actual => rowFits expected actual
    | .result expectedValue expectedError, .result actualValue actualError =>
        typeFits expectedValue actualValue && typeFits expectedError actualError
    | .nominal expected base, .nominal actual actualBase =>
        decide (expected = actual) && typeFits base actualBase
    | expected, actual => decide (expected = actual)

  def rowFits : RuntimeType → RuntimeType → Bool
    | .rowEmpty, _ => true
    | .rowField name value rest, actual =>
        match rowFieldType name actual with
        | none => false
        | some actualValue => typeFits value actualValue && rowFits rest actual
    | _, _ => false
end

mutual
  def valueType : Value → Option RuntimeType
    | .scalar (.integer _) => some .integer
    | .scalar (.boolean _) => some .boolean
    | .scalar (.string _) => some .string
    | .scalar _ => none
    | .validationError => some .error
    | .record fields =>
      if (fields.map Prod.fst).Nodup then
        (recordTypes fields).map fun types =>
          .record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty)
      else none
    | .ok value errorType => (valueType value).map (fun ty => .result ty errorType)
    | .error value valueType' => (valueType value).map (fun ty => .result valueType' ty)
    | .function _ arguments result => some (.function arguments result)
    | .schemeFunction _ _ => some .schemeFunction
    | .instantiatedFunction _ arguments result _ _ => some (.function arguments result)
    | .nominal identity base value => do
      let actual ← valueType value
      if typeFits base actual then some (.nominal identity base) else none

  def recordTypes : List (NameId × Value) → Option (List (NameId × RuntimeType))
    | [] => some []
    | (name, value) :: rest => do
      let ty ← valueType value
      let others ← recordTypes rest
      some ((name, ty) :: others)
end

def eraseNominal : Value → Value
  | .nominal _ _ value => eraseNominal value
  | value => value

def HasType (ty : RuntimeType) (value : Value) : Prop := valueType value = some ty

instance (ty : RuntimeType) (value : Value) : Decidable (HasType ty value) :=
  inferInstanceAs (Decidable (valueType value = some ty))

abbrev Store := TypedStore RuntimeType Value HasType

def write (store : Store) (cell : CellId) (candidate : Value) : StoreWriteResult Store :=
  writeCell (fun _ _ => inferInstance) store cell candidate

theorem write_rejects_wrong_type_atomically (store : Store) (cell : CellId)
    (candidate : Value) (current : StoreCell RuntimeType Value HasType)
    (present : store cell = some current) (mutable : current.mutability = .mutable)
    (wrong : ¬ HasType current.contents.typeTag candidate) :
    (write store cell candidate).storeAfter = store := by
  exact Xsh.Semantics.Environments.write_rejects_nonmember_atomically (fun _ _ => inferInstance) store cell candidate
    current present mutable wrong

end Xsh.Semantics.Connected
