import Xsh.Validation.RelPath

namespace Xsh.Validation.Core.Paths

open Xsh.Validation.RelPath

abbrev Component := List UInt8

def components (bytes : PathBytes) : List Component := bytes.splitOn slashByte

def join (parts : List Component) : PathBytes := [slashByte].intercalate parts

def isTrivial (component : Component) : Bool := component = [] || component = [dotByte]

def readComponents (parts : List Component) : List Component := parts.filter fun part => !isTrivial part

def parentComponents (parts : List Component) : List Component := (readComponents parts).dropLast

def parent (bytes : PathBytes) : PathBytes :=
  match parentComponents (components bytes) with
  | [] => [dotByte]
  | parts => join parts

def normalizeStack : List Component → List Component → List Component
  | stack, [] => stack
  | stack, component :: rest =>
    if isTrivial component then normalizeStack stack rest
    else if component = [dotByte, dotByte] then normalizeStack stack.tail rest
    else normalizeStack (component :: stack) rest

def normalize (bytes : PathBytes) : PathBytes :=
  match (normalizeStack [] (components bytes)).reverse with
  | [] => [dotByte]
  | parts => join parts

end Xsh.Validation.Core.Paths
