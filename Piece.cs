using System;
// ReSharper disable UnusedMember.Global

namespace Mattjes
{
  /// <summary>
  /// Typen der Spielerfiguren
  /// </summary>
  [Flags]
  public enum Piece : byte
  {
    /// <summary>
    /// Weiße Spielerfigur
    /// </summary>
    White = 0x40,
    /// <summary>
    /// Schwarze Spielerfigur
    /// </summary>
    Black = 0x80,

    /// <summary>
    /// König
    /// </summary>
    King = 0x01,
    /// <summary>
    /// Dame
    /// </summary>
    Queen = 0x02,
    /// <summary>
    /// Turm
    /// </summary>
    Rook = 0x04,
    /// <summary>
    /// Läufer
    /// </summary>
    Bishop = 0x08,
    /// <summary>
    /// Springer
    /// </summary>
    Knight = 0x10,
    /// <summary>
    /// Bauer
    /// </summary>
    Pawn = 0x20,

    /// <summary>
    /// Weißer König
    /// </summary>
    WhiteKing = White | King,
    /// <summary>
    /// Weiße Dame
    /// </summary>
    WhiteQueen = White | Queen,
    /// <summary>
    /// Weißer Turm
    /// </summary>
    WhiteRook = White | Rook,
    /// <summary>
    /// Weißer Läufer
    /// </summary>
    WhiteBishop = White | Bishop,
    /// <summary>
    /// Weißer Springer
    /// </summary>
    WhiteKnight = White | Knight,
    /// <summary>
    /// Weißer Bauer
    /// </summary>
    WhitePawn = White | Pawn,

    /// <summary>
    /// Schwarzer König
    /// </summary>
    BlackKing = Black | King,
    /// <summary>
    /// Schwarze Dame
    /// </summary>
    BlackQueen = Black | Queen,
    /// <summary>
    /// Schwarzer Turm
    /// </summary>
    BlackRook = Black | Rook,
    /// <summary>
    /// Schwarzer Läufer
    /// </summary>
    BlackBishop = Black | Bishop,
    /// <summary>
    /// Schwarzer Springer
    /// </summary>
    BlackKnight = Black | Knight,
    /// <summary>
    /// Schwarzer Bauer
    /// </summary>
    BlackPawn = Black | Pawn
  }
}
