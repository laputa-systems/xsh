import Xsh.Conformance.Connected.Documents
import Xsh.Conformance.Transport.JsonLexPrefix
namespace Xsh.Conformance.Connected
open Xsh.Conformance.Transport

def connectedChunk1 : List Nat := [123, 34, 115, 99, 104, 101, 109, 97, 34, 58, 34, 120, 115, 104, 45, 99, 111, 110, 110, 101, 99, 116, 101, 100, 45, 102, 105, 101, 108, 100, 45, 102, 97, 99, 116, 115, 45, 118, 49, 34, 44, 34, 102, 117, 110, 99, 116, 105, 111, 110, 115, 34, 58, 91, 123, 34, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 110, 97, 109, 101, 34, 58, 34, 112, 114, 111, 106, 101, 99, 116, 34, 44]
def connectedChunk2 : List Nat := [34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 98, 111, 100, 121, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 52, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 48, 34, 125, 125, 44, 123, 34, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 34, 58]
def connectedChunk3 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 110, 97, 109, 101, 34, 58, 34, 117, 115, 101, 95, 112, 114, 111, 106, 101, 99, 116, 34, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 98, 111, 100, 121, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 51, 102, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58]
def connectedChunk4 : List Nat := [34, 48, 48, 48, 48, 48, 48, 50, 55, 34, 125, 125, 93, 44, 34, 98, 111, 100, 105, 101, 115, 34, 58, 91, 123, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 52, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 48, 34, 125, 44, 34, 107, 105, 110, 100, 34, 58]
def connectedChunk5 : List Nat := [34, 112, 117, 114, 101, 34, 44, 34, 116, 121, 112, 101, 80, 97, 114, 97, 109, 101, 116, 101, 114, 115, 34, 58, 91, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 93, 44, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 115, 34, 58, 91, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58]
def connectedChunk6 : List Nat := [34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 93, 44, 34, 114, 101, 115, 117, 108, 116, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 125, 44, 34, 101, 120, 112, 108, 105, 99, 105, 116, 82, 101, 116, 117, 114, 110, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44]
def connectedChunk7 : List Nat := [34, 101, 102, 102, 101, 99, 116, 115, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 117, 110, 107, 110, 111, 119, 110, 34, 44, 34, 101, 102, 102, 101, 99, 116, 115, 34, 58, 91, 93, 125, 44, 34, 105, 110, 102, 101, 114, 114, 101, 100, 69, 102, 102, 101, 99, 116, 115, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 115, 34, 58, 91, 123, 34, 107, 105, 110, 100, 34, 58, 34, 102, 105, 101, 108, 100, 34, 44, 34, 114, 101, 99, 101, 105, 118, 101, 114, 34]
def connectedChunk8 : List Nat := [58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 44, 34, 110, 97, 109, 101, 34, 58, 34, 97, 109, 111, 117, 110, 116, 34, 44, 34, 114, 101, 115, 117, 108, 116, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 125, 44]
def connectedChunk9 : List Nat := [34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 54, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 99, 34, 125, 125, 93, 44, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 84, 121, 112, 101, 115, 34, 58, 91, 123, 34, 105, 100, 34, 58]
def connectedChunk10 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 54, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34]
def connectedChunk11 : List Nat := [44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 125, 44, 123, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 54, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58]
def connectedChunk12 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 99, 34, 125, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 44, 34, 105, 100, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 125, 125, 93, 44, 34, 116, 97, 105, 108, 84, 121, 112, 101, 115, 34, 58, 91, 93, 44, 34, 114, 101, 97, 100, 66, 105, 110, 100, 105, 110, 103, 115, 34, 58, 91, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44]
def connectedChunk13 : List Nat := [34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 100, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44]
def connectedChunk14 : List Nat := [34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58, 34, 102, 97, 108, 115, 101, 34, 125, 93, 44, 34, 114, 101, 115, 111, 108, 118, 101, 100, 82, 101, 97, 100, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 105, 110, 100, 105, 110, 103, 34, 58, 123, 34, 110, 97, 109, 101, 34, 58, 34, 118, 97, 108, 117, 101, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123]
def connectedChunk15 : List Nat := [34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 100, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 53, 34, 125, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 103, 108, 111, 98, 97, 108, 34, 58, 34, 102, 97, 108, 115, 101, 34, 44, 34, 101, 120, 112, 111, 114, 116, 101, 100, 34, 58]
def connectedChunk16 : List Nat := [34, 102, 97, 108, 115, 101, 34, 125, 125, 93, 44, 34, 114, 101, 115, 111, 108, 118, 101, 100, 84, 97, 105, 108, 115, 34, 58, 91, 93, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 83, 105, 116, 101, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 125, 93, 125, 93, 44]
def connectedChunk17 : List Nat := [34, 99, 97, 108, 108, 115, 34, 58, 91, 123, 34, 101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 56, 34, 44, 34, 115, 112, 97, 110, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 52, 49, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 50, 51, 34, 125, 44]
def connectedChunk18 : List Nat := [34, 114, 111, 108, 101, 34, 58, 34, 99, 97, 108, 108, 34, 44, 34, 116, 97, 114, 103, 101, 116, 34, 58, 123, 34, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 110, 97, 109, 101, 34, 58, 34, 112, 114, 111, 106, 101, 99, 116, 34, 44, 34, 110, 97, 109, 101, 115, 112, 97, 99, 101, 34, 58, 91, 93, 44, 34, 98, 111, 100, 121, 34, 58, 123, 34, 115, 111, 117, 114, 99, 101, 34, 58]
def connectedChunk19 : List Nat := [34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 115, 116, 97, 114, 116, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 52, 34, 44, 34, 108, 101, 110, 103, 116, 104, 34, 58, 34, 48, 48, 48, 48, 48, 48, 49, 48, 34, 125, 125, 44, 34, 116, 121, 112, 101, 65, 114, 103, 117, 109, 101, 110, 116, 115, 34, 58, 91, 123, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123]
def connectedChunk20 : List Nat := [34, 107, 105, 110, 100, 34, 58, 34, 114, 101, 99, 111, 114, 100, 34, 44, 34, 102, 105, 101, 108, 100, 115, 34, 58, 91, 123, 34, 110, 97, 109, 101, 34, 58, 34, 97, 109, 111, 117, 110, 116, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 105, 110, 116, 34, 125, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34, 101, 110, 97, 98, 108, 101, 100, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58]
def connectedChunk21 : List Nat := [34, 98, 111, 111, 108, 34, 125, 125, 93, 125, 125, 44, 123, 34, 112, 97, 114, 97, 109, 101, 116, 101, 114, 34, 58, 34, 48, 48, 48, 48, 48, 48, 48, 49, 34, 44, 34, 116, 121, 112, 101, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34, 105, 110, 116, 34, 125, 125, 93, 44, 34, 111, 112, 101, 114, 97, 116, 105, 111, 110, 66, 105, 110, 100, 105, 110, 103, 115, 34, 58, 91, 34, 102, 105, 101, 108, 100, 34, 93, 125, 93, 125]
def connectedTail21 : List Nat := connectedChunk21
def connectedTail20 : List Nat := connectedChunk20 ++ connectedTail21
def connectedTail19 : List Nat := connectedChunk19 ++ connectedTail20
def connectedTail18 : List Nat := connectedChunk18 ++ connectedTail19
def connectedTail17 : List Nat := connectedChunk17 ++ connectedTail18
def connectedTail16 : List Nat := connectedChunk16 ++ connectedTail17
def connectedTail15 : List Nat := connectedChunk15 ++ connectedTail16
def connectedTail14 : List Nat := connectedChunk14 ++ connectedTail15
def connectedTail13 : List Nat := connectedChunk13 ++ connectedTail14
def connectedTail12 : List Nat := connectedChunk12 ++ connectedTail13
def connectedTail11 : List Nat := connectedChunk11 ++ connectedTail12
def connectedTail10 : List Nat := connectedChunk10 ++ connectedTail11
def connectedTail9 : List Nat := connectedChunk9 ++ connectedTail10
def connectedTail8 : List Nat := connectedChunk8 ++ connectedTail9
def connectedTail7 : List Nat := connectedChunk7 ++ connectedTail8
def connectedTail6 : List Nat := connectedChunk6 ++ connectedTail7
def connectedTail5 : List Nat := connectedChunk5 ++ connectedTail6
def connectedTail4 : List Nat := connectedChunk4 ++ connectedTail5
def connectedTail3 : List Nat := connectedChunk3 ++ connectedTail4
def connectedTail2 : List Nat := connectedChunk2 ++ connectedTail3
def connectedTail1 : List Nat := connectedChunk1 ++ connectedTail2
def connectedLexemes1 : List PlainLexeme := [.comma, .string [112, 114, 111, 106, 101, 99, 116], .colon, .string [110, 97, 109, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [100, 101, 102, 105, 110, 105, 116, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [102, 117, 110, 99, 116, 105, 111, 110, 115], .comma, .string [120, 115, 104, 45, 99, 111, 110, 110, 101, 99, 116, 101, 100, 45, 102, 105, 101, 108, 100, 45, 102, 97, 99, 116, 115, 45, 118, 49], .colon, .string [115, 99, 104, 101, 109, 97], .leftBrace]
def connectedAcc1 : List Token := (connectedLexemes1.reverse.map PlainLexeme.token).reverse
def connectedLexemes2 : List PlainLexeme := [.colon, .string [100, 101, 102, 105, 110, 105, 116, 105, 111, 110], .leftBrace, .comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 48], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 52], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [98, 111, 100, 121], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101]]
def connectedAcc2 : List Token := (connectedLexemes2.reverse.map PlainLexeme.token).reverse ++ connectedAcc1
def connectedLexemes3 : List PlainLexeme := [.colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 51, 102], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [98, 111, 100, 121], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .string [117, 115, 101, 95, 112, 114, 111, 106, 101, 99, 116], .colon, .string [110, 97, 109, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49]]
def connectedAcc3 : List Token := (connectedLexemes3.reverse.map PlainLexeme.token).reverse ++ connectedAcc2
def connectedLexemes4 : List PlainLexeme := [.colon, .string [107, 105, 110, 100], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 48], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 52], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .leftBrace, .leftBracket, .colon, .string [98, 111, 100, 105, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 50, 55]]
def connectedAcc4 : List Token := (connectedLexemes4.reverse.map PlainLexeme.token).reverse ++ connectedAcc3
def connectedLexemes5 : List PlainLexeme := [.colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .leftBracket, .colon, .string [112, 97, 114, 97, 109, 101, 116, 101, 114, 115], .comma, .rightBracket, .string [48, 48, 48, 48, 48, 48, 48, 49], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .leftBracket, .colon, .string [116, 121, 112, 101, 80, 97, 114, 97, 109, 101, 116, 101, 114, 115], .comma, .string [112, 117, 114, 101]]
def connectedAcc5 : List Token := (connectedLexemes5.reverse.map PlainLexeme.token).reverse ++ connectedAcc4
def connectedLexemes6 : List PlainLexeme := [.comma, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 108, 105, 99, 105, 116, 82, 101, 116, 117, 114, 110], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [114, 101, 115, 117, 108, 116], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114]]
def connectedAcc6 : List Token := (connectedLexemes6.reverse.map PlainLexeme.token).reverse ++ connectedAcc5
def connectedLexemes7 : List PlainLexeme := [.string [114, 101, 99, 101, 105, 118, 101, 114], .comma, .string [102, 105, 101, 108, 100], .colon, .string [107, 105, 110, 100], .leftBrace, .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 115], .comma, .string [102, 97, 108, 115, 101], .colon, .string [105, 110, 102, 101, 114, 114, 101, 100, 69, 102, 102, 101, 99, 116, 115], .comma, .rightBrace, .rightBracket, .leftBracket, .colon, .string [101, 102, 102, 101, 99, 116, 115], .comma, .string [117, 110, 107, 110, 111, 119, 110], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [101, 102, 102, 101, 99, 116, 115]]
def connectedAcc7 : List Token := (connectedLexemes7.reverse.map PlainLexeme.token).reverse ++ connectedAcc6
def connectedLexemes8 : List PlainLexeme := [.comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [114, 101, 115, 117, 108, 116], .comma, .string [97, 109, 111, 117, 110, 116], .colon, .string [110, 97, 109, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon]
def connectedAcc8 : List Token := (connectedLexemes8.reverse.map PlainLexeme.token).reverse ++ connectedAcc7
def connectedLexemes9 : List PlainLexeme := [.colon, .string [105, 100], .leftBrace, .leftBracket, .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110, 84, 121, 112, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 99], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 54], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110]]
def connectedAcc9 : List Token := (connectedLexemes9.reverse.map PlainLexeme.token).reverse ++ connectedAcc8
def connectedLexemes10 : List PlainLexeme := [.string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 54], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48]]
def connectedAcc10 : List Token := (connectedLexemes10.reverse.map PlainLexeme.token).reverse ++ connectedAcc9
def connectedLexemes11 : List PlainLexeme := [.colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 54], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [105, 100], .leftBrace, .comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [105, 100], .comma]
def connectedAcc11 : List Token := (connectedLexemes11.reverse.map PlainLexeme.token).reverse ++ connectedAcc10
def connectedLexemes12 : List PlainLexeme := [.comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .leftBracket, .colon, .string [114, 101, 97, 100, 66, 105, 110, 100, 105, 110, 103, 115], .comma, .rightBracket, .leftBracket, .colon, .string [116, 97, 105, 108, 84, 121, 112, 101, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [105, 100], .comma, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 99]]
def connectedAcc12 : List Token := (connectedLexemes12.reverse.map PlainLexeme.token).reverse ++ connectedAcc11
def connectedLexemes13 : List PlainLexeme := [.comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 48, 100], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110]]
def connectedAcc13 : List Token := (connectedLexemes13.reverse.map PlainLexeme.token).reverse ++ connectedAcc12
def connectedLexemes14 : List PlainLexeme := [.leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [118, 97, 108, 117, 101], .colon, .string [110, 97, 109, 101], .leftBrace, .colon, .string [98, 105, 110, 100, 105, 110, 103], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [114, 101, 115, 111, 108, 118, 101, 100, 82, 101, 97, 100, 115], .comma, .rightBracket, .rightBrace, .string [102, 97, 108, 115, 101], .colon, .string [101, 120, 112, 111, 114, 116, 101, 100]]
def connectedAcc14 : List Token := (connectedLexemes14.reverse.map PlainLexeme.token).reverse ++ connectedAcc13
def connectedLexemes15 : List PlainLexeme := [.colon, .string [101, 120, 112, 111, 114, 116, 101, 100], .comma, .string [102, 97, 108, 115, 101], .colon, .string [103, 108, 111, 98, 97, 108], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 53], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 48, 100], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101]]
def connectedAcc15 : List Token := (connectedLexemes15.reverse.map PlainLexeme.token).reverse ++ connectedAcc14
def connectedLexemes16 : List PlainLexeme := [.comma, .rightBracket, .rightBrace, .rightBracket, .rightBrace, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 83, 105, 116, 101, 115], .comma, .rightBracket, .leftBracket, .colon, .string [114, 101, 115, 111, 108, 118, 101, 100, 84, 97, 105, 108, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [102, 97, 108, 115, 101]]
def connectedAcc16 : List Token := (connectedLexemes16.reverse.map PlainLexeme.token).reverse ++ connectedAcc15
def connectedLexemes17 : List PlainLexeme := [.comma, .rightBrace, .string [48, 48, 48, 48, 48, 48, 50, 51], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 52, 49], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [115, 112, 97, 110], .comma, .string [48, 48, 48, 48, 48, 48, 48, 56], .colon, .string [101, 120, 112, 114, 101, 115, 115, 105, 111, 110], .leftBrace, .leftBracket, .colon, .string [99, 97, 108, 108, 115]]
def connectedAcc17 : List Token := (connectedLexemes17.reverse.map PlainLexeme.token).reverse ++ connectedAcc16
def connectedLexemes18 : List PlainLexeme := [.colon, .string [115, 111, 117, 114, 99, 101], .leftBrace, .colon, .string [98, 111, 100, 121], .comma, .rightBracket, .leftBracket, .colon, .string [110, 97, 109, 101, 115, 112, 97, 99, 101], .comma, .string [112, 114, 111, 106, 101, 99, 116], .colon, .string [110, 97, 109, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [100, 101, 102, 105, 110, 105, 116, 105, 111, 110], .leftBrace, .colon, .string [116, 97, 114, 103, 101, 116], .comma, .string [99, 97, 108, 108], .colon, .string [114, 111, 108, 101]]
def connectedAcc18 : List Token := (connectedLexemes18.reverse.map PlainLexeme.token).reverse ++ connectedAcc17
def connectedLexemes19 : List PlainLexeme := [.leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48], .colon, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .leftBrace, .leftBracket, .colon, .string [116, 121, 112, 101, 65, 114, 103, 117, 109, 101, 110, 116, 115], .comma, .rightBrace, .rightBrace, .string [48, 48, 48, 48, 48, 48, 49, 48], .colon, .string [108, 101, 110, 103, 116, 104], .comma, .string [48, 48, 48, 48, 48, 48, 49, 52], .colon, .string [115, 116, 97, 114, 116], .comma, .string [48, 48, 48, 48, 48, 48, 48, 48]]
def connectedAcc19 : List Token := (connectedLexemes19.reverse.map PlainLexeme.token).reverse ++ connectedAcc18
def connectedLexemes20 : List PlainLexeme := [.colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [101, 110, 97, 98, 108, 101, 100], .colon, .string [110, 97, 109, 101], .leftBrace, .comma, .rightBrace, .rightBrace, .string [105, 110, 116], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [97, 109, 111, 117, 110, 116], .colon, .string [110, 97, 109, 101], .leftBrace, .leftBracket, .colon, .string [102, 105, 101, 108, 100, 115], .comma, .string [114, 101, 99, 111, 114, 100], .colon, .string [107, 105, 110, 100]]
def connectedAcc20 : List Token := (connectedLexemes20.reverse.map PlainLexeme.token).reverse ++ connectedAcc19
def connectedLexemes21 : List PlainLexeme := [.rightBrace, .rightBracket, .rightBrace, .rightBracket, .string [102, 105, 101, 108, 100], .leftBracket, .colon, .string [111, 112, 101, 114, 97, 116, 105, 111, 110, 66, 105, 110, 100, 105, 110, 103, 115], .comma, .rightBracket, .rightBrace, .rightBrace, .string [105, 110, 116], .colon, .string [107, 105, 110, 100], .leftBrace, .colon, .string [116, 121, 112, 101], .comma, .string [48, 48, 48, 48, 48, 48, 48, 49], .colon, .string [112, 97, 114, 97, 109, 101, 116, 101, 114], .leftBrace, .comma, .rightBrace, .rightBrace, .rightBracket, .rightBrace, .rightBrace, .string [98, 111, 111, 108]]
def connectedAcc21 : List Token := (connectedLexemes21.reverse.map PlainLexeme.token).reverse ++ connectedAcc20
set_option maxRecDepth 100000 in
theorem connectedCodeChunks : connectedFactsCodes = connectedTail1 := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes1 : connectedChunk1 = connectedLexemes1.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid1 : connectedLexemes1.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition1 (suffix : List Nat) (tail : List Token) : lex (connectedChunk1 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes1.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes1, lexPrefix_plain_lexemes _ tail connectedChunkValid1]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes2 : connectedChunk2 = connectedLexemes2.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid2 : connectedLexemes2.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition2 (suffix : List Nat) (tail : List Token) : lex (connectedChunk2 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes2.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes2, lexPrefix_plain_lexemes _ tail connectedChunkValid2]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes3 : connectedChunk3 = connectedLexemes3.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid3 : connectedLexemes3.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition3 (suffix : List Nat) (tail : List Token) : lex (connectedChunk3 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes3.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes3, lexPrefix_plain_lexemes _ tail connectedChunkValid3]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes4 : connectedChunk4 = connectedLexemes4.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid4 : connectedLexemes4.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition4 (suffix : List Nat) (tail : List Token) : lex (connectedChunk4 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes4.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes4, lexPrefix_plain_lexemes _ tail connectedChunkValid4]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes5 : connectedChunk5 = connectedLexemes5.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid5 : connectedLexemes5.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition5 (suffix : List Nat) (tail : List Token) : lex (connectedChunk5 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes5.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes5, lexPrefix_plain_lexemes _ tail connectedChunkValid5]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes6 : connectedChunk6 = connectedLexemes6.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid6 : connectedLexemes6.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition6 (suffix : List Nat) (tail : List Token) : lex (connectedChunk6 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes6.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes6, lexPrefix_plain_lexemes _ tail connectedChunkValid6]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes7 : connectedChunk7 = connectedLexemes7.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid7 : connectedLexemes7.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition7 (suffix : List Nat) (tail : List Token) : lex (connectedChunk7 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes7.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes7, lexPrefix_plain_lexemes _ tail connectedChunkValid7]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes8 : connectedChunk8 = connectedLexemes8.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid8 : connectedLexemes8.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition8 (suffix : List Nat) (tail : List Token) : lex (connectedChunk8 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes8.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes8, lexPrefix_plain_lexemes _ tail connectedChunkValid8]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes9 : connectedChunk9 = connectedLexemes9.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid9 : connectedLexemes9.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition9 (suffix : List Nat) (tail : List Token) : lex (connectedChunk9 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes9.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes9, lexPrefix_plain_lexemes _ tail connectedChunkValid9]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes10 : connectedChunk10 = connectedLexemes10.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid10 : connectedLexemes10.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition10 (suffix : List Nat) (tail : List Token) : lex (connectedChunk10 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes10.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes10, lexPrefix_plain_lexemes _ tail connectedChunkValid10]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes11 : connectedChunk11 = connectedLexemes11.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid11 : connectedLexemes11.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition11 (suffix : List Nat) (tail : List Token) : lex (connectedChunk11 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes11.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes11, lexPrefix_plain_lexemes _ tail connectedChunkValid11]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes12 : connectedChunk12 = connectedLexemes12.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid12 : connectedLexemes12.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition12 (suffix : List Nat) (tail : List Token) : lex (connectedChunk12 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes12.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes12, lexPrefix_plain_lexemes _ tail connectedChunkValid12]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes13 : connectedChunk13 = connectedLexemes13.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid13 : connectedLexemes13.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition13 (suffix : List Nat) (tail : List Token) : lex (connectedChunk13 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes13.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes13, lexPrefix_plain_lexemes _ tail connectedChunkValid13]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes14 : connectedChunk14 = connectedLexemes14.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid14 : connectedLexemes14.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition14 (suffix : List Nat) (tail : List Token) : lex (connectedChunk14 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes14.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes14, lexPrefix_plain_lexemes _ tail connectedChunkValid14]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes15 : connectedChunk15 = connectedLexemes15.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid15 : connectedLexemes15.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition15 (suffix : List Nat) (tail : List Token) : lex (connectedChunk15 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes15.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes15, lexPrefix_plain_lexemes _ tail connectedChunkValid15]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes16 : connectedChunk16 = connectedLexemes16.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid16 : connectedLexemes16.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition16 (suffix : List Nat) (tail : List Token) : lex (connectedChunk16 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes16.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes16, lexPrefix_plain_lexemes _ tail connectedChunkValid16]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes17 : connectedChunk17 = connectedLexemes17.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid17 : connectedLexemes17.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition17 (suffix : List Nat) (tail : List Token) : lex (connectedChunk17 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes17.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes17, lexPrefix_plain_lexemes _ tail connectedChunkValid17]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes18 : connectedChunk18 = connectedLexemes18.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid18 : connectedLexemes18.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition18 (suffix : List Nat) (tail : List Token) : lex (connectedChunk18 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes18.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes18, lexPrefix_plain_lexemes _ tail connectedChunkValid18]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes19 : connectedChunk19 = connectedLexemes19.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid19 : connectedLexemes19.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition19 (suffix : List Nat) (tail : List Token) : lex (connectedChunk19 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes19.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes19, lexPrefix_plain_lexemes _ tail connectedChunkValid19]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes20 : connectedChunk20 = connectedLexemes20.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid20 : connectedLexemes20.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition20 (suffix : List Nat) (tail : List Token) : lex (connectedChunk20 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes20.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes20, lexPrefix_plain_lexemes _ tail connectedChunkValid20]
  rfl
