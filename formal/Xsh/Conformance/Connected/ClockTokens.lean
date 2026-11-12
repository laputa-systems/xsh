import Xsh.Conformance.Connected.SourceEffectModel

namespace Xsh.Conformance.Connected

open Xsh.Syntax

def clockSource : String := "proc clock() [time] -> Int { time.now() }\n"
def clockDerivedEffect : DerivedDeclarationEffect := ⟨clockEffectIdentity, [.time], [.time], false, true⟩

def clockTokens : List SourceToken :=
  [sourceToken 0 0 4 (.word "proc"), sourceToken 0 5 5 (.word "clock"),
   sourceToken 0 10 1 (.symbol "("), sourceToken 0 11 1 (.symbol ")"),
   sourceToken 0 13 1 (.symbol "["), sourceToken 0 14 4 (.word "time"),
   sourceToken 0 18 1 (.symbol "]"), sourceToken 0 20 2 (.symbol "->"),
   sourceToken 0 23 3 (.word "Int"), sourceToken 0 27 1 (.symbol "{"),
   sourceToken 0 29 4 (.word "time"), sourceToken 0 33 1 (.symbol "."),
   sourceToken 0 34 3 (.word "now"), sourceToken 0 37 1 (.symbol "("),
   sourceToken 0 38 1 (.symbol ")"), sourceToken 0 40 1 (.symbol "}"),
   sourceToken 0 41 1 .newline]

theorem clock_source_tokenizes : tokenizeText 0 clockSource = .ok clockTokens := by
  rfl

def clockNames : List String := ["Int", "Bool", "Str", "Result", "proc", "clock", "time", "now"]

theorem clock_token_names : sourceNames clockTokens = clockNames := by
  decide +kernel

end Xsh.Conformance.Connected
