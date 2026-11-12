import Xsh.Core.MembershipDecision

namespace Xsh.Core.MembershipControls

open Xsh.Values

private abbrev TestValue := Value Nat Nat Unit
private abbrev TestType := ValueType Nat

private def integer : TestType := .primitive .integer
private def string : TestType := .primitive .string
private def unsigned : TestType := .primitive .unsignedInteger
private def one : TestValue := .scalar (.integer intOne)
private def negative : TestValue := .scalar (.integer intNegativeOne)
private def text : TestValue := .scalar (.string "item")
private def relative : NativePath := ⟨[97, 47, 98], by decide⟩
private def absolute : NativePath := ⟨[47, 97], by decide⟩
private def escaping : NativePath := ⟨[46, 46, 47, 97], by decide⟩
private def bytePath : NativePath := ⟨[255], by decide⟩
private def fields : RecordType Nat := .rowField "x" unsigned .rowEmpty
private def record : TestValue := .record [("x", one), ("extra", text)]
private def signature : SignatureType Nat :=
  .signature .pure (.parameter .required "x" unsigned .argumentsEmpty) unsigned

private def world : MembershipWorld Nat Nat Unit where
  entryFields _ := [("path", .scalar (.path relative)), ("name", text),
    ("ext", .scalar (.string "")), ("kind", .scalar (.string "file"))]
  stream reference type := reference.identity = 0 ∧ type = integer
  callable reference type := reference.identity = 0 ∧ type = signature
  scheme reference declaration := reference.identity = 1 ∧ declaration = 7
  generic reference kind := reference.identity = 1 ∧ kind = .procedure

private def contracts : ContractDecisions world where
  stream _ _ := by unfold world; infer_instance
  callable _ _ := by unfold world; infer_instance
  scheme _ _ := by unfold world; infer_instance
  generic _ _ := by unfold world; infer_instance

private def error : ErrorData Nat Nat Unit where
  family := 3
  variant := "Bad"
  kind := "bad"
  message := "failure"
  payload := [("x", one)]
  facets := ["timeout"]
  span := none
  contexts := []
  cause := none
  abort := none

private def processError : ProcessErrorData Nat Nat Unit where
  kind := "process"
  message := "failed"
  span := none
  status := some ⟨false, .exit, some intOne, []⟩
  contexts := []
  cause := some (.error error)

private def bounds : Xsh.Validation.BoundedSpec :=
  ⟨.uint, 0, 1, by decide,
    by dsimp [Xsh.Validation.IntegerBase.accepts]; decide,
    by dsimp [Xsh.Validation.IntegerBase.accepts]; decide⟩

