import Xsh.Conformance.Connected.ScopedDocuments
import Xsh.Conformance.Transport.JsonLexPrefix
namespace Xsh.Conformance.Connected
open Xsh.Conformance.Transport

def scopedChunk1 : List Nat := [123, 34, 115, 99, 104, 101, 109, 97, 34, 58, 34, 120, 115, 104, 45, 99, 111, 110, 110, 101, 99, 116, 101, 100, 45, 102, 105, 101, 108, 100, 45, 102, 97, 99, 116, 115, 45, 118, 49, 34, 44, 34, 102, 117, 110, 99, 116, 105, 111, 110, 115, 34, 58, 91, 123, 34, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 110, 97, 109, 101, 34, 58, 34, 115, 99, 111, 112, 101, 100, 34, 44]
def scopedChunk2 : List Nat := [34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 98, 111, 100, 121, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 50, 34, 125, 125, 93, 44, 34, 98, 111, 100, 105, 101, 115, 34, 58, 91, 123, 34, 115, 112, 97, 110, 34, 58, 123]
def scopedChunk3 : List Nat := [34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 50, 34, 125, 44, 34, 107, 105, 110, 100, 34, 58, 34, 112, 117, 114, 101, 34, 44, 34, 116, 121, 112, 101, 80, 97, 114, 97, 109, 101, 116, 101, 114, 115, 34, 58, 91]
def scopedChunk4 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 93, 44, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 115, 34, 58, 91, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 93, 44, 34, 114, 101, 115, 117, 108, 116, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58]
def scopedChunk5 : List Nat := [34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 44, 34, 101, 120, 112, 108, 105, 99, 105, 116, 82, 101, 116, 117, 114, 110, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 102, 102, 101, 99, 116, 115, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 117, 110, 107, 110, 111, 119, 110, 34, 44, 34, 101, 102, 102, 101, 99, 116, 115, 34, 58, 91, 93, 125, 44]
def scopedChunk6 : List Nat := [34, 105, 110, 102, 101, 114, 114, 101, 100, 69, 102, 102, 101, 99, 116, 115, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 115, 34, 58, 91, 123, 34, 107, 105, 110, 100, 34, 58, 34, 115, 116, 111, 114, 101, 86, 97, 108, 117, 101, 34, 44, 34, 115, 116, 111, 114, 97, 103, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 44]
def scopedChunk7 : List Nat := [34, 114, 101, 112, 108, 97, 99, 101, 109, 101, 110, 116, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 51, 34, 44]
def scopedChunk8 : List Nat := [34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 125, 93, 44, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 84, 121, 112, 101, 115, 34, 58, 91, 123, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58]
def scopedChunk9 : List Nat := [34, 48, 48, 48, 48, 48, 48, 50, 51, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 44, 123, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123]
def scopedChunk10 : List Nat := [34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 51, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 44]
def scopedChunk11 : List Nat := [123, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 50, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 51, 55, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58]
def scopedChunk12 : List Nat := [34, 117, 110, 105, 116, 34, 125, 125, 93, 44, 34, 116, 97, 105, 108, 84, 121, 112, 101, 115, 34, 58, 91, 123, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 51, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 99, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58]
def scopedChunk13 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 55, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 93, 44, 34, 114, 101, 97, 100, 66, 105, 110, 100, 105, 110, 103, 115, 34, 58, 91, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123]
def scopedChunk14 : List Nat := [34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 99, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58]
def scopedChunk15 : List Nat := [34, 102, 97, 108, 115, 101, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34, 99, 117, 114, 114, 101, 110, 116, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 53, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 52, 34, 125, 44]
def scopedChunk16 : List Nat := [34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58, 34, 102, 97, 108, 115, 101, 34, 125, 93, 44, 34, 114, 101, 115, 111, 108, 118, 101, 100, 82, 101, 97, 100, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 105, 110, 100, 105, 110, 103, 34, 58, 123]
def scopedChunk17 : List Nat := [34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 99, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44]
def scopedChunk18 : List Nat := [34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58, 34, 102, 97, 108, 115, 101, 34, 125, 125, 44, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 98, 105, 110, 100, 105, 110, 103, 34, 58, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123]
def scopedChunk19 : List Nat := [34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 99, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58]
def scopedChunk20 : List Nat := [34, 102, 97, 108, 115, 101, 34, 125, 125, 93, 44, 34, 114, 101, 115, 111, 108, 118, 101, 100, 84, 97, 105, 108, 115, 34, 58, 91, 123, 34, 115, 116, 97, 116, 101, 109, 101, 110, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 51, 34, 44, 34, 98, 105, 110, 100, 105, 110, 103, 34, 58, 123, 34, 110, 97, 109, 101, 34, 58, 34, 99, 117, 114, 114, 101, 110, 116, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58]
def scopedChunk21 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 53, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 52, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58, 34, 102, 97, 108, 115, 101, 34, 125, 125, 93, 44]
def scopedChunk22 : List Nat := [34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 83, 105, 116, 101, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 93, 125, 93, 44, 34, 99, 97, 108, 108, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58]
def scopedChunk23 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 53, 100, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 57, 34, 125, 44, 34, 114, 111, 108, 101, 34, 58, 34, 99, 97, 108, 108, 34, 44, 34, 116, 97, 114, 103, 101, 116, 34, 58, 123]
def scopedChunk24 : List Nat := [34, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 110, 97, 109, 101, 34, 58, 34, 115, 99, 111, 112, 101, 100, 34, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 98, 111, 100, 121, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 51, 34, 44]
def scopedChunk25 : List Nat := [34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 50, 34, 125, 125, 44, 34, 116, 121, 112, 101, 65, 114, 103, 117, 109, 101, 110, 116, 115, 34, 58, 91, 123, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 105, 110, 116, 34, 125, 125, 93, 44]
def scopedChunk26 : List Nat := [34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 66, 105, 110, 100, 105, 110, 103, 115, 34, 58, 91, 34, 115, 116, 111, 114, 101, 86, 97, 108, 117, 101, 34, 93, 125, 93, 125]
def scopedTail26 : List Nat := scopedChunk26
def scopedTail25 : List Nat := scopedChunk25 ++ scopedTail26
def scopedTail24 : List Nat := scopedChunk24 ++ scopedTail25
def scopedTail23 : List Nat := scopedChunk23 ++ scopedTail24
def scopedTail22 : List Nat := scopedChunk22 ++ scopedTail23
def scopedTail21 : List Nat := scopedChunk21 ++ scopedTail22
def scopedTail20 : List Nat := scopedChunk20 ++ scopedTail21
def scopedTail19 : List Nat := scopedChunk19 ++ scopedTail20
def scopedTail18 : List Nat := scopedChunk18 ++ scopedTail19
def scopedTail17 : List Nat := scopedChunk17 ++ scopedTail18
def scopedTail16 : List Nat := scopedChunk16 ++ scopedTail17
def scopedTail15 : List Nat := scopedChunk15 ++ scopedTail16
def scopedTail14 : List Nat := scopedChunk14 ++ scopedTail15
def scopedTail13 : List Nat := scopedChunk13 ++ scopedTail14
def scopedTail12 : List Nat := scopedChunk12 ++ scopedTail13
def scopedTail11 : List Nat := scopedChunk11 ++ scopedTail12
def scopedTail10 : List Nat := scopedChunk10 ++ scopedTail11
def scopedTail9 : List Nat := scopedChunk9 ++ scopedTail10
def scopedTail8 : List Nat := scopedChunk8 ++ scopedTail9
def scopedTail7 : List Nat := scopedChunk7 ++ scopedTail8
def scopedTail6 : List Nat := scopedChunk6 ++ scopedTail7
def scopedTail5 : List Nat := scopedChunk5 ++ scopedTail6
def scopedTail4 : List Nat := scopedChunk4 ++ scopedTail5
def scopedTail3 : List Nat := scopedChunk3 ++ scopedTail4
def scopedTail2 : List Nat := scopedChunk2 ++ scopedTail3
def scopedTail1 : List Nat := scopedChunk1 ++ scopedTail2
def scopedLexemes1 : List PlainLexeme := [.comma, .string [115, 99, 111, 112, 101, 100], .colon, .string [110, 97, 109, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [100, 101, 102, 105, 110, 105, 116, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [102, 117, 110, 99, 116, 105, 111, 110, 115], .comma, .string [120, 115, 104, 45, 99, 111, 110, 110, 101, 99, 116, 101, 100, 45, 102, 105, 101, 108, 100, 45, 102, 97, 99, 116, 115, 45, 118, 49], .colon, .string [115, 99, 104, 101, 109, 97], .leftBrace]
def scopedLexAcc1 : List Token := (scopedLexemes1.reverse.map PlainLexeme.token).reverse
def scopedLexemes2 : List PlainLexeme := [.leftBrace, .colon, .string [115, 112, 97, 110], .leftBrace, .leftBracket, .colon, .string [98, 111, 100, 105, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 52, 50], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 51], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [98, 111, 100, 121], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101]]
def scopedLexAcc2 : List Token := (scopedLexemes2.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc1
def scopedLexemes3 : List PlainLexeme := [.leftBracket, .colon, .string [116, 121, 112, 101, 80, 97, 114, 97, 109, 101, 116, 101, 114, 115], .comma, .string [112, 117, 114, 101], .colon, .string [107, 105, 110, 100], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 52, 50], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 51], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101]]
def scopedLexAcc3 : List Token := (scopedLexemes3.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc2
def scopedLexemes4 : List PlainLexeme := [.colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [114, 101, 115, 117, 108, 116], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .leftBracket, .colon, .string [112, 97, 114, 97, 109, 101, 116, 101, 114, 115], .comma, .rightBracket, .string [48, 48, 48, 48, 48, 48, 48, 48]]
def scopedLexAcc4 : List Token := (scopedLexemes4.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc3
def scopedLexemes5 : List PlainLexeme := [.comma, .rightBrace, .rightBracket, .leftBracket, .colon, .string [101, 102, 102, 101, 99, 116, 115], .comma, .string [117, 110, 107, 110, 111, 119, 110], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [101, 102, 102, 101, 99, 116, 115], .comma, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 108, 105, 99, 105, 116, 82, 101, 116, 117, 114, 110], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114]]
def scopedLexAcc5 : List Token := (scopedLexemes5.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc4
def scopedLexemes6 : List PlainLexeme := [.comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [115, 116, 111, 114, 97, 103, 101], .comma, .string [115, 116, 111, 114, 101, 86, 97, 108, 117, 101], .colon, .string [107, 105, 110, 100], .leftBrace, .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 115], .comma, .string [102, 97, 108, 115, 101], .colon, .string [105, 110, 102, 101, 114, 114, 101, 100, 69, 102, 102, 101, 99, 116, 115]]
def scopedLexAcc6 : List Token := (scopedLexemes6.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc5
def scopedLexemes7 : List PlainLexeme := [.comma, .string [48, 48, 48, 48, 48, 48, 52, 51], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [114, 101, 112, 108, 97, 99, 101, 109, 101, 110, 116]]
def scopedLexAcc7 : List Token := (scopedLexemes7.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc6
def scopedLexemes8 : List PlainLexeme := [.colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .leftBrace, .leftBracket, .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 84, 121, 112, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104]]
def scopedLexAcc8 : List Token := (scopedLexemes8.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc7
def scopedLexemes9 : List PlainLexeme := [.leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [105, 100], .leftBrace, .comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 50, 51]]
def scopedLexAcc9 : List Token := (scopedLexemes9.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc8
def scopedLexemes10 : List PlainLexeme := [.comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 52, 51], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101]]
def scopedLexAcc10 : List Token := (scopedLexemes10.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc9
def scopedLexemes11 : List PlainLexeme := [.colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 51], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 51, 55], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 50], .colon, .string [105, 100], .leftBrace]
def scopedLexAcc11 : List Token := (scopedLexemes11.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc10
def scopedLexemes12 : List PlainLexeme := [.colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 52, 99], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 51], .colon, .string [105, 100], .leftBrace, .leftBracket, .colon, .string [116, 97, 105, 108, 84, 121, 112, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [117, 110, 105, 116]]
def scopedLexAcc12 : List Token := (scopedLexemes12.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc11
def scopedLexemes13 : List PlainLexeme := [.leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .leftBracket, .colon, .string [114, 101, 97, 100, 66, 105, 110, 100, 105, 110, 103, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 55]]
def scopedLexAcc13 : List Token := (scopedLexemes13.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc12
def scopedLexemes14 : List PlainLexeme := [.colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 48, 99], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101]]
def scopedLexAcc14 : List Token := (scopedLexemes14.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc13
def scopedLexemes15 : List PlainLexeme := [.comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 52], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 53], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [99, 117, 114, 114, 101, 110, 116], .colon, .string [110, 97, 109, 101], .leftBrace, .comma, .rightBrace, .string [102, 97, 108, 115, 101]]
def scopedLexAcc15 : List Token := (scopedLexemes15.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc14
def scopedLexemes16 : List PlainLexeme := [.leftBrace, .colon, .string [98, 105, 110, 100, 105, 110, 103], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [114, 101, 115, 111, 108, 118, 101, 100, 82, 101, 97, 100, 115], .comma, .rightBracket, .rightBrace, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101]]
def scopedLexAcc16 : List Token := (scopedLexemes16.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc15
def scopedLexemes17 : List PlainLexeme := [.comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 48, 99], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101]]
def scopedLexAcc17 : List Token := (scopedLexemes17.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc16
def scopedLexemes18 : List PlainLexeme := [.leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .colon, .string [98, 105, 110, 100, 105, 110, 103], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .comma, .rightBrace, .rightBrace, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108]]
def scopedLexAcc18 : List Token := (scopedLexemes18.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc17
def scopedLexemes19 : List PlainLexeme := [.colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 48, 99], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101]]
def scopedLexAcc19 : List Token := (scopedLexemes19.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc18
def scopedLexemes20 : List PlainLexeme := [.colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [99, 117, 114, 114, 101, 110, 116], .colon, .string [110, 97, 109, 101], .leftBrace, .colon, .string [98, 105, 110, 100, 105, 110, 103], .comma, .string [48, 48, 48, 48, 48, 48, 48, 51], .colon, .string [115, 116, 97, 116, 101, 109, 101, 110, 116], .leftBrace, .leftBracket, .colon, .string [114, 101, 115, 111, 108, 118, 101, 100, 84, 97, 105, 108, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [102, 97, 108, 115, 101]]
def scopedLexAcc20 : List Token := (scopedLexemes20.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc19
def scopedLexemes21 : List PlainLexeme := [.comma, .rightBracket, .rightBrace, .rightBrace, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 52], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 53], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48]]
def scopedLexAcc21 : List Token := (scopedLexemes21.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc20
def scopedLexemes22 : List PlainLexeme := [.colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [99, 97, 108, 108, 115], .comma, .rightBracket, .rightBrace, .rightBracket, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 83, 105, 116, 101, 115]]
def scopedLexAcc22 : List Token := (scopedLexemes22.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc21
def scopedLexemes23 : List PlainLexeme := [.leftBrace, .colon, .string [116, 97, 114, 103, 101, 116], .comma, .string [99, 97, 108, 108], .colon, .string [114, 111, 108, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 57], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 53, 100], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 53]]
def scopedLexAcc23 : List Token := (scopedLexemes23.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc22
def scopedLexemes24 : List PlainLexeme := [.comma, .string [48, 48, 48, 48, 48, 48, 49, 51], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [98, 111, 100, 121], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .string [115, 99, 111, 112, 101, 100], .colon, .string [110, 97, 109, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [100, 101, 102, 105, 110, 105, 116, 105, 111, 110]]
def scopedLexAcc24 : List Token := (scopedLexemes24.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc23
def scopedLexemes25 : List PlainLexeme := [.comma, .rightBracket, .rightBrace, .rightBrace, .string [105, 110, 116], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .leftBrace, .leftBracket, .colon, .string [116, 121, 112, 101, 65, 114, 103, 117, 109, 101, 110, 116, 115], .comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 52, 50], .colon, .string [108, 101, 110, 103, 116, 104]]
def scopedLexAcc25 : List Token := (scopedLexemes25.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc24
def scopedLexemes26 : List PlainLexeme := [.rightBrace, .rightBracket, .rightBrace, .rightBracket, .string [115, 116, 111, 114, 101, 86, 97, 108, 117, 101], .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 66, 105, 110, 100, 105, 110, 103, 115]]
def scopedLexAcc26 : List Token := (scopedLexemes26.reverse.map PlainLexeme.token).reverse ++ scopedLexAcc25

set_option maxRecDepth 100000 in
theorem scopedCodeChunks : scopedFactsCodes = scopedTail1 := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkCodes1 : scopedChunk1 = scopedLexemes1.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid1 : scopedLexemes1.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep1 : lexPrefix scopedChunk1 .outside [] = some (.outside, scopedLexAcc1) := by
  rw [scopedChunkCodes1]
  exact lexPrefix_plain_lexemes _ [] scopedChunkValid1

set_option maxRecDepth 100000 in
theorem scopedChunkCodes2 : scopedChunk2 = scopedLexemes2.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid2 : scopedLexemes2.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep2 : lexPrefix scopedChunk2 .outside scopedLexAcc1 = some (.outside, scopedLexAcc2) := by
  rw [scopedChunkCodes2]
  exact lexPrefix_plain_lexemes _ scopedLexAcc1 scopedChunkValid2

set_option maxRecDepth 100000 in
theorem scopedChunkCodes3 : scopedChunk3 = scopedLexemes3.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid3 : scopedLexemes3.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep3 : lexPrefix scopedChunk3 .outside scopedLexAcc2 = some (.outside, scopedLexAcc3) := by
  rw [scopedChunkCodes3]
  exact lexPrefix_plain_lexemes _ scopedLexAcc2 scopedChunkValid3

set_option maxRecDepth 100000 in
theorem scopedChunkCodes4 : scopedChunk4 = scopedLexemes4.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid4 : scopedLexemes4.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep4 : lexPrefix scopedChunk4 .outside scopedLexAcc3 = some (.outside, scopedLexAcc4) := by
  rw [scopedChunkCodes4]
  exact lexPrefix_plain_lexemes _ scopedLexAcc3 scopedChunkValid4

set_option maxRecDepth 100000 in
theorem scopedChunkCodes5 : scopedChunk5 = scopedLexemes5.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid5 : scopedLexemes5.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep5 : lexPrefix scopedChunk5 .outside scopedLexAcc4 = some (.outside, scopedLexAcc5) := by
  rw [scopedChunkCodes5]
  exact lexPrefix_plain_lexemes _ scopedLexAcc4 scopedChunkValid5

set_option maxRecDepth 100000 in
theorem scopedChunkCodes6 : scopedChunk6 = scopedLexemes6.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid6 : scopedLexemes6.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep6 : lexPrefix scopedChunk6 .outside scopedLexAcc5 = some (.outside, scopedLexAcc6) := by
  rw [scopedChunkCodes6]
  exact lexPrefix_plain_lexemes _ scopedLexAcc5 scopedChunkValid6

set_option maxRecDepth 100000 in
theorem scopedChunkCodes7 : scopedChunk7 = scopedLexemes7.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid7 : scopedLexemes7.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep7 : lexPrefix scopedChunk7 .outside scopedLexAcc6 = some (.outside, scopedLexAcc7) := by
  rw [scopedChunkCodes7]
  exact lexPrefix_plain_lexemes _ scopedLexAcc6 scopedChunkValid7

set_option maxRecDepth 100000 in
theorem scopedChunkCodes8 : scopedChunk8 = scopedLexemes8.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid8 : scopedLexemes8.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep8 : lexPrefix scopedChunk8 .outside scopedLexAcc7 = some (.outside, scopedLexAcc8) := by
  rw [scopedChunkCodes8]
  exact lexPrefix_plain_lexemes _ scopedLexAcc7 scopedChunkValid8

set_option maxRecDepth 100000 in
theorem scopedChunkCodes9 : scopedChunk9 = scopedLexemes9.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid9 : scopedLexemes9.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep9 : lexPrefix scopedChunk9 .outside scopedLexAcc8 = some (.outside, scopedLexAcc9) := by
  rw [scopedChunkCodes9]
  exact lexPrefix_plain_lexemes _ scopedLexAcc8 scopedChunkValid9

set_option maxRecDepth 100000 in
theorem scopedChunkCodes10 : scopedChunk10 = scopedLexemes10.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid10 : scopedLexemes10.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep10 : lexPrefix scopedChunk10 .outside scopedLexAcc9 = some (.outside, scopedLexAcc10) := by
  rw [scopedChunkCodes10]
  exact lexPrefix_plain_lexemes _ scopedLexAcc9 scopedChunkValid10

set_option maxRecDepth 100000 in
theorem scopedChunkCodes11 : scopedChunk11 = scopedLexemes11.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid11 : scopedLexemes11.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep11 : lexPrefix scopedChunk11 .outside scopedLexAcc10 = some (.outside, scopedLexAcc11) := by
  rw [scopedChunkCodes11]
  exact lexPrefix_plain_lexemes _ scopedLexAcc10 scopedChunkValid11

set_option maxRecDepth 100000 in
theorem scopedChunkCodes12 : scopedChunk12 = scopedLexemes12.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid12 : scopedLexemes12.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep12 : lexPrefix scopedChunk12 .outside scopedLexAcc11 = some (.outside, scopedLexAcc12) := by
  rw [scopedChunkCodes12]
  exact lexPrefix_plain_lexemes _ scopedLexAcc11 scopedChunkValid12

set_option maxRecDepth 100000 in
theorem scopedChunkCodes13 : scopedChunk13 = scopedLexemes13.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid13 : scopedLexemes13.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep13 : lexPrefix scopedChunk13 .outside scopedLexAcc12 = some (.outside, scopedLexAcc13) := by
  rw [scopedChunkCodes13]
  exact lexPrefix_plain_lexemes _ scopedLexAcc12 scopedChunkValid13

set_option maxRecDepth 100000 in
theorem scopedChunkCodes14 : scopedChunk14 = scopedLexemes14.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid14 : scopedLexemes14.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep14 : lexPrefix scopedChunk14 .outside scopedLexAcc13 = some (.outside, scopedLexAcc14) := by
  rw [scopedChunkCodes14]
  exact lexPrefix_plain_lexemes _ scopedLexAcc13 scopedChunkValid14

set_option maxRecDepth 100000 in
theorem scopedChunkCodes15 : scopedChunk15 = scopedLexemes15.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid15 : scopedLexemes15.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep15 : lexPrefix scopedChunk15 .outside scopedLexAcc14 = some (.outside, scopedLexAcc15) := by
  rw [scopedChunkCodes15]
  exact lexPrefix_plain_lexemes _ scopedLexAcc14 scopedChunkValid15

set_option maxRecDepth 100000 in
theorem scopedChunkCodes16 : scopedChunk16 = scopedLexemes16.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid16 : scopedLexemes16.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep16 : lexPrefix scopedChunk16 .outside scopedLexAcc15 = some (.outside, scopedLexAcc16) := by
  rw [scopedChunkCodes16]
  exact lexPrefix_plain_lexemes _ scopedLexAcc15 scopedChunkValid16

set_option maxRecDepth 100000 in
theorem scopedChunkCodes17 : scopedChunk17 = scopedLexemes17.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid17 : scopedLexemes17.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep17 : lexPrefix scopedChunk17 .outside scopedLexAcc16 = some (.outside, scopedLexAcc17) := by
  rw [scopedChunkCodes17]
  exact lexPrefix_plain_lexemes _ scopedLexAcc16 scopedChunkValid17

set_option maxRecDepth 100000 in
theorem scopedChunkCodes18 : scopedChunk18 = scopedLexemes18.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid18 : scopedLexemes18.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep18 : lexPrefix scopedChunk18 .outside scopedLexAcc17 = some (.outside, scopedLexAcc18) := by
  rw [scopedChunkCodes18]
  exact lexPrefix_plain_lexemes _ scopedLexAcc17 scopedChunkValid18

set_option maxRecDepth 100000 in
theorem scopedChunkCodes19 : scopedChunk19 = scopedLexemes19.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid19 : scopedLexemes19.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep19 : lexPrefix scopedChunk19 .outside scopedLexAcc18 = some (.outside, scopedLexAcc19) := by
  rw [scopedChunkCodes19]
  exact lexPrefix_plain_lexemes _ scopedLexAcc18 scopedChunkValid19

set_option maxRecDepth 100000 in
theorem scopedChunkCodes20 : scopedChunk20 = scopedLexemes20.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid20 : scopedLexemes20.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep20 : lexPrefix scopedChunk20 .outside scopedLexAcc19 = some (.outside, scopedLexAcc20) := by
  rw [scopedChunkCodes20]
  exact lexPrefix_plain_lexemes _ scopedLexAcc19 scopedChunkValid20

set_option maxRecDepth 100000 in
theorem scopedChunkCodes21 : scopedChunk21 = scopedLexemes21.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid21 : scopedLexemes21.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep21 : lexPrefix scopedChunk21 .outside scopedLexAcc20 = some (.outside, scopedLexAcc21) := by
  rw [scopedChunkCodes21]
  exact lexPrefix_plain_lexemes _ scopedLexAcc20 scopedChunkValid21

set_option maxRecDepth 100000 in
theorem scopedChunkCodes22 : scopedChunk22 = scopedLexemes22.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid22 : scopedLexemes22.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep22 : lexPrefix scopedChunk22 .outside scopedLexAcc21 = some (.outside, scopedLexAcc22) := by
  rw [scopedChunkCodes22]
  exact lexPrefix_plain_lexemes _ scopedLexAcc21 scopedChunkValid22

set_option maxRecDepth 100000 in
theorem scopedChunkCodes23 : scopedChunk23 = scopedLexemes23.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid23 : scopedLexemes23.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep23 : lexPrefix scopedChunk23 .outside scopedLexAcc22 = some (.outside, scopedLexAcc23) := by
  rw [scopedChunkCodes23]
  exact lexPrefix_plain_lexemes _ scopedLexAcc22 scopedChunkValid23

set_option maxRecDepth 100000 in
theorem scopedChunkCodes24 : scopedChunk24 = scopedLexemes24.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid24 : scopedLexemes24.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep24 : lexPrefix scopedChunk24 .outside scopedLexAcc23 = some (.outside, scopedLexAcc24) := by
  rw [scopedChunkCodes24]
  exact lexPrefix_plain_lexemes _ scopedLexAcc23 scopedChunkValid24

set_option maxRecDepth 100000 in
theorem scopedChunkCodes25 : scopedChunk25 = scopedLexemes25.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid25 : scopedLexemes25.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep25 : lexPrefix scopedChunk25 .outside scopedLexAcc24 = some (.outside, scopedLexAcc25) := by
  rw [scopedChunkCodes25]
  exact lexPrefix_plain_lexemes _ scopedLexAcc24 scopedChunkValid25

set_option maxRecDepth 100000 in
theorem scopedChunkCodes26 : scopedChunk26 = scopedLexemes26.reverse.flatMap PlainLexeme.codes := by rfl

set_option maxRecDepth 100000 in
theorem scopedChunkValid26 : scopedLexemes26.reverse.all PlainLexeme.valid = true := by decide

theorem scopedPrefixStep26 : lexPrefix scopedChunk26 .outside scopedLexAcc25 = some (.outside, scopedLexAcc26) := by
  rw [scopedChunkCodes26]
  exact lexPrefix_plain_lexemes _ scopedLexAcc25 scopedChunkValid26

theorem scopedLexTransition1 (suffix : List Nat) (tail : List Token) : lex (scopedChunk1 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes1.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes1, lexPrefix_plain_lexemes _ tail scopedChunkValid1]
  rfl

theorem scopedLexTransition2 (suffix : List Nat) (tail : List Token) : lex (scopedChunk2 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes2.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes2, lexPrefix_plain_lexemes _ tail scopedChunkValid2]
  rfl

theorem scopedLexTransition3 (suffix : List Nat) (tail : List Token) : lex (scopedChunk3 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes3.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes3, lexPrefix_plain_lexemes _ tail scopedChunkValid3]
  rfl

theorem scopedLexTransition4 (suffix : List Nat) (tail : List Token) : lex (scopedChunk4 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes4.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes4, lexPrefix_plain_lexemes _ tail scopedChunkValid4]
  rfl

theorem scopedLexTransition5 (suffix : List Nat) (tail : List Token) : lex (scopedChunk5 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes5.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes5, lexPrefix_plain_lexemes _ tail scopedChunkValid5]
  rfl

theorem scopedLexTransition6 (suffix : List Nat) (tail : List Token) : lex (scopedChunk6 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes6.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes6, lexPrefix_plain_lexemes _ tail scopedChunkValid6]
  rfl

theorem scopedLexTransition7 (suffix : List Nat) (tail : List Token) : lex (scopedChunk7 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes7.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes7, lexPrefix_plain_lexemes _ tail scopedChunkValid7]
  rfl

theorem scopedLexTransition8 (suffix : List Nat) (tail : List Token) : lex (scopedChunk8 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes8.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes8, lexPrefix_plain_lexemes _ tail scopedChunkValid8]
  rfl

theorem scopedLexTransition9 (suffix : List Nat) (tail : List Token) : lex (scopedChunk9 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes9.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes9, lexPrefix_plain_lexemes _ tail scopedChunkValid9]
  rfl

theorem scopedLexTransition10 (suffix : List Nat) (tail : List Token) : lex (scopedChunk10 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes10.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes10, lexPrefix_plain_lexemes _ tail scopedChunkValid10]
  rfl

theorem scopedLexTransition11 (suffix : List Nat) (tail : List Token) : lex (scopedChunk11 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes11.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes11, lexPrefix_plain_lexemes _ tail scopedChunkValid11]
  rfl

theorem scopedLexTransition12 (suffix : List Nat) (tail : List Token) : lex (scopedChunk12 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes12.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes12, lexPrefix_plain_lexemes _ tail scopedChunkValid12]
  rfl

theorem scopedLexTransition13 (suffix : List Nat) (tail : List Token) : lex (scopedChunk13 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes13.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes13, lexPrefix_plain_lexemes _ tail scopedChunkValid13]
  rfl

theorem scopedLexTransition14 (suffix : List Nat) (tail : List Token) : lex (scopedChunk14 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes14.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes14, lexPrefix_plain_lexemes _ tail scopedChunkValid14]
  rfl

theorem scopedLexTransition15 (suffix : List Nat) (tail : List Token) : lex (scopedChunk15 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes15.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes15, lexPrefix_plain_lexemes _ tail scopedChunkValid15]
  rfl

theorem scopedLexTransition16 (suffix : List Nat) (tail : List Token) : lex (scopedChunk16 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes16.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes16, lexPrefix_plain_lexemes _ tail scopedChunkValid16]
  rfl

theorem scopedLexTransition17 (suffix : List Nat) (tail : List Token) : lex (scopedChunk17 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes17.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes17, lexPrefix_plain_lexemes _ tail scopedChunkValid17]
  rfl

theorem scopedLexTransition18 (suffix : List Nat) (tail : List Token) : lex (scopedChunk18 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes18.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes18, lexPrefix_plain_lexemes _ tail scopedChunkValid18]
  rfl

theorem scopedLexTransition19 (suffix : List Nat) (tail : List Token) : lex (scopedChunk19 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes19.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes19, lexPrefix_plain_lexemes _ tail scopedChunkValid19]
  rfl

theorem scopedLexTransition20 (suffix : List Nat) (tail : List Token) : lex (scopedChunk20 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes20.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes20, lexPrefix_plain_lexemes _ tail scopedChunkValid20]
  rfl

theorem scopedLexTransition21 (suffix : List Nat) (tail : List Token) : lex (scopedChunk21 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes21.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes21, lexPrefix_plain_lexemes _ tail scopedChunkValid21]
  rfl

theorem scopedLexTransition22 (suffix : List Nat) (tail : List Token) : lex (scopedChunk22 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes22.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes22, lexPrefix_plain_lexemes _ tail scopedChunkValid22]
  rfl

theorem scopedLexTransition23 (suffix : List Nat) (tail : List Token) : lex (scopedChunk23 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes23.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes23, lexPrefix_plain_lexemes _ tail scopedChunkValid23]
  rfl

theorem scopedLexTransition24 (suffix : List Nat) (tail : List Token) : lex (scopedChunk24 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes24.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes24, lexPrefix_plain_lexemes _ tail scopedChunkValid24]
  rfl

theorem scopedLexTransition25 (suffix : List Nat) (tail : List Token) : lex (scopedChunk25 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes25.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes25, lexPrefix_plain_lexemes _ tail scopedChunkValid25]
  rfl

theorem scopedLexTransition26 (suffix : List Nat) (tail : List Token) : lex (scopedChunk26 ++ suffix) .outside tail = lex suffix .outside ((scopedLexemes26.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, scopedChunkCodes26, lexPrefix_plain_lexemes _ tail scopedChunkValid26]
  rfl

set_option maxRecDepth 100000 in
theorem scopedLexResult : lex scopedFactsCodes .outside [] = some scopedLexAcc26.reverse := by
  rw [scopedCodeChunks]
  change lex (scopedChunk1 ++ scopedTail2) .outside [] = _
  rw [scopedLexTransition1]
  change lex (scopedChunk2 ++ scopedTail3) .outside scopedLexAcc1 = _
  rw [scopedLexTransition2]
  change lex (scopedChunk3 ++ scopedTail4) .outside scopedLexAcc2 = _
  rw [scopedLexTransition3]
  change lex (scopedChunk4 ++ scopedTail5) .outside scopedLexAcc3 = _
  rw [scopedLexTransition4]
  change lex (scopedChunk5 ++ scopedTail6) .outside scopedLexAcc4 = _
  rw [scopedLexTransition5]
  change lex (scopedChunk6 ++ scopedTail7) .outside scopedLexAcc5 = _
  rw [scopedLexTransition6]
  change lex (scopedChunk7 ++ scopedTail8) .outside scopedLexAcc6 = _
  rw [scopedLexTransition7]
  change lex (scopedChunk8 ++ scopedTail9) .outside scopedLexAcc7 = _
  rw [scopedLexTransition8]
  change lex (scopedChunk9 ++ scopedTail10) .outside scopedLexAcc8 = _
  rw [scopedLexTransition9]
  change lex (scopedChunk10 ++ scopedTail11) .outside scopedLexAcc9 = _
  rw [scopedLexTransition10]
  change lex (scopedChunk11 ++ scopedTail12) .outside scopedLexAcc10 = _
  rw [scopedLexTransition11]
  change lex (scopedChunk12 ++ scopedTail13) .outside scopedLexAcc11 = _
  rw [scopedLexTransition12]
  change lex (scopedChunk13 ++ scopedTail14) .outside scopedLexAcc12 = _
  rw [scopedLexTransition13]
  change lex (scopedChunk14 ++ scopedTail15) .outside scopedLexAcc13 = _
  rw [scopedLexTransition14]
  change lex (scopedChunk15 ++ scopedTail16) .outside scopedLexAcc14 = _
  rw [scopedLexTransition15]
  change lex (scopedChunk16 ++ scopedTail17) .outside scopedLexAcc15 = _
  rw [scopedLexTransition16]
  change lex (scopedChunk17 ++ scopedTail18) .outside scopedLexAcc16 = _
  rw [scopedLexTransition17]
  change lex (scopedChunk18 ++ scopedTail19) .outside scopedLexAcc17 = _
  rw [scopedLexTransition18]
  change lex (scopedChunk19 ++ scopedTail20) .outside scopedLexAcc18 = _
  rw [scopedLexTransition19]
  change lex (scopedChunk20 ++ scopedTail21) .outside scopedLexAcc19 = _
  rw [scopedLexTransition20]
  change lex (scopedChunk21 ++ scopedTail22) .outside scopedLexAcc20 = _
  rw [scopedLexTransition21]
  change lex (scopedChunk22 ++ scopedTail23) .outside scopedLexAcc21 = _
  rw [scopedLexTransition22]
  change lex (scopedChunk23 ++ scopedTail24) .outside scopedLexAcc22 = _
  rw [scopedLexTransition23]
  change lex (scopedChunk24 ++ scopedTail25) .outside scopedLexAcc23 = _
  rw [scopedLexTransition24]
  change lex (scopedChunk25 ++ scopedTail26) .outside scopedLexAcc24 = _
  rw [scopedLexTransition25]
  change lex (scopedChunk26 ++ []) .outside scopedLexAcc25 = _
  rw [scopedLexTransition26]
  rfl


end Xsh.Conformance.Connected
