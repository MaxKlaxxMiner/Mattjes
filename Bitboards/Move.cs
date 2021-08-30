using System;
// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Global
// ReSharper disable ShiftExpressionZeroLeftOperand

namespace Mattjes.Bitboards
{
  /// <summary>
  /// ein kompakter 16-Bit Zug (Bit-Identisch mit Stockfish)
  /// </summary>
  [Flags]
  public enum Move : ushort
  {
    /// <summary>
    /// Bitmaske für das Zielfeld (0-63)
    /// </summary>
    SquareMaskDest = 0x3f << 0,                        // Bits: ........ ..xxxxxx
    /// <summary>
    /// Bitmaske für das Quellfeld (0-63), Shift-6
    /// </summary>
    SquareMaskOrigin = 0x3f << 6,                      // Bits: ....xxxx xx......
    /// <summary>
    /// Bitmaske für die Quell und Zielfelder
    /// </summary>
    SquareMaskAll = SquareMaskDest | SquareMaskOrigin, // Bits: ....xxxx xxxxxxxx
    /// <summary>
    /// leerer Zug
    /// </summary>
    None = 0,                                          // Bits: 00000000 00000000
    /// <summary>
    /// Null-Zug (von Feld 1 zu Feld 1)
    /// </summary>
    Null = 1 << 6 | 1,                                 // Bits: ....0000 01000001
    /// <summary>
    /// Maske der Figur bei einer Bauernumwandlung, Shift-12
    /// </summary>
    PromotionMask = 0x3 << 12,                         // Bits: ..xx.... ........
    /// <summary>
    /// Bauernumwandlung zu einem Springer, Shift-12
    /// </summary>
    PromotionKnight = 0 << 12,                         // Bits: ..00.... ........
    /// <summary>
    /// Bauernumwandlung zu einem Läufer, Shift-12
    /// </summary>
    PromotionBishop = 1 << 12,                         // Bits: ..01.... ........
    /// <summary>
    /// Bauernumwandlung zu einem Turm, Shift-12
    /// </summary>
    PromotionRook = 2 << 12,                           // Bits: ..10.... ........
    /// <summary>
    /// Bauernumwandlung zu einer Dame, Shift-12
    /// </summary>
    PromotionQueen = 3 << 12,                          // Bits: ..11.... ........
    /// <summary>
    /// Bitmaske für die Zugtypen (0-3), Shift-14
    /// </summary>
    TypeMask = 0x3 << 14,                              // Bits: xx...... ........
    /// <summary>
    /// normaler Zug-Typ, Shift-14
    /// </summary>
    TypeNormal = 0 << 14,                              // Bits: 00...... ........
    /// <summary>
    /// Umwandlung eines Bauern, Shift-14
    /// </summary>
    TypePromotion = 1 << 14,                           // Bits: 01...... ........
    /// <summary>
    /// Bauernzug, welcher "en passant" ermöglicht, Shift-14
    /// </summary>
    TypeEnPassant = 2 << 14,                           // Bits: 10...... ........
    /// <summary>
    /// 
    /// </summary>
    TypeCastling = 3 << 14,                            // Bits: 11...... ........
  }
}