set_option maxRecDepth 100000 in
theorem connectedChunkCodes21 : connectedChunk21 = connectedLexemes21.reverse.flatMap PlainLexeme.codes := by rfl
set_option maxRecDepth 100000 in
theorem connectedChunkValid21 : connectedLexemes21.reverse.all PlainLexeme.valid = true := by decide
theorem connectedLexTransition21 (suffix : List Nat) (tail : List Token) : lex (connectedChunk21 ++ suffix) .outside tail = lex suffix .outside ((connectedLexemes21.reverse.map PlainLexeme.token).reverse ++ tail) := by
  rw [lex_append_prefix, connectedChunkCodes21, lexPrefix_plain_lexemes _ tail connectedChunkValid21]
  rfl
set_option maxRecDepth 100000 in
theorem connectedLexResult : lex connectedFactsCodes .outside [] = some connectedAcc21.reverse := by
  rw [connectedCodeChunks]
  change lex (connectedChunk1 ++ connectedTail2) .outside [] = _
  rw [connectedLexTransition1]
  change lex (connectedChunk2 ++ connectedTail3) .outside connectedAcc1 = _
  rw [connectedLexTransition2]
  change lex (connectedChunk3 ++ connectedTail4) .outside connectedAcc2 = _
  rw [connectedLexTransition3]
  change lex (connectedChunk4 ++ connectedTail5) .outside connectedAcc3 = _
  rw [connectedLexTransition4]
  change lex (connectedChunk5 ++ connectedTail6) .outside connectedAcc4 = _
  rw [connectedLexTransition5]
  change lex (connectedChunk6 ++ connectedTail7) .outside connectedAcc5 = _
  rw [connectedLexTransition6]
  change lex (connectedChunk7 ++ connectedTail8) .outside connectedAcc6 = _
  rw [connectedLexTransition7]
  change lex (connectedChunk8 ++ connectedTail9) .outside connectedAcc7 = _
  rw [connectedLexTransition8]
  change lex (connectedChunk9 ++ connectedTail10) .outside connectedAcc8 = _
  rw [connectedLexTransition9]
  change lex (connectedChunk10 ++ connectedTail11) .outside connectedAcc9 = _
  rw [connectedLexTransition10]
  change lex (connectedChunk11 ++ connectedTail12) .outside connectedAcc10 = _
  rw [connectedLexTransition11]
  change lex (connectedChunk12 ++ connectedTail13) .outside connectedAcc11 = _
  rw [connectedLexTransition12]
  change lex (connectedChunk13 ++ connectedTail14) .outside connectedAcc12 = _
  rw [connectedLexTransition13]
  change lex (connectedChunk14 ++ connectedTail15) .outside connectedAcc13 = _
  rw [connectedLexTransition14]
  change lex (connectedChunk15 ++ connectedTail16) .outside connectedAcc14 = _
  rw [connectedLexTransition15]
  change lex (connectedChunk16 ++ connectedTail17) .outside connectedAcc15 = _
  rw [connectedLexTransition16]
  change lex (connectedChunk17 ++ connectedTail18) .outside connectedAcc16 = _
  rw [connectedLexTransition17]
  change lex (connectedChunk18 ++ connectedTail19) .outside connectedAcc17 = _
  rw [connectedLexTransition18]
  change lex (connectedChunk19 ++ connectedTail20) .outside connectedAcc18 = _
  rw [connectedLexTransition19]
  change lex (connectedChunk20 ++ connectedTail21) .outside connectedAcc19 = _
  rw [connectedLexTransition20]
  change lex (connectedChunk21 ++ []) .outside connectedAcc20 = _
  rw [connectedLexTransition21]
  rfl

end Xsh.Conformance.Connected
