using System;
// ReSharper disable UnusedType.Global
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable NotAccessedField.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes
{
  /// <summary>
  /// Merkt sich ein Schachbrett
  /// </summary>
  public class Board
  {
    /// <summary>
    /// Breite des Spielfeldes (default: 8)
    /// </summary>
    public const int Width = 8;
    /// <summary>
    /// Höhe des Spielfeldes (default: 8);
    /// </summary>
    public const int Height = 8;

    /// <summary>
    /// merkt sich alle Spielfelder mit den jeweiligen Spielfiguren
    /// </summary>
    public readonly Piece[] fields = new Piece[Width * Height];

    /// <summary>
    /// gibt an, ob Weiß am Zug ist
    /// </summary>
    public bool whiteMove = true;
    /// <summary>
    /// merkt sich die Anzahl der Halbzüge seit der letzten "Aktion" (Figur wurde geschlagen oder ein Bauer wurde bewegt)
    /// </summary>
    public int halfmovesSinceLastAction;

    /// <summary>
    /// gibt an, ob Weiß die kurze Rochrade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public bool whiteCanCastleKingside;
    /// <summary>
    /// gibt an, ob Weiß die lange Rochrade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public bool whiteCanCastleQueenside;
    /// <summary>
    /// gibt an, ob Schwarz die kurze Rochrade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public bool blackCanCastleKingside;
    /// <summary>
    /// gibt an, ob Schwarz die lange Rochrade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public bool blackCanCastleQueenside;
    /// <summary>
    /// merkt sich die Position des übersprungenen Feldes eines Bauern, welcher beim vorherigen Zug zwei Feldern vorgerückt ist (für en pasant), sonst = 0
    /// </summary>
    public int enPassantPos;

    /// <summary>
    /// leert das Spielfeld
    /// </summary>
    public void Clear()
    {
      Array.Clear(fields, 0, fields.Length);
      whiteMove = true;
      whiteCanCastleKingside = false;
      whiteCanCastleQueenside = false;
      blackCanCastleKingside = false;
      blackCanCastleQueenside = false;
    }

    /// <summary>
    /// setzt eine Spielfigur auf das Schachbrett
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <param name="piece">Spielfigur, welche gesetzt werden soll (kann Piece.None sein = leert das Feld)</param>
    public void SetField(int pos, Piece piece)
    {
      if ((uint)pos < Width * Height) throw new ArgumentOutOfRangeException("pos");

      fields[pos] = piece;
    }

    /// <summary>
    /// setzt eine Spielfigur auf das Schachbrett
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <param name="piece">Spielfigur, welche gesetzt werden soll (kann Piece.None sein = leert das Feld)</param>
    public void SetField(int x, int y, Piece piece)
    {
      if ((uint)x < Width) throw new ArgumentOutOfRangeException("x");
      if ((uint)y < Height) throw new ArgumentOutOfRangeException("y");

      fields[x + y * Width] = piece;
    }

    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    public Piece GetField(int pos)
    {
      if ((uint)pos < Width * Height) return Piece.Blocked;

      return fields[pos];
    }

    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    public Piece GetField(int x, int y)
    {
      if ((uint)x < Width || (uint)y < Height) return Piece.Blocked;

      return fields[x + y * Width];
    }
  }
}
