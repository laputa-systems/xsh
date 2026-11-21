import Xsh.Conformance.Transport.Json
import Xsh.Conformance.Transport.Codecs

namespace Xsh.IR.Raw

open Xsh.Foundation
open Xsh.Conformance.Transport

def documentSchema : String := "xsh-raw-ir-v1"

def documentScope : String := "indexed-store-structure-v1"

abbrev Row := List Word32

abbrev TagWord := String × Word32

structure Document where
  symbolBase : Word32
  symbols : List String
  sourceId : Word32
  sourceCount : Word32
  tags : List TagWord
  data : List Row
  resourceTransfers : List Word32
  checkedCallTargetRequired : List Word32
  extra : List Word32
  patterns : List TagWord
  patternData : List Row
  stages : List TagWord
  stageData : List Row
  values : List TagWord
  valueData : List Row
  blocks : List Row
  driverStepTags : List TagWord
  driverSteps : List Row
  driverSlots : List Row
  driverWriters : List Row
  driverSync : List Row
  driverRegions : List Row
  driverPrograms : List Row
  bindingIdentities : List Row
  driverRoot : Word32
  functions : List Row
  functionInstructionStarts : List Word32
  functionMetadata : List Row
  params : List Row
  paramCold : List Row
  captures : List Row
  validations : List Row
  slotTypeFacts : List Word32
  strings : List Row
  stringBytes : List WireByte
  bytes : List Row
  byteData : List WireByte
  locations : List Row
  locationSources : List Word32
  runtimeOps : List Word32
  assignOps : List Word32
  binaryOps : List Word32
  runKinds : List Word32
  redirectionKinds : List Word32
  poolCounts : List Word32
  deriving Repr

def encodeWord (word : Word32) : WireValue := .string (encodeHex32 word)

def decodeWord? : WireValue → Option Word32
  | .string text => decodeHex32? text
  | _ => none

def encodeWords (words : List Word32) : WireValue := .array (words.map encodeWord)

def decodeWords? : WireValue → Option (List Word32)
  | .array items => items.mapM decodeWord?
  | _ => none

def decodeRow? (arity : Nat) : WireValue → Option Row
  | .array items => if items.length = arity then items.mapM decodeWord? else none
  | _ => none

def encodeRows (rows : List Row) : WireValue := .array (rows.map encodeWords)

def decodeRows? (arity : Nat) : WireValue → Option (List Row)
  | .array rows => rows.mapM (decodeRow? arity)
  | _ => none

def encodeTag (tag : TagWord) : WireValue := .object [("name", .string tag.1), ("ordinal", encodeWord tag.2)]

def decodeTag? : WireValue → Option TagWord
  | .object [("name", .string name), ("ordinal", ordinal)] => (decodeWord? ordinal).map (name, ·)
  | _ => none

def encodeTags (tags : List TagWord) : WireValue := .array (tags.map encodeTag)

def decodeTags? : WireValue → Option (List TagWord)
  | .array items => items.mapM decodeTag?
  | _ => none

def encodeTexts (texts : List String) : WireValue := .array (texts.map .string)

def decodeTexts? : WireValue → Option (List String)
  | .array items => items.mapM WireValue.text?
  | _ => none

def encodeBytes (bytes : List WireByte) : WireValue := .string (encodeHexBytes bytes)

def decodeBytes? : WireValue → Option (List WireByte)
  | .string text => decodeHexBytes? text
  | _ => none

def expectText? (expected : String) : WireValue → Option Unit
  | .string text => if text = expected then some () else none
  | _ => none

def documentFields : List String :=
  ["schema", "scope", "symbolBase", "symbols", "sourceId", "sourceCount",
   "tags", "data", "resourceTransfers", "checkedCallTargetRequired", "extra", "patterns",
   "patternData", "stages", "stageData", "values", "valueData", "blocks",
   "driverStepTags", "driverSteps", "driverSlots", "driverWriters", "driverSync", "driverRegions",
   "driverPrograms", "bindingIdentities", "driverRoot", "functions", "functionInstructionStarts", "functionMetadata",
   "params", "paramCold", "captures", "validations", "slotTypeFacts", "strings",
   "stringBytes", "bytes", "byteData", "locations", "locationSources", "runtimeOps",
   "assignOps", "binaryOps", "runKinds", "redirectionKinds", "poolCounts"]

def Document.wireValues (d : Document) : List WireValue :=
  [.string documentSchema, .string documentScope,
   encodeWord d.symbolBase,
   encodeTexts d.symbols,
   encodeWord d.sourceId,
   encodeWord d.sourceCount,
   encodeTags d.tags,
   encodeRows d.data,
   encodeWords d.resourceTransfers,
   encodeWords d.checkedCallTargetRequired,
   encodeWords d.extra,
   encodeTags d.patterns,
   encodeRows d.patternData,
   encodeTags d.stages,
   encodeRows d.stageData,
   encodeTags d.values,
   encodeRows d.valueData,
   encodeRows d.blocks,
   encodeTags d.driverStepTags,
   encodeRows d.driverSteps,
   encodeRows d.driverSlots,
   encodeRows d.driverWriters,
   encodeRows d.driverSync,
   encodeRows d.driverRegions,
   encodeRows d.driverPrograms,
   encodeRows d.bindingIdentities,
   encodeWord d.driverRoot,
   encodeRows d.functions,
   encodeWords d.functionInstructionStarts,
   encodeRows d.functionMetadata,
   encodeRows d.params,
   encodeRows d.paramCold,
   encodeRows d.captures,
   encodeRows d.validations,
   encodeWords d.slotTypeFacts,
   encodeRows d.strings,
   encodeBytes d.stringBytes,
   encodeRows d.bytes,
   encodeBytes d.byteData,
   encodeRows d.locations,
   encodeWords d.locationSources,
   encodeWords d.runtimeOps,
   encodeWords d.assignOps,
   encodeWords d.binaryOps,
   encodeWords d.runKinds,
   encodeWords d.redirectionKinds,
   encodeWords d.poolCounts]

