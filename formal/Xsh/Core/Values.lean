import Xsh.Core.Types

namespace Xsh.Core

open Xsh.Values

inductive ReferenceKind where
  | callable
  | module
  | stream
  | command
  | process
  | networkJob
  | filesystemRoot
  | filesystemLock
  | filesystemEntry
  | sshSession
  deriving DecidableEq

/-- References identify semantic objects. Their interpretation belongs to the
semantic world; tokens are not Rust addresses, slots, or allocation counters. -/
structure Reference (Identity : Type) (kind : ReferenceKind) where
  identity : Identity
  deriving DecidableEq

structure SourceSpan (Source : Type) where
  source : Source
  start : Nat
  stop : Nat
  ordered : start ≤ stop
  deriving DecidableEq

structure ErrorContext (Source : Type) where
  kind : String
  message : Option String
  span : Option (SourceSpan Source)
  deriving DecidableEq

inductive CompletionKind where
  | exit
  | signal
  | exec
  deriving DecidableEq

structure SegmentStatus (Identity : Type) where
  index : Nat
  target : Bytes
  process : Option (Reference Identity .process)
  success : Bool
  kind : CompletionKind
  code : Option IntValue
  errorKind : Option String
  errorMessage : Option String
  deriving DecidableEq

structure ProcessStatus (Identity : Type) where
  success : Bool
  kind : CompletionKind
  code : Option IntValue
  segments : List (SegmentStatus Identity)
  deriving DecidableEq

structure AbortSignal where
  status : UInt8
  force : Bool
  deriving DecidableEq

structure Digest where
  algorithm : String
  bytes : Bytes
  deriving DecidableEq

inductive Scalar where
  | null
  | boolean (value : Bool)
  | integer (value : IntValue)
  | float (bits : FloatBits)
  | duration (value : DurationValue)
  | string (value : String)
  | bytes (value : Bytes)
  | digest (value : Digest)
  | regex (pattern : String)
  | path (value : NativePath)
  | unit
  deriving DecidableEq

inductive MapKey where
  | string (value : String)
  | integer (value : IntValue)
  | boolean (value : Bool)
  | bytes (value : Bytes)
  | path (value : NativePath)
  | duration (value : DurationValue)
  deriving DecidableEq

def MapKey.scalar : MapKey → Scalar
  | .string value => .string value
  | .integer value => .integer value
  | .boolean value => .boolean value
  | .bytes value => .bytes value
  | .path value => .path value
  | .duration value => .duration value

mutual
  /-- The shared data domain retains semantic identities and all value families.
  It admits raw candidates; uniqueness, ordering, nominal provenance, reference
  validity and retained callable/stream contracts need their own judgments.
  This definition is not a claim of Rust conformance or full-core safety. -/
  inductive Value (Declaration Identity Source : Type) where
    | scalar (value : Scalar)
    | list (values : List (Value Declaration Identity Source))
    | map (entries : List (MapKey × Value Declaration Identity Source))
    | set (keys : List MapKey)
    | stream (reference : Reference Identity .stream)
    | record (fields : List (String × Value Declaration Identity Source))
    | filesystemEntry (reference : Reference Identity .filesystemEntry)
    | module (reference : Reference Identity .module)
        (exports : List (String × Value Declaration Identity Source))
    | ok (value : Value Declaration Identity Source)
    | err (value : Value Declaration Identity Source)
    | status (value : ProcessStatus Identity)
    | environmentPaths (paths : List NativePath)
    | error (value : ErrorData Declaration Identity Source)
    | processError (value : ProcessErrorData Declaration Identity Source)
    | callable (reference : Reference Identity .callable) (kind : CallableKind)
    | command (reference : Reference Identity .command)
    | processHandle (reference : Reference Identity .process)
    | networkJob (reference : Reference Identity .networkJob)
    | filesystemRoot (reference : Reference Identity .filesystemRoot)
    | filesystemLock (reference : Reference Identity .filesystemLock)
    | sshSession (reference : Reference Identity .sshSession)
    | tagged (declaration : Declaration) (variant : String)
        (fields : List (Value Declaration Identity Source))
    | nominal (declaration : Declaration) (value : Value Declaration Identity Source)

  structure ErrorData (Declaration Identity Source : Type) where
    family : Declaration
    variant : String
    kind : String
    message : String
    payload : List (String × Value Declaration Identity Source)
    facets : List String
    span : Option (SourceSpan Source)
    contexts : List (ErrorContext Source)
    cause : Option (Value Declaration Identity Source)
    abort : Option AbortSignal

  structure ProcessErrorData (Declaration Identity Source : Type) where
    kind : String
    message : String
    span : Option (SourceSpan Source)
    status : Option (ProcessStatus Identity)
    contexts : List (ErrorContext Source)
    cause : Option (Value Declaration Identity Source)
end

theorem MapKey.scalar_injective : Function.Injective MapKey.scalar := by
  intro left right same
  cases left <;> cases right <;> simp_all [MapKey.scalar]

def Value.eraseNominal {Declaration Identity Source : Type} :
    Value Declaration Identity Source → Value Declaration Identity Source
  | .nominal _ value => value.eraseNominal
  | value => value

theorem Value.eraseNominal_idempotent {Declaration Identity Source : Type}
    (value : Value Declaration Identity Source) :
    value.eraseNominal.eraseNominal = value.eraseNominal := by
  cases value with
  | nominal _ value => exact eraseNominal_idempotent value
  | _ => rfl
termination_by value

end Xsh.Core
