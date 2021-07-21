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
    /// keine Spielfigur = leeres Feld
    /// </summary>
    None = 0x00,
    /// <summary>
    /// geblocktes Feld (außerhalb des Schachbrettes)
    /// </summary>
    Blocked = White | Black,

    /// <summary>
    /// weiße Spielerfigur
    /// </summary>
    White = 0x40,
    /// <summary>
    /// schwarze Spielerfigur
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
    /// weißer König
    /// </summary>
    WhiteKing = White | King,
    /// <summary>
    /// weiße Dame
    /// </summary>
    WhiteQueen = White | Queen,
    /// <summary>
    /// weißer Turm
    /// </summary>
    WhiteRook = White | Rook,
    /// <summary>
    /// weißer Läufer
    /// </summary>
    WhiteBishop = White | Bishop,
    /// <summary>
    /// weißer Springer
    /// </summary>
    WhiteKnight = White | Knight,
    /// <summary>
    /// weißer Bauer
    /// </summary>
    WhitePawn = White | Pawn,

    /// <summary>
    /// schwarzer König
    /// </summary>
    BlackKing = Black | King,
    /// <summary>
    /// schwarze Dame
    /// </summary>
    BlackQueen = Black | Queen,
    /// <summary>
    /// schwarzer Turm
    /// </summary>
    BlackRook = Black | Rook,
    /// <summary>
    /// schwarzer Läufer
    /// </summary>
    BlackBishop = Black | Bishop,
    /// <summary>
    /// schwarzer Springer
    /// </summary>
    BlackKnight = Black | Knight,
    /// <summary>
    /// schwarzer Bauer
    /// </summary>
    BlackPawn = Black | Pawn
  }
}
