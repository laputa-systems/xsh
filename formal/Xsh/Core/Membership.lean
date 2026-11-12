import Xsh.Core.Values
import Xsh.Validation.RelPath

namespace Xsh.Core

open Xsh.Values

abbrev ValueType (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .value
abbrev RecordType (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .row
abbrev ExportType (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .exports
abbrev UnionTail (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .members
abbrev SignatureType (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .signature

/-- Retained contracts describe semantic objects without inspecting suspended
stream items or invoking callables. Establishing these predicates from source
checking and preserving them during execution are separate obligations. -/
structure MembershipWorld (Declaration Identity Source : Type) where
  entryFields : Reference Identity .filesystemEntry → List (String × Value Declaration Identity Source)
  stream : Reference Identity .stream → ValueType Declaration → Prop
  callable : Reference Identity .callable → SignatureType Declaration → Prop
  scheme : Reference Identity .callable → Declaration → Prop
  generic : Reference Identity .callable → CallableKind → Prop

namespace Membership

variable {Declaration Identity Source : Type}

def recordFields (world : MembershipWorld Declaration Identity Source)
    (value : Value Declaration Identity Source) : Option (List (String × Value Declaration Identity Source)) :=
  match value.eraseNominal with
  | .record fields => some fields
  | .filesystemEntry reference => some (world.entryFields reference)
  | _ => none

def primitive (type : PrimitiveType) (value : Value Declaration Identity Source) : Prop :=
  match type, value.eraseNominal with
  | .any, _ => True
  | .null, .scalar .null
  | .boolean, .scalar (.boolean _)
  | .integer, .scalar (.integer _)
  | .float, .scalar (.float _)
  | .duration, .scalar (.duration _)
  | .string, .scalar (.string _)
  | .bytes, .scalar (.bytes _)
  | .digest, .scalar (.digest _)
  | .regex, .scalar (.regex _)
  | .path, .scalar (.path _)
  | .erasedRecord, .record _
  | .erasedRecord, .filesystemEntry _
  | .dynamicModule, .module _ _
  | .status, .status _
  | .environmentPaths, .environmentPaths _
  | .error, .error _
  | .error, .processError _
  | .processError, .processError _
  | .command, .command _
  | .processHandle, .processHandle _
  | .networkJob, .networkJob _
  | .filesystemRoot, .filesystemRoot _
  | .filesystemLock, .filesystemLock _
  | .sshSession, .sshSession _
  | .unit, .scalar .unit => True
  | .unsignedInteger, .scalar (.integer value) => 0 ≤ value.val
  | _, _ => False

def signatureKind : SignatureType Declaration → CallableKind
  | .variable id => nomatch id
  | .signature policy _ _ => policy.kind

def callable (world : MembershipWorld Declaration Identity Source)
    (type : SignatureType Declaration) (value : Value Declaration Identity Source) : Prop :=
  match value.eraseNominal with
  | .callable reference kind => kind = signatureKind type ∧ world.callable reference type
  | _ => False

def generic (world : MembershipWorld Declaration Identity Source)
    (kind : CallableKind) (value : Value Declaration Identity Source) : Prop :=
  match value.eraseNominal with
  | .callable reference actual => actual = kind ∧ world.generic reference kind
  | _ => False

def exportNames : ExportType Declaration → List String
  | .variable id => nomatch id
  | .exportsEmpty => []
  | .exportValue name _ _ rest
  | .exportCallable name _ _ rest
  | .exportGenericCallable name _ _ rest => name :: exportNames rest

def absentAllowed : ExportPresence → Prop
  | .required => False
  | .optional => True

/-- Map and set keys have the six scalar representations admitted by the
language. Their type domain does not include compound or validated types. -/
def key (type : ValueType Declaration) (value : MapKey) : Prop :=
  match type with
  | .primitive .any | .primitive .boolean | .primitive .integer | .primitive .unsignedInteger
  | .primitive .duration | .primitive .string | .primitive .bytes | .primitive .path =>
      match type with
      | .primitive type => primitive type (Value.scalar (Declaration := Declaration) (Identity := Unit) (Source := Unit) value.scalar)
      | _ => False
  | _ => False

mutual
  def value (world : MembershipWorld Declaration Identity Source)
      (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Prop :=
    match type with
    | .variable id => nomatch id
    | .primitive type => primitive type candidate
    | .list element =>
        match candidate.eraseNominal with
        | .list items => ∀ item ∈ items, value world element item
        | .environmentPaths paths => ∀ path ∈ paths, value world element (.scalar (.path path))
        | _ => False
    | .map keyType element =>
        match candidate.eraseNominal with
        | .map entries => ∀ entry ∈ entries, key keyType entry.1 ∧ value world element entry.2
        | .record fields => keyType = .primitive .string ∧ ∀ field ∈ fields, value world element field.2
        | _ => False
    | .set element =>
        match candidate.eraseNominal with
        | .set items => ∀ item ∈ items, key element item
        | _ => False
    | .stream element =>
        match candidate.eraseNominal with
        | .stream reference => world.stream reference element
        | _ => False
    | .record fields =>
        match recordFields world candidate with
        | some actual => row world fields actual
        | none => False
    | .module closure required =>
        match candidate.eraseNominal with
        | .module _ actual => exports world required actual ∧
            (closure = .exact → ∀ field ∈ actual, field.1 ∈ exportNames required)
        | _ => False
    | .result success failure =>
        match candidate.eraseNominal with
        | .ok item => value world success item
        | .err item => value world failure item
        | _ => False
    | .optional element => primitive .null candidate ∨ value world element candidate
    | .union first second rest => value world first candidate ∨ value world second candidate ∨ members world rest candidate
    | .functionClass kind =>
        match candidate.eraseNominal with
        | .callable _ actual => actual = kind
        | _ => False
    | .callable signature => callable world signature candidate
    | .schemeCallable declaration =>
        match candidate.eraseNominal with
        | .callable reference _ => world.scheme reference declaration
        | _ => False
    | .tagged declaration =>
        match candidate.eraseNominal with
        | .tagged actual _ _ => actual = declaration
        | _ => False
    | .errorFamily declaration =>
        match candidate.eraseNominal with
        | .error error => error.family = declaration
        | _ => False
    | .errorVariant family variant =>
        match candidate.eraseNominal with
        | .error error => error.family = family ∧ error.variant = variant
        | _ => False
    | .errorFacet facet =>
        match candidate.eraseNominal with
        | .error error => facet ∈ error.facets
        | _ => False
    | .nonEmpty element =>
        match candidate.eraseNominal with
        | .list items => items ≠ [] ∧ ∀ item ∈ items, value world element item
        | .environmentPaths paths => paths ≠ [] ∧ ∀ path ∈ paths, value world element (.scalar (.path path))
        | _ => False
    | .relativePath =>
        match candidate.eraseNominal with
        | .scalar (.path path) => Xsh.Validation.RelPath.isRelPath path.bytes
        | _ => False
    | .nominal declaration fields =>
        match candidate with
        | .nominal actual contents => actual = declaration ∧
            match recordFields world contents with
            | some actualFields => row world fields actualFields
            | none => False
        | _ => False
    | .boundedInteger bounds =>
        match candidate.eraseNominal with
        | .scalar (.integer integer) => bounds.accepts integer.val
        | _ => False

  def row (world : MembershipWorld Declaration Identity Source) (type : RecordType Declaration)
      (fields : List (String × Value Declaration Identity Source)) : Prop :=
    match type with
    | .variable id => nomatch id
    | .rowEmpty => True
    | .rowField name type rest =>
        (match fields.lookup name with
         | some candidate => value world type candidate
         | none => False) ∧ row world rest fields

  def exports (world : MembershipWorld Declaration Identity Source) (type : ExportType Declaration)
      (fields : List (String × Value Declaration Identity Source)) : Prop :=
    match type with
    | .variable id => nomatch id
    | .exportsEmpty => True
    | .exportValue name presence type rest =>
        (match fields.lookup name with
         | some candidate => value world type candidate
         | none => absentAllowed presence) ∧ exports world rest fields
    | .exportCallable name presence signature rest =>
        (match fields.lookup name with
         | some candidate => callable world signature candidate
         | none => absentAllowed presence) ∧ exports world rest fields
    | .exportGenericCallable name presence kind rest =>
        (match fields.lookup name with
         | some candidate => generic world kind candidate
         | none => absentAllowed presence) ∧ exports world rest fields

  def members (world : MembershipWorld Declaration Identity Source) (type : UnionTail Declaration)
      (candidate : Value Declaration Identity Source) : Prop :=
    match type with
    | .variable id => nomatch id
    | .membersEmpty => False
    | .member type rest => value world type candidate ∨ members world rest candidate
end

theorem nonempty_is_list (world : MembershipWorld Declaration Identity Source)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    value world (.nonEmpty type) candidate → value world (.list type) candidate := by
  simp only [value]
  split <;> simp_all

end Membership

end Xsh.Core
