using System;
using System.Linq;
using System.Text;

// ReSharper disable UnusedType.Global
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable NotAccessedField.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes
{
  /// <summary>
  /// Merkt sich ein Schachbrett
  /// </summary>
  public sealed class Board
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
    /// aktuelle Spielzug Nummer (ganze Züge)
    /// </summary>
    public int moveNumber = 1;

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
      if ((uint)pos >= Width * Height) throw new ArgumentOutOfRangeException("pos");

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
      if ((uint)x >= Width) throw new ArgumentOutOfRangeException("x");
      if ((uint)y >= Height) throw new ArgumentOutOfRangeException("y");

      fields[x + y * Width] = piece;
    }

    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    public Piece GetField(int pos)
    {
      if ((uint)pos >= Width * Height) return Piece.Blocked;

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
      if ((uint)x >= Width || (uint)y >= Height) return Piece.Blocked;

      return fields[x + y * Width];
    }

    /// <summary>
    /// gibt das passende ASCII-Zeichen zu einer Spielerfigur zurück
    /// </summary>
    /// <param name="piece">Spielefigur, welche abgefragt werden soll</param>
    /// <returns>passende Zeichen für die Spielerfigur</returns>
    static char PieceChar(Piece piece)
    {
      bool w = (piece & Piece.White) == Piece.White;
      switch (piece & Piece.BasicPieces)
      {
        case Piece.King: return w ? 'K' : 'k';
        case Piece.Queen: return w ? 'Q' : 'q';
        case Piece.Rook: return w ? 'R' : 'r';
        case Piece.Bishop: return w ? 'B' : 'b';
        case Piece.Knight: return w ? 'N' : 'n';
        case Piece.Pawn: return w ? 'P' : 'p';
        default: return w ? '/' : '.';
      }
    }

    /// <summary>
    /// gibt die Position als zweistellige FEN-Schreibweise zurück (z.B. "e4")
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Position als zweistellige FEN-Schreibweise</returns>
    static string PosChars(int pos)
    {
      return PosChars(pos % Width, pos / Width);
    }

    /// <summary>
    /// gibt die Position als zweistellige FEN-Schreibweise zurück (z.B. "e4")
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <returns>Position als zweistellige FEN-Schreibweise</returns>
    static string PosChars(int x, int y)
    {
      return "abcdefgh"[x] + (Height - y).ToString();
    }

    /// <summary>
    /// gibt das Schachbrett als FEN-Schreibweise zurück
    /// </summary>
    /// <returns>FEN</returns>
    public string GetFEN()
    {
      return new string(Enumerable.Range(0, Height).SelectMany(y => Enumerable.Range(-1, Width + 1).Select(x => PieceChar(GetField(x, y)))).ToArray()).TrimStart('/')
        .Replace('.', '1').Replace("11", "2").Replace("22", "4").Replace("44", "8").Replace("42", "6").Replace("61", "7").Replace("41", "5").Replace("21", "3")
        + (whiteMove ? " w" : " b") + " "
        + (whiteCanCastleKingside ? "K" : "")
        + (whiteCanCastleQueenside ? "Q" : "")
        + (blackCanCastleKingside ? "k" : "")
        + (blackCanCastleQueenside ? "q" : "")
        + (!whiteCanCastleKingside && !whiteCanCastleQueenside && !blackCanCastleKingside && !blackCanCastleQueenside ? "-" : "") + " "
        + (enPassantPos == 0 ? "-" : PosChars(enPassantPos)) + " "
        + halfmovesSinceLastAction + " "
        + moveNumber
        ;
    }

    /// <summary>
    /// gibt das Spielfeld als lesbare Zeichenkette zurück
    /// </summary>
    /// <returns>lesbare Zeichenkette</returns>
    public override string ToString()
    {
      var sb = new StringBuilder();
      for (int y = 0; y < Height; y++)
      {
        sb.Append("    ");
        for (int x = 0; x < Width; x++)
        {
          sb.Append(PieceChar(GetField(x, y)));
        }
        sb.AppendLine();
      }
      return sb.ToString();
    }
  }
}
