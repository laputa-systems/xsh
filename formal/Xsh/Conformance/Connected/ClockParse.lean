import Xsh.Conformance.Connected.ClockTokens

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Foundation

def clockName (id : Nat) (bound : id < 4294967296) : NameId := ⟨⟨id, bound⟩⟩
def clockExpression : Expr :=
  .call (.field (.ident (clockName 6 (by decide))) (clockName 7 (by decide))) []
def clockFunction : SourceFunction :=
  ⟨.proc, some [.time], clockName 5 (by decide), ⟨0, 5, 5⟩, ⟨0, 27, 14⟩, [],
    some (.named (clockName 0 (by decide))), [.expression clockExpression],
    [(⟨0, 29, 10⟩, clockExpression),
     (⟨0, 29, 8⟩, .field (.ident (clockName 6 (by decide))) (clockName 7 (by decide))),
     (⟨0, 29, 4⟩, .ident (clockName 6 (by decide)))],
    [(⟨0, 29, 10⟩, .expression clockExpression)]⟩
def clockUnit : SourceUnit :=
  ⟨0, ["Int", "Bool", "Str", "Result", "proc", "clock", "time", "now"], [.function clockFunction]⟩

theorem clock_tokens_parse :
    parseDeclarations (32 * (clockTokens.length + 1)) clockNames clockTokens = .ok [.function clockFunction] := by
  rfl

def derivedClockUnit : Option DerivedDeclarationEffect :=
  match deriveDeclarationEffect 32 clockUnit 0 with | .ok value => some value | .error _ => none

theorem clock_unit_derives_time : derivedClockUnit = some clockDerivedEffect := by
  decide +kernel

end Xsh.Conformance.Connected
