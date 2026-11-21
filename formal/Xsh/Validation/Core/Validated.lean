import Xsh.Semantics.CoreWorld

namespace Xsh.Validation.Core

open Xsh.Core

variable {Declaration Identity Source : Type}

inductive ValidationIdentity (Declaration : Type) where
  | nonEmpty (element : ValueType Declaration)
  | relativePath
  | nominal (declaration : Declaration) (fields : RecordType Declaration)
  | bounded (bounds : Xsh.Validation.BoundedSpec)
  deriving DecidableEq

def validation : ValueType Declaration → Option (ValidationIdentity Declaration)
  | .nonEmpty element => some (.nonEmpty element)
  | .relativePath => some .relativePath
  | .nominal declaration fields => some (.nominal declaration fields)
  | .boundedInteger bounds => some (.bounded bounds)
  | _ => none

def IntegerBaseType : Xsh.Validation.IntegerBase → ValueType Declaration
  | .int => .primitive .integer
  | .uint => .primitive .unsignedInteger

def base : ValueType Declaration → Option (ValueType Declaration)
  | .nonEmpty element => some (.list element)
  | .relativePath => some (.primitive .path)
  | .nominal _ fields => some (.record fields)
  | .boundedInteger bounds => some (IntegerBaseType bounds.base)
  | _ => none

mutual
  inductive BaseView : ValueType Declaration → ValueType Declaration → Prop where
    | refl (type : ValueType Declaration) : BaseView type type
    | validated {validatedType baseType target : ValueType Declaration} :
        base validatedType = some baseType → BaseView baseType target → BaseView validatedType target
    | list {source target : ValueType Declaration} : BaseView source target → BaseView (.list source) (.list target)
    | map (key : ValueType Declaration) {source target : ValueType Declaration} :
        BaseView source target → BaseView (.map key source) (.map key target)
    | stream {source target : ValueType Declaration} :
        BaseView source target → BaseView (.stream source) (.stream target)
    | record {source target : RecordType Declaration} :
        RowView source target → BaseView (.record source) (.record target)
    | optional {source target : ValueType Declaration} :
        BaseView source target → BaseView (.optional source) (.optional target)
    | result {sourceValue targetValue sourceError targetError : ValueType Declaration} :
        BaseView sourceValue targetValue → BaseView sourceError targetError →
          BaseView (.result sourceValue sourceError) (.result targetValue targetError)
    | union {sourceFirst targetFirst sourceSecond targetSecond : ValueType Declaration}
        {sourceRest targetRest : UnionTail Declaration} :
        BaseView sourceFirst targetFirst → BaseView sourceSecond targetSecond → MembersView sourceRest targetRest →
          BaseView (.union sourceFirst sourceSecond sourceRest) (.union targetFirst targetSecond targetRest)
    | callable {source target : SignatureType Declaration} :
        SignatureView source target → BaseView (.callable source) (.callable target)

  inductive RowView : RecordType Declaration → RecordType Declaration → Prop where
    | empty : RowView .rowEmpty .rowEmpty
    | field (name : String) {source target : ValueType Declaration} {sourceRest targetRest : RecordType Declaration} :
        BaseView source target → RowView sourceRest targetRest →
          RowView (.rowField name source sourceRest) (.rowField name target targetRest)

  inductive MembersView : UnionTail Declaration → UnionTail Declaration → Prop where
    | empty : MembersView .membersEmpty .membersEmpty
    | member {source target : ValueType Declaration} {sourceRest targetRest : UnionTail Declaration} :
        BaseView source target → MembersView sourceRest targetRest →
          MembersView (.member source sourceRest) (.member target targetRest)

  inductive SignatureView : SignatureType Declaration → SignatureType Declaration → Prop where
    | result (policy : InvocationPolicy 0) (parameters : TypeVariableFreeTerm 0 String Declaration .arguments)
        {source target : ValueType Declaration} :
        BaseView source target →
          SignatureView (.signature policy parameters source) (.signature policy parameters target)
end

structure ViewClosed (world : MembershipWorld Declaration Identity Source) : Prop where
  stream : ∀ reference source target, BaseView source target →
    world.stream reference source → world.stream reference target
  callable : ∀ reference source target, SignatureView source target →
    world.callable reference source → world.callable reference target

def eraseValidated : ValueType Declaration → Value Declaration Identity Source → Value Declaration Identity Source
  | .nominal _ _, .nominal _ contents => contents
  | _, candidate => candidate

end Xsh.Validation.Core
