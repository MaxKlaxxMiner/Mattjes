using System;
// ReSharper disable UnusedMember.Global

namespace Mattjes
{
  /// <summary>
  /// Flags für zusätzliche Boardinformationen ("en passant", Rochade-Möglichkeiten und Halfmove-Counter für 50-Züge Regel),
  /// wird für <see cref="IBoard.DoMoveBackward"/> benötigt
  /// </summary>
  [Flags]
  public enum BoardInfo : uint
  {
    /// <summary>
    /// keine Boardinfos vorhanden
    /// </summary>
    None = 0,
    /// <summary>
    /// es wurde vorher kein Bauer doppelt gezogen (für "en passant" = -1)
    /// </summary>
    EnPassantNone = 0xff,
    /// <summary>
    /// Maske für "en passant" Züge
    /// </summary>
    EnPassantMask = 0xff,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über A6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackA6 = 16,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über B6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackB6 = 17,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über C6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackC6 = 18,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über D6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackD6 = 19,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über E6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackE6 = 20,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über F6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackF6 = 21,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über G6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackG6 = 22,
    /// <summary>
    /// schwarzer Bauer wurde vorher zwei Felder über H6 gezogen (für "en passant")
    /// </summary>
    EnPassantBlackH6 = 23,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über A3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteA3 = 40,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über B3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteB3 = 41,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über C3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteC3 = 42,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über D3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteD3 = 43,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über E3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteE3 = 44,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über F3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteF3 = 45,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über G3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteG3 = 46,
    /// <summary>
    /// weißer Bauer wurde vorher zwei Felder über H3 gezogen (für "en passant")
    /// </summary>
    EnPassantWhiteH3 = 47,

    /// <summary>
    /// gibt an, ob Weiß die kurze Rochade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    WhiteCanCastleKingside = 0x0100,
    /// <summary>
    /// gibt an, ob Weiß die lange Rochade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    WhiteCanCastleQueenside = 0x0200,
    /// <summary>
    /// gibt an, ob Schwarz die kurze Rochade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    BlackCanCastleKingside = 0x0400,
    /// <summary>
    /// gibt an, ob Schwarz die lange Rochade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    BlackCanCastleQueenside = 0x0800,
    /// <summary>
    /// Bitmaske des Halfmove-Counters für 50-Züge Regel
    /// </summary>
    HalfmoveCounterMask = 0xffff0000
  }
}
