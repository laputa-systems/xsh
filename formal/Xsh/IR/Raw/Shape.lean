namespace Xsh.IR.Raw

inductive Role where
  | raw
  | symbol
  | slot
  | stringId
  | bytesId
  | location
  | typeId
  | loweredType
  | expr
  | stmt
  | int
  | bool
  | pattern
  | stage
  | value
  | function
  | regex
  | constant
  | modulePlan
  | wireEnum
  | schema
  | runtimeOp
  | assignOp
  | binaryOp
  | runKind
  | redirectionKind
  | bindingIdentity
  | childProgram
  | deferKind
  | endDistance
  | materialization
  | schemaExpectation
  | textHole
  deriving DecidableEq, Repr

inductive Shape where
  | word (role : Role)
  | seq (items : List Shape)
  | choice (tags : List Nat) (variants : List Shape)
  | many (item : Shape)
  | block (statements : Bool) (item : Shape)
  | recur (name : String)
  | textPattern
  deriving Repr

inductive RowKind where
  | int
  | bool
  | expr
  | stmt
  | pattern
  | stage
  | value
  | driver
  deriving DecidableEq, Repr

structure TagSchema where
  name : String
  ordinal : Nat
  kind : RowKind
  fields : List Shape
  deriving Repr

structure RecursiveShape where
  name : String
  tags : List Nat
  variants : List Shape
  deriving Repr

/-- The tag tables, recursive shapes and closed domains that a raw-IR document is checked against. -/
structure Schema where
  instructions : List TagSchema
  patterns : List TagSchema
  stages : List TagSchema
  values : List TagSchema
  drivers : List TagSchema
  recursive : List RecursiveShape
  assignOps : Nat
  binaryOps : Nat
  runKinds : Nat
  redirectionKinds : Nat
  deferKinds : Nat
  textHoleKinds : Nat
  callableKinds : Nat
  resourceTransfers : List Nat

/-- A table entry that a payload, a function body or a driver program takes as its own. -/
inductive Claim where
  | instruction (index : Nat)
  | block (index : Nat)
  | pattern (index : Nat)
  | stage (index : Nat)
  | program (index : Nat)
  | step (index : Nat)
  deriving DecidableEq, Repr

end Xsh.IR.Raw
