import Xsh.Proofs.Core.Callables

namespace Xsh.Proofs.Core.Constructors

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

variable {Declaration Identity Source : Type}

section Root

variable [DecidableEq Declaration]

/-- Record and error constructors from `type` and `error` declarations
(SPEC 4.7, 4.10). The declarations are top-level only (SPEC 3.1): run in a
block or a callable body they are refused, and at the top level they bind no
runtime name, since their constructors come from the program's declarations,
which the runtime makes visible throughout. A record constructor evaluates its
arguments once in written order, places positional ones in declaration order
and named ones by label ("positional arguments come first and fill fields in
declaration order"), fills a missing field from its default, and builds the
record in declaration order, nominal under the checker's declaration identity
when the schema is. An error constructor `Family.Variant(...)` builds an
`Error` with the family's declaration identity, the variant, its facets and
payload: a variant without a payload carries only its message, which is its
one optional positional argument or `Family.Variant`; a variant with a payload
places its arguments like a record constructor, each field's type the expected
type of the argument that fills it, and its `.message` "is its
`message: Str` field when it declares one and its family and variant name
otherwise". The checker's declaration identity is an oracle. -/
theorem declared_constructors {world : World Declaration Identity Source} (machine : Machine world) :
    (∀ fuel (frame : Frame world) name parameters body nominal bounds only,
      frame.callerStore = none → frame.environment.names = [only] →
        runStatement machine (fuel + 1) frame (.typeDef name parameters body nominal bounds) = .normal frame) ∧
    (∀ fuel (frame : Frame world) name variants only,
      frame.callerStore = none → frame.environment.names = [only] →
        runStatement machine (fuel + 1) frame (.errorDef name variants) = .normal frame) ∧
    (∀ fuel (frame : Frame world) name variants caller,
      frame.callerStore = some caller → runStatement machine (fuel + 1) frame (.errorDef name variants) = .stuck) ∧
    (∀ fuel (frame : Frame world) name parameters body nominal bounds caller,
      frame.callerStore = some caller →
        runStatement machine (fuel + 1) frame (.typeDef name parameters body nominal bounds) = .stuck) ∧
    (∀ fuel (frame : Frame world) receiver method family variant facets,
      errorConstructor machine frame receiver method = some (family, .mk variant [] facets) →
          evalExpr machine (fuel + 1) frame (.call (.field receiver method) []) =
            .value (declaredError machine family variant facets (qualifiedName machine family variant)
              [("message", .scalar (.string (qualifiedName machine family variant)))]) frame) ∧
    (∀ fuel (frame : Frame world) receiver method family variant facets message,
      errorConstructor machine frame receiver method = some (family, .mk variant [] facets) →
          evalExpr machine (fuel + 1) frame (.call (.field receiver method) [.positional message]) =
            (evalExpr machine fuel frame message).bind fun written after =>
              match written.eraseNominal with
              | .scalar (.string text) =>
                .value (declaredError machine family variant facets text [("message", .scalar (.string text))]) after
              | _ => .exec .stuck) ∧
    (∀ fuel (frame : Frame world) receiver method arguments family variant first rest facets,
      errorConstructor machine frame receiver method = some (family, .mk variant (first :: rest) facets) →
          evalExpr machine (fuel + 1) frame (.call (.field receiver method) arguments) =
            (threadArguments machine.context.spelling (evalExpr machine fuel) frame
                (expectArguments machine.context.spelling machine.declarations (payloadParameters (first :: rest))
                  (payloadParameters (first :: rest)) arguments)).bind fun supplied after =>
              match placeArguments machine.context.spelling ((first :: rest).map Prod.fst) supplied with
              | none => .exec .stuck
              | some placed =>
                (fillFields machine.context.spelling (evalExpr machine fuel) after
                    ((first :: rest).map fun field => (field.1, none)) placed).bind fun payload last =>
                  .value (declaredError machine family variant facets
                    (payloadMessage payload (qualifiedName machine family variant)) payload) last) ∧
    (∀ (payload : List (String × Value Declaration Identity Source)) text otherwise,
      payload.lookup "message" = some (.scalar (.string text)) → payloadMessage payload otherwise = text) ∧
    (∀ (payload : List (String × Value Declaration Identity Source)) otherwise,
      payload.lookup "message" = none → payloadMessage payload otherwise = otherwise) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · intro fuel frame name parameters body nominal bounds only calling top
    simp [runStatement, calling, top]
  · intro fuel frame name variants only calling top
    simp [runStatement, calling, top]
  · intro fuel frame name variants caller calling
    simp [runStatement, calling]
  · intro fuel frame name parameters body nominal bounds caller calling
    simp [runStatement, calling]
  · intro fuel frame receiver method family variant facets named
    simp only [evalExpr, named]
  · intro fuel frame receiver method family variant facets message named
    simp only [evalExpr, named]
    rfl
  · intro fuel frame receiver method arguments family variant first rest facets named
    cases arguments with
    | nil => simp only [evalExpr, named]; rfl
    | cons argument others =>
      cases argument <;> (simp only [evalExpr, named]; rfl)
  · intro payload text otherwise found
    simp [payloadMessage, found]
  · intro payload otherwise absent
    simp [payloadMessage, absent]

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Threading.Witness
open Xsh.Proofs.Core.Callables.Witness

def pointName : Xsh.Syntax.RawName := name 110 (by decide)
def xField : Xsh.Syntax.RawName := name 111 (by decide)
def yField : Xsh.Syntax.RawName := name 112 (by decide)
def pairName : Xsh.Syntax.RawName := name 113 (by decide)
def leftField : Xsh.Syntax.RawName := name 114 (by decide)
def rightField : Xsh.Syntax.RawName := name 115 (by decide)
def labelledName : Xsh.Syntax.RawName := name 116 (by decide)
def titleField : Xsh.Syntax.RawName := name 117 (by decide)
def sizeField : Xsh.Syntax.RawName := name 118 (by decide)
def fetchName : Xsh.Syntax.RawName := name 119 (by decide)
def usageName : Xsh.Syntax.RawName := name 120 (by decide)
def missingName : Xsh.Syntax.RawName := name 121 (by decide)
def badName : Xsh.Syntax.RawName := name 122 (by decide)
def whereField : Xsh.Syntax.RawName := name 123 (by decide)
def messageField : Xsh.Syntax.RawName := name 124 (by decide)
def codeField : Xsh.Syntax.RawName := name 125 (by decide)
def notFoundName : Xsh.Syntax.RawName := name 126 (by decide)
def tokenName : Xsh.Syntax.RawName := name 127 (by decide)
def uName : Xsh.Syntax.RawName := name 128 (by decide)
def mName : Xsh.Syntax.RawName := name 129 (by decide)
def bName' : Xsh.Syntax.RawName := name 130 (by decide)
def cName : Xsh.Syntax.RawName := name 131 (by decide)

def nameOf (index : Nat) : Xsh.Syntax.RawName :=
  [name 135 (by decide), name 136 (by decide), name 137 (by decide), name 138 (by decide)].getD index
    (name 139 (by decide))

def spellings : List (Xsh.Syntax.RawName × String) :=
  [(pointName, "Point"), (xField, "x"), (yField, "y"), (pairName, "Pair"), (leftField, "left"),
    (rightField, "right"), (labelledName, "Labelled"), (titleField, "title"), (sizeField, "size"),
    (fetchName, "Fetch"), (usageName, "Usage"), (missingName, "Missing"), (badName, "Bad"), (whereField, "where"),
    (messageField, "message"), (codeField, "code"), (notFoundName, "NotFound"), (tokenName, "Token")]

def constructorProgram : Context Unit Unit Unit :=
  { callableProgram with
    spelling := fun named => (spellings.lookup named).getD (callableProgram.spelling named) }

def anyType : Xsh.Syntax.TypeExpr := .named (name 140 (by decide))

/-- `type Point = { x: Int, y: Int = 7 }`. -/
def declarePoint : Xsh.Syntax.Statement :=
  .typeDef pointName [] (.recordSchema [.mk xField anyType none, .mk yField anyType (some (int "7"))]) false none

/-- `type Pair = { left: Int, right: Int }`. -/
def declarePair : Xsh.Syntax.Statement :=
  .typeDef pairName [] (.recordSchema [.mk leftField anyType none, .mk rightField anyType none]) false none

/-- `type Labelled = { title: Str, size: Int }`. -/
def declareLabelled : Xsh.Syntax.Statement :=
  .typeDef labelledName [] (.recordSchema [.mk titleField anyType none, .mk sizeField anyType none]) false none

/-- A nominal `type Token = { x: Int }`. -/
def declareToken : Xsh.Syntax.Statement :=
  .typeDef tokenName [] (.recordSchema [.mk xField anyType none]) true none

/-- `error Fetch = Usage | Missing(where: Str) : NotFound | Bad(message: Str, code: Int)`. -/
def declareFetch : Xsh.Syntax.Statement :=
  .errorDef fetchName [.mk usageName [] [], .mk missingName [(whereField, anyType)] [notFoundName],
    .mk badName [(messageField, anyType), (codeField, anyType)] []]

def constructorMachine : Machine witnessWorld :=
  { callingMachine tallyName with
    context := constructorProgram
    declarations := [declarePoint, declarePair, declareLabelled, declareToken, declareFetch] }

def runConstructors (body : List Xsh.Syntax.Statement) : Exec witnessWorld :=
  runStatements constructorMachine 90 fullStart body

def valueOf (target : Xsh.Syntax.RawName) : Exec witnessWorld → Option (Value Unit Unit Unit)
  | .normal frame =>
    match frame.environment.cellOf target with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

def intOf : Value Unit Unit Unit → Option Int
  | .scalar (.integer value) => some value.val
  | _ => none

def textOf : Value Unit Unit Unit → Option String
  | .scalar (.string text) => some text
  | _ => none

/-- A record's labels and integer fields, in its field order. -/
def recordInts : Value Unit Unit Unit → Option (List (String × Int))
  | .record fields => fields.mapM fun entry => (intOf entry.2).map fun value => (entry.1, value)
  | _ => none

def namedArguments (pairs : List (Xsh.Syntax.RawName × Xsh.Syntax.Expr)) : List Xsh.Syntax.CallArgument :=
  pairs.map fun pair => .named pair.1 pair.2

/-- `let a = Point(x: 1)`, run before `type Point` in the script: the
constructor fills `y` from its default and is visible before the
declaration, as the verification binary prints. -/
def pointRun : Exec witnessWorld :=
  runConstructors [letOf rName (.call (ident pointName) (namedArguments [(xField, int "1")])), declarePoint]

theorem record_default_and_hoisting :
    ((valueOf rName pointRun).bind recordInts) = some [("x", 1), ("y", 7)] := by
  decide +kernel

/-- `var count = 0; proc bump() ...; let p = Pair(right: bump(), left: bump())`:
the arguments run in written order, so `right` is `1` and `left` is `2`, and
the record is in declaration order (the binary prints `2 1`). -/
def pairCall : Xsh.Syntax.Expr :=
  .call (ident pairName) (namedArguments [(rightField, call tallyName []), (leftField, call tallyName [])])

def pairRun : Exec witnessWorld :=
  runConstructors [varOf countName "0", declareCounter, declarePair, letOf rName pairCall]

theorem record_arguments_in_written_order :
    ((valueOf rName pairRun).bind recordInts) = some [("left", 2), ("right", 1)] := by
  decide +kernel

/-- Evaluating named arguments in declaration order instead gives `left 1`,
`right 2`. -/
def declarationOrderPair : Exec witnessWorld :=
  runConstructors [varOf countName "0", declareCounter, declarePair,
    letOf rName (.call (ident pairName) (namedArguments [(leftField, call tallyName []), (rightField, call tallyName [])]))]

/-- Placing named arguments by their position instead of their label puts
`right: 1` in `left`. -/
def positionalNamed : Option (List (Option (Value Unit Unit Unit))) :=
  placeArguments constructorProgram.spelling [leftField, rightField]
    [.positional (integer intOne), .positional (integer ⟨2, by decide⟩)]

theorem declaration_order_rejected :
    ((valueOf rName declarationOrderPair).bind recordInts) = some [("left", 1), ("right", 2)] ∧
      (placeArguments constructorProgram.spelling [leftField, rightField]
          [.named "right" (integer intOne), .named "left" (integer ⟨2, by decide⟩)]).map
        (fun placed => placed.map fun entry => entry.bind intOf) = some [some 2, some 1] ∧
      (positionalNamed.map fun placed => placed.map fun entry => entry.bind intOf) = some [some 1, some 2] := by
  decide +kernel

/-- `Labelled("x", 2)` fills fields positionally in declaration order; a
nominal schema's constructor gives a nominal record. -/
def labelledRun : Exec witnessWorld :=
  runConstructors [declareLabelled, declareToken,
    letOf rName (.call (ident labelledName) [.positional (.literal (.string "x")), .positional (int "2")]),
    letOf vName (.call (ident tokenName) (namedArguments [(xField, int "1")]))]

def isNominalRecord : Value Unit Unit Unit → Bool
  | .nominal _ (.record _) => true
  | _ => false

theorem positional_and_nominal :
    ((valueOf rName labelledRun).map fun value => match value with
      | .record [("title", title), ("size", size)] => (textOf title, intOf size)
      | _ => (none, none)) = some (some "x", some 2) ∧
      ((valueOf vName labelledRun).map isNominalRecord) = some true := by
  decide +kernel

def errorMessage : Value Unit Unit Unit → Option String
  | .error data => some data.message
  | _ => none

def errorVariant : Value Unit Unit Unit → Option String
  | .error data => some data.variant
  | _ => none

def errorFacets : Value Unit Unit Unit → Option (List String)
  | .error data => some data.facets
  | _ => none

def payloadText (label : String) : Value Unit Unit Unit → Option String
  | .error data => (data.payload.lookup label).bind textOf
  | _ => none

def fetchCall (variant : Xsh.Syntax.RawName) (arguments : List Xsh.Syntax.CallArgument) : Xsh.Syntax.Expr :=
  .call (.field (ident fetchName) variant) arguments

def errorsRun : Exec witnessWorld :=
  runConstructors [
    letOf uName (fetchCall usageName []),
    letOf vName (fetchCall usageName [.positional (.literal (.string "custom"))]),
    letOf mName (fetchCall missingName (namedArguments [(whereField, .literal (.string "/x"))])),
    letOf bName' (fetchCall badName (namedArguments [(messageField, .literal (.string "boom")), (codeField, int "2")])),
    declareFetch]

/-- `Fetch.Usage()` has message `Fetch.Usage`, `Fetch.Usage("custom")` has
`custom`, `Fetch.Missing(where: "/x")` has `Fetch.Missing`, payload `where`
and facet `NotFound`, and `Fetch.Bad(message: "boom", code: 2)` has `boom`,
as the verification binary prints. -/
theorem error_constructors :
    (valueOf uName errorsRun).bind errorVariant = some "Usage" ∧
      (valueOf uName errorsRun).bind errorMessage = some "Fetch.Usage" ∧
      (valueOf uName errorsRun).bind (payloadText "message") = some "Fetch.Usage" ∧
      (valueOf vName errorsRun).bind errorMessage = some "custom" ∧
      (valueOf mName errorsRun).bind errorMessage = some "Fetch.Missing" ∧
      (valueOf mName errorsRun).bind errorFacets = some ["NotFound"] ∧
      (valueOf mName errorsRun).bind (payloadText "where") = some "/x" ∧
      (valueOf bName' errorsRun).bind errorMessage = some "boom" := by
  decide +kernel

/-- Matching the constructed `Missing` error binds its payload and its facet. -/
def errorMatchRunWithR : Exec witnessWorld :=
  runConstructors [
    letOf mName (fetchCall missingName (namedArguments [(whereField, .literal (.string "/x"))])),
    .binding .var (.name rName) none (.literal (.string "")), varOf seenName "0",
    .matchStmt (ident mName) [(.errorVariant fetchName missingName [(whereField, .binding cName)], none,
      [.assign (.name rName) .set (ident cName)])],
    .matchStmt (ident mName) [(.facet notFoundName, none, [set seenName "1"])]]

theorem error_patterns_match :
    (valueOf rName errorMatchRunWithR).bind textOf = some "/x" ∧ readInt seenName errorMatchRunWithR = some 1 := by
  decide +kernel

/-- An error constructor that takes the variant name alone as the default
message gives `Usage`, not `Fetch.Usage`. -/
def variantOnlyMessage : String := constructorProgram.spelling usageName

theorem variant_only_message_rejected :
    variantOnlyMessage = "Usage" ∧ qualifiedName constructorMachine fetchName usageName = "Fetch.Usage" := by
  decide +kernel

def baseLocal : Xsh.Syntax.RawName := name 132 (by decide)
def onlyLocal : Xsh.Syntax.RawName := name 133 (by decide)
def fieldsLocal : Xsh.Syntax.RawName := name 134 (by decide)

/-- `Point(...base)` with `base = {x: 2, y: 8}`, `Point(x:)` with `x = 1`,
`Point(...only)` with `only = {x: 8}` and `Fetch.Bad(...fields)` with
`fields = {message: "boom", code: 2}`; the verification binary prints
`2 8 1 7 7` and `boom`. A pun `x:` is the named argument `x: x`. -/
def spreadRun : Exec witnessWorld :=
  runConstructors [
    letOf baseLocal (.record [.named xField (int "2"), .named yField (int "8")]),
    varOf xField "1",
    letOf onlyLocal (.record [.named xField (int "8")]),
    letOf fieldsLocal (.record [.named messageField (.literal (.string "boom")), .named codeField (int "2")]),
    letOf (nameOf 0) (.call (ident pointName) [.namedSpread (ident baseLocal)]),
    letOf (nameOf 1) (.call (ident pointName) [.named xField (ident xField)]),
    letOf (nameOf 2) (.call (ident pointName) [.namedSpread (ident onlyLocal)]),
    letOf (nameOf 3) (fetchCall badName [.namedSpread (ident fieldsLocal)])]

theorem spread_and_pun_constructors :
    ((valueOf (nameOf 0) spreadRun).bind recordInts) = some [("x", 2), ("y", 8)] ∧
      ((valueOf (nameOf 1) spreadRun).bind recordInts) = some [("x", 1), ("y", 7)] ∧
      ((valueOf (nameOf 2) spreadRun).bind recordInts) = some [("x", 8), ("y", 7)] ∧
      (valueOf (nameOf 3) spreadRun).bind errorMessage = some "boom" := by
  decide +kernel

/-- A spread and a named argument for the same field is refused, as the
checker's `check.named-arg` does; a constructor that lets the later one win
would build `{x: 2, y: 1}`. -/
theorem spread_duplicate_refused :
    isStuck (runConstructors [letOf baseLocal (.record [.named xField (int "2"), .named yField (int "8")]),
      letOf rName (.call (ident pointName) [.namedSpread (ident baseLocal), .named yField (int "1")])]) = true ∧
      (placeArguments constructorProgram.spelling [xField, yField]
          ([.named "x" (integer ⟨2, by decide⟩), .named "y" (integer ⟨8, by decide⟩)] :
            List (Supplied Unit Unit Unit))).isSome = true := by
  decide +kernel

/-- A `type` declared in a block is refused. -/
theorem nested_type_refused :
    isStuck (runConstructors [.expression (.valueBlock [declarePoint])]) = true := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Constructors
