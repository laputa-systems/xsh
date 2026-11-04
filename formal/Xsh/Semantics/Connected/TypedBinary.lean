import Xsh.Semantics.Connected.TypedScope

namespace Xsh.Semantics.Connected

open Xsh.Values
open Xsh.Values.IntegerOps
open Xsh.Typing.Connected
open Xsh.Syntax

theorem integer_value_shape (value : Value) (typed : valueType value = some .integer) :
    ∃ integer, value = .scalar (.integer integer) := by
  cases value with
  | scalar scalar =>
    cases scalar <;> simp [valueType] at typed
    case integer integer => exact ⟨integer, rfl⟩
  | record fields => simp [valueType] at typed
  | ok value failure | error value failure => simp [valueType] at typed
  | nominal identity base value =>
    cases inner : valueType value <;> simp [valueType, inner] at typed
  | _ => simp [valueType] at typed

theorem boolean_value_shape (value : Value) (typed : valueType value = some .boolean) :
    ∃ boolean, value = .scalar (.boolean boolean) := by
  cases value with
  | scalar scalar =>
    cases scalar <;> simp [valueType] at typed
    case boolean boolean => exact ⟨boolean, rfl⟩
  | record fields => simp [valueType] at typed
  | ok value failure | error value failure => simp [valueType] at typed
  | nominal identity base value =>
    cases inner : valueType value <;> simp [valueType, inner] at typed
  | _ => simp [valueType] at typed

def BinaryResultTyped (expected : RuntimeType) : Except Fault Value → Prop
  | .ok value => valueType value = some expected
  | .error (.integer _) => True
  | .error _ => False

theorem integer_binary_has_typed_result_or_integer_fault (operator : BinaryOp) (operation : IntegerBinaryOp)
    (left right : Value) (selected : integerOperator operator = some operation)
    (leftTyped : valueType left = some .integer) (rightTyped : valueType right = some .integer) :
    BinaryResultTyped .integer (evaluateBinary operator left right) := by
  obtain ⟨first, rfl⟩ := integer_value_shape left leftTyped
  obtain ⟨second, rfl⟩ := integer_value_shape right rightTyped
  cases result : applyBinary operation first second <;>
    simp [evaluateBinary, selected, result, BinaryResultTyped, valueType]

theorem typed_binary_result_or_integer_fault (operator : BinaryOp) (leftType rightType resultType : MonoType)
    (left right : Value) (typing : BinaryTyping operator leftType rightType resultType)
    (leftTyped : valueType left = monoRuntime leftType) (rightTyped : valueType right = monoRuntime rightType) :
    ∃ runtimeType, monoRuntime resultType = some runtimeType ∧
      BinaryResultTyped runtimeType (evaluateBinary operator left right) := by
  cases typing with
  | add => exact ⟨.integer, rfl, integer_binary_has_typed_result_or_integer_fault .add .add left right rfl leftTyped rightTyped⟩
  | sub => exact ⟨.integer, rfl, integer_binary_has_typed_result_or_integer_fault .sub .subtract left right rfl leftTyped rightTyped⟩
  | mul => exact ⟨.integer, rfl, integer_binary_has_typed_result_or_integer_fault .mul .multiply left right rfl leftTyped rightTyped⟩
  | div => exact ⟨.integer, rfl, integer_binary_has_typed_result_or_integer_fault .div .divide left right rfl leftTyped rightTyped⟩
  | rem => exact ⟨.integer, rfl, integer_binary_has_typed_result_or_integer_fault .rem .remainder left right rfl leftTyped rightTyped⟩
  | lt | le | gt | ge =>
    obtain ⟨first, rfl⟩ := integer_value_shape left leftTyped
    obtain ⟨second, rfl⟩ := integer_value_shape right rightTyped
    exact ⟨.boolean, rfl, by simp [evaluateBinary, integerOperator, BinaryResultTyped, valueType]⟩
  | and | or =>
    obtain ⟨first, rfl⟩ := boolean_value_shape left leftTyped
    obtain ⟨second, rfl⟩ := boolean_value_shape right rightTyped
    exact ⟨.boolean, rfl, by simp [evaluateBinary, BinaryResultTyped, valueType]⟩

theorem typed_binary_frame_progress (state : Machine) (operator : BinaryOp)
    (leftType rightType resultType : MonoType) (left right : Value)
    (typing : BinaryTyping operator leftType rightType resultType)
    (leftTyped : valueType left = monoRuntime leftType) (rightTyped : valueType right = monoRuntime rightType) :
    ∃ runtimeType control, monoRuntime resultType = some runtimeType ∧
      Step { state with control := .value right, stack := .binaryRight operator left :: state.stack } .internal
        { state with control := control } ∧
      ((∃ value, control = .value value ∧ valueType value = some runtimeType) ∨
        (∃ fault, control = .fault (.integer fault))) := by
  obtain ⟨runtimeType, resultClosed, safe⟩ :=
    typed_binary_result_or_integer_fault operator leftType rightType resultType left right typing leftTyped rightTyped
  cases outcome : evaluateBinary operator left right with
  | ok value =>
    refine ⟨runtimeType, .value value, resultClosed, Step.binaryResult state operator left right value outcome, ?_⟩
    left
    exact ⟨value, rfl, by simpa [outcome, BinaryResultTyped] using safe⟩
  | error fault =>
    have permitted : BinaryResultTyped runtimeType (.error fault) := by simpa [outcome] using safe
    cases fault <;> simp [BinaryResultTyped] at permitted
    case integer fault =>
      exact ⟨runtimeType, .fault (.integer fault), resultClosed,
        Step.binaryFault state operator left right (.integer fault) outcome, Or.inr ⟨fault, rfl⟩⟩

end Xsh.Semantics.Connected