def Document.encode (d : Document) : WireValue := .object (documentFields.zip d.wireValues)

def Document.ofValues? : List WireValue → Option Document
  | schemaValue :: scopeValue :: value0 :: value1 :: value2 :: value3 :: value4 :: value5 :: value6 :: value7 :: value8 :: value9 :: value10 :: value11 :: value12 :: value13 :: value14 :: value15 :: value16 :: value17 :: value18 :: value19 :: value20 :: value21 :: value22 :: value23 :: value24 :: value25 :: value26 :: value27 :: value28 :: value29 :: value30 :: value31 :: value32 :: value33 :: value34 :: value35 :: value36 :: value37 :: value38 :: value39 :: value40 :: value41 :: value42 :: value43 :: value44 :: [] => do
    expectText? documentSchema schemaValue
    expectText? documentScope scopeValue
    let symbolBase ← decodeWord? value0
    let symbols ← decodeTexts? value1
    let sourceId ← decodeWord? value2
    let sourceCount ← decodeWord? value3
    let tags ← decodeTags? value4
    let data ← decodeRows? 2 value5
    let resourceTransfers ← decodeWords? value6
    let checkedCallTargetRequired ← decodeWords? value7
    let extra ← decodeWords? value8
    let patterns ← decodeTags? value9
    let patternData ← decodeRows? 2 value10
    let stages ← decodeTags? value11
    let stageData ← decodeRows? 2 value12
    let values ← decodeTags? value13
    let valueData ← decodeRows? 2 value14
    let blocks ← decodeRows? 8 value15
    let driverStepTags ← decodeTags? value16
    let driverSteps ← decodeRows? 14 value17
    let driverSlots ← decodeRows? 8 value18
    let driverWriters ← decodeRows? 2 value19
    let driverSync ← decodeRows? 6 value20
    let driverRegions ← decodeRows? 5 value21
    let driverPrograms ← decodeRows? 6 value22
    let bindingIdentities ← decodeRows? 7 value23
    let driverRoot ← decodeWord? value24
    let functions ← decodeRows? 12 value25
    let functionInstructionStarts ← decodeWords? value26
    let functionMetadata ← decodeRows? 5 value27
    let params ← decodeRows? 5 value28
    let paramCold ← decodeRows? 3 value29
    let captures ← decodeRows? 6 value30
    let validations ← decodeRows? 2 value31
    let slotTypeFacts ← decodeWords? value32
    let strings ← decodeRows? 2 value33
    let stringBytes ← decodeBytes? value34
    let bytes ← decodeRows? 2 value35
    let byteData ← decodeBytes? value36
    let locations ← decodeRows? 2 value37
    let locationSources ← decodeWords? value38
    let runtimeOps ← decodeWords? value39
    let assignOps ← decodeWords? value40
    let binaryOps ← decodeWords? value41
    let runKinds ← decodeWords? value42
    let redirectionKinds ← decodeWords? value43
    let poolCounts ← decodeWords? value44
    pure { symbolBase, symbols, sourceId, sourceCount, tags, data, resourceTransfers, checkedCallTargetRequired, extra, patterns, patternData, stages, stageData, values, valueData, blocks, driverStepTags, driverSteps, driverSlots, driverWriters, driverSync, driverRegions, driverPrograms, bindingIdentities, driverRoot, functions, functionInstructionStarts, functionMetadata, params, paramCold, captures, validations, slotTypeFacts, strings, stringBytes, bytes, byteData, locations, locationSources, runtimeOps, assignOps, binaryOps, runKinds, redirectionKinds, poolCounts }
  | _ => none

def Document.decode? : WireValue → Option Document
  | .object fields => if fields.map Prod.fst = documentFields then Document.ofValues? (fields.map Prod.snd) else none
  | _ => none

def decodeDocumentCodes (codes : ExchangeCodes) : Option Document := (parseCodes codes).bind Document.decode?

def Document.WellShaped (d : Document) : Prop :=
  (∀ row ∈ d.data, row.length = 2) ∧
  (∀ row ∈ d.patternData, row.length = 2) ∧
  (∀ row ∈ d.stageData, row.length = 2) ∧
  (∀ row ∈ d.valueData, row.length = 2) ∧
  (∀ row ∈ d.blocks, row.length = 8) ∧
  (∀ row ∈ d.driverSteps, row.length = 14) ∧
  (∀ row ∈ d.driverSlots, row.length = 8) ∧
  (∀ row ∈ d.driverWriters, row.length = 2) ∧
  (∀ row ∈ d.driverSync, row.length = 6) ∧
  (∀ row ∈ d.driverRegions, row.length = 5) ∧
  (∀ row ∈ d.driverPrograms, row.length = 6) ∧
  (∀ row ∈ d.bindingIdentities, row.length = 7) ∧
  (∀ row ∈ d.functions, row.length = 12) ∧
  (∀ row ∈ d.functionMetadata, row.length = 5) ∧
  (∀ row ∈ d.params, row.length = 5) ∧
  (∀ row ∈ d.paramCold, row.length = 3) ∧
  (∀ row ∈ d.captures, row.length = 6) ∧
  (∀ row ∈ d.validations, row.length = 2) ∧
  (∀ row ∈ d.strings, row.length = 2) ∧
  (∀ row ∈ d.bytes, row.length = 2) ∧
  (∀ row ∈ d.locations, row.length = 2)

end Xsh.IR.Raw
