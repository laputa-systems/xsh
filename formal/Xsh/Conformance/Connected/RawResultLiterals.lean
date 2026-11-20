import Xsh.Conformance.Connected.RawResult

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultSourceNames : List String :=
  ["Int", "Bool", "Str", "Result", "pure", "through", "r", "return", "Ok", "refuse", "Err"]

def resultRows : RawInstructionRows :=
  ⟨[⟨"ExprParam", 31⟩, ⟨"ExprTry", 107⟩, ⟨"ExprOk", 104⟩, ⟨"StmtReturn", 150⟩, ⟨"ExprStr", 24⟩, ⟨"ExprErr", 105⟩, ⟨"StmtReturn", 150⟩],
    [false, false, false, false, false, false, false],
    [false, false, false, false, false, false, false],
    [⟨0, 1⟩, ⟨1, 2⟩, ⟨3, 1⟩, ⟨4, 1⟩, ⟨7, 1⟩, ⟨8, 2⟩, ⟨10, 1⟩],
    [0, 0, 2, 1, 2, 1, 3, 6, 4, 0, 5, 1, 6],
    [], [],
    [⟨0, 0⟩, ⟨66, 2⟩, ⟨0, 71⟩, ⟨72, 54⟩], [0, 0, 0, 0],
    [⟨0, 7⟩, ⟨7, 16⟩, ⟨23, 1⟩, ⟨24, 4⟩, ⟨28, 6⟩, ⟨34, 2⟩], [116, 104, 114, 111, 117, 103, 104, 82, 101, 115, 117, 108, 116, 91, 73, 110, 116, 44, 32, 83, 116, 114, 93, 114, 97, 114, 103, 115, 114, 101, 102, 117, 115, 101, 110, 111], []⟩

def resultFunctions : RawFunctionRows :=
  ⟨[⟨⟨5, 2⟩, 4294967295, 1, 3, (0, 0, 0)⟩, ⟨⟨11, 2⟩, 4294967295, 2, 3, (0, 0, 0)⟩],
    [⟨1, .ground 1, ⟨0, 1⟩, ⟨0, 1⟩, 1, 2, ⟨0, 2⟩, 3⟩, ⟨5, .ground 2, ⟨1, 0⟩, ⟨1, 1⟩, 2, 1, ⟨2, 1⟩, 3⟩]⟩

def resultParameters : List RawParameter := [⟨3, 0, (0, 0, 0)⟩]

def resultSignatures : List ResultSignature :=
  [⟨"through", ⟨0, 54, 17⟩, [("r", .result (.primitive .integer) (.primitive .string))], .result (.primitive .integer) (.primitive .string),
    [(⟨0, 63, 6⟩, .result (.primitive .integer) (.primitive .string)), (⟨0, 66, 2⟩, .primitive .integer), (⟨0, 66, 1⟩, .result (.primitive .integer) (.primitive .string))], []⟩, ⟨"refuse", ⟨0, 106, 20⟩, [], .result (.primitive .integer) (.primitive .string),
    [(⟨0, 115, 9⟩, .result (.primitive .integer) (.primitive .string)), (⟨0, 119, 4⟩, .primitive .string)], []⟩]

end Xsh.Conformance.Connected