private def cases : List (TestType × TestValue × Bool) := [
  (.primitive .any, .scalar .null, true),
  (.primitive .null, one, false),
  (.primitive .boolean, .scalar (.boolean true), true),
  (integer, one, true),
  (unsigned, negative, false),
  (.primitive .float, .scalar (.float ⟨0, by decide⟩), true),
  (.primitive .duration, .scalar (.duration ⟨1, by decide⟩), true),
  (string, text, true),
  (.primitive .bytes, .scalar (.bytes [0, 255]), true),
  (.primitive .digest, .scalar (.digest ⟨"sha256", List.replicate 32 0⟩), true),
  (.primitive .regex, .scalar (.regex "a"), true),
  (.primitive .path, .scalar (.path bytePath), true),
  (.primitive .erasedRecord, .filesystemEntry ⟨0⟩, true),
  (.primitive .dynamicModule, .module ⟨0⟩ [], true),
  (.primitive .status, .status ⟨true, .exit, some intZero, []⟩, true),
  (.primitive .environmentPaths, .environmentPaths [relative], true),
  (.primitive .error, .processError processError, true),
  (.primitive .processError, .error error, false),
  (.primitive .command, .command ⟨0⟩, true),
  (.primitive .processHandle, .processHandle ⟨0⟩, true),
  (.primitive .networkJob, .networkJob ⟨0⟩, true),
  (.primitive .filesystemRoot, .filesystemRoot ⟨0⟩, true),
  (.primitive .filesystemLock, .filesystemLock ⟨0⟩, true),
  (.primitive .sshSession, .sshSession ⟨0⟩, true),
  (.primitive .unit, .scalar .unit, true),
  (.list integer, .list [one, text], false),
  (.list (.primitive .path), .environmentPaths [relative], true),
  (.map string unsigned, .map [(.string "x", one)], true),
  (.map string unsigned, .map [(.boolean true, one)], false),
  (.map string unsigned, .record [("x", one)], true),
  (.map (.primitive .any) unsigned, .record [("x", one)], false),
  (.map string (.primitive .any), .filesystemEntry ⟨0⟩, false),
  (.set unsigned, .set [.integer intNegativeOne], false),
  (.set (.optional integer), .set [.integer intOne], false),
  (.stream integer, .stream ⟨0⟩, true),
  (.stream integer, .stream ⟨1⟩, false),
  (.record fields, record, true),
  (.record fields, .record [], false),
  (.record fields, .record [("x", negative)], false),
  (.record (.rowField "path" (.primitive .path) .rowEmpty), .filesystemEntry ⟨0⟩, true),
  (.module .open (.exportValue "x" .required unsigned .exportsEmpty), .module ⟨0⟩ [("x", one), ("extra", text)], true),
  (.module .exact (.exportValue "x" .required unsigned .exportsEmpty), .module ⟨0⟩ [("x", one), ("extra", text)], false),
  (.module .exact (.exportValue "x" .optional unsigned .exportsEmpty), .module ⟨0⟩ [], true),
  (.module .open (.exportValue "x" .required unsigned .exportsEmpty), .module ⟨0⟩ [], false),
  (.module .exact (.exportCallable "f" .required signature .exportsEmpty), .module ⟨0⟩ [("f", .callable ⟨0⟩ .pure)], true),
  (.module .exact (.exportGenericCallable "f" .required .procedure .exportsEmpty), .module ⟨0⟩ [("f", .callable ⟨1⟩ .procedure)], true),
  (.module .exact (.exportGenericCallable "f" .required .procedure .exportsEmpty), .module ⟨0⟩ [("f", .callable ⟨1⟩ .pure)], false),
  (.result integer (.primitive .error), .ok one, true),
  (.result integer (.primitive .error), .err text, false),
  (.optional integer, .scalar .null, true),
  (.optional integer, one, true),
  (.union integer string .membersEmpty, .scalar (.boolean true), false),
  (.union integer string (.member (.primitive .path) .membersEmpty), .scalar (.path bytePath), true),
  (.functionClass .procedure, .callable ⟨0⟩ .producer, false),
  (.callable signature, .callable ⟨0⟩ .pure, true),
  (.callable signature, .callable ⟨0⟩ .procedure, false),
  (.schemeCallable 7, .callable ⟨1⟩ .procedure, true),
  (.schemeCallable 7, .callable ⟨0⟩ .procedure, false),
  (.tagged 4, .tagged 4 "First" [one], true),
  (.tagged 4, .tagged 5 "First" [one], false),
  (.errorFamily 3, .error error, true),
  (.errorFamily 4, .error error, false),
  (.errorVariant 3 "Bad", .error error, true),
  (.errorVariant 3 "Other", .error error, false),
  (.errorFacet "timeout", .error error, true),
  (.errorFacet "missing", .error error, false),
  (.nonEmpty integer, .list [], false),
  (.nonEmpty integer, .list [one], true),
  (.nonEmpty (.primitive .path), .environmentPaths [], false),
  (.nonEmpty (.primitive .path), .environmentPaths [relative], true),
  (.relativePath, .scalar (.path relative), true),
  (.relativePath, .scalar (.path absolute), false),
  (.relativePath, .scalar (.path escaping), false),
  (.relativePath, .scalar (.path bytePath), true),
  (.nominal 7 fields, .nominal 7 record, true),
  (.nominal 7 fields, .nominal 8 record, false),
  (.nominal 7 fields, record, false),
  (.boundedInteger bounds, one, true),
  (.boundedInteger bounds, negative, false)
]

theorem decisions_match_controls :
    cases.all (fun (type, candidate, expected) => Membership.check world contracts type candidate == expected) = true := by
  simp only [cases, List.all_cons, List.all_nil, Bool.and_eq_true, beq_iff_eq]
  repeat' apply And.intro
  all_goals simp only [Bool.eq_false_iff, Ne, Membership.check_iff]
  all_goals simp [Membership.value, Membership.row, Membership.exports,
    Membership.members, Membership.primitive, Membership.key, Membership.callable,
    Membership.generic, Membership.signatureKind, Membership.recordFields,
    Membership.exportNames, Membership.absentAllowed, world, signature, fields, record,
    integer, unsigned, string, one, negative, text, error, processError, bounds,
    relative, absolute, escaping, bytePath, Value.eraseNominal, MapKey.scalar, InvocationPolicy.kind,
    Xsh.Validation.RelPath.isRelPath, Xsh.Validation.RelPath.isPathBytes,
    Xsh.Validation.RelPath.scanPath, Xsh.Validation.RelPath.slashByte,
    Xsh.Validation.RelPath.nulByte,
    Xsh.Validation.BoundedSpec.accepts, Xsh.Validation.IntegerBase.accepts,
    intOne, intZero, intNegativeOne]
  all_goals decide

end Xsh.Core.MembershipControls
