using System;
using System.Collections.Generic;
using System.Linq;
// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Global
// ReSharper disable MemberCanBePrivate.Global

namespace Mattjes
{
  public static class BoardTools
  {
    /// <summary>
    /// gibt das Spielbrett in der Console aus (optional mit markierten Feldern)
    /// </summary>
    /// <param name="b">Spielbrett, welches ausgegeben werden soll</param>
    /// <param name="marker">optional markierte Felder</param>
    public static void PrintBoard(IBoard b, IEnumerable<int> marker = null)
    {
      if (marker == null) marker = Enumerable.Empty<int>();
      var tmp = b.ToString(marker, (char)0x1000);
      bool last = false;
      foreach (char c in tmp)
      {
        if ((c & 0x1000) != 0)
        {
          if (!last)
          {
            Console.ForegroundColor = ConsoleColor.Yellow;
            Console.BackgroundColor = ConsoleColor.DarkGray;
          }
          Console.Write((char)(c & 0xff));
          last = true;
        }
        else
        {
          if (last)
          {
            Console.ForegroundColor = ConsoleColor.Gray;
            Console.BackgroundColor = ConsoleColor.Black;
          }
          Console.Write(c);
          last = false;
        }
      }
    }

    /// <summary>
    /// prüft, ob eine Mattstellung erreicht wurde
    /// </summary>
    /// <param name="b">Spielbrett, welches geprüft werden soll</param>
    /// <returns>true, wenn es sich um ein Matt handelt</returns>
    public static bool IsMate(IBoard b)
    {
      if (b.HasMoves) return false; // sind noch Züge möglich? = kein Matt

      return b.IsChecked(b.GetKingPos(b.WhiteMove ? Piece.White : Piece.Black), b.WhiteMove ? Piece.Black : Piece.White); // steht der König im Schach?
    }

    /// <summary>
    /// gibt das passende ASCII-Zeichen zu einer Spielerfigur zurück
    /// </summary>
    /// <param name="piece">Spielefigur, welche abgefragt werden soll</param>
    /// <returns>passende Zeichen für die Spielerfigur</returns>
    public static char PieceChar(Piece piece)
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
    /// wandelt ein ASCII-Zeichen zu einer Spielfigur um
    /// </summary>
    /// <param name="c">Zeichen, welches eingelesen werden soll</param>
    /// <returns>fertige Spielfigur oder <see cref="Piece.Blocked"/> wenn ungültig</returns>
    public static Piece PieceFromChar(char c)
    {
      switch (c)
      {
        case '1':
        case '2':
        case '3':
        case '4':
        case '5':
        case '6':
        case '7':
        case '8': return Piece.None;
        case 'K': return Piece.WhiteKing;
        case 'k': return Piece.BlackKing;
        case 'Q': return Piece.WhiteQueen;
        case 'q': return Piece.BlackQueen;
        case 'R': return Piece.WhiteRook;
        case 'r': return Piece.BlackRook;
        case 'B': return Piece.WhiteBishop;
        case 'b': return Piece.BlackBishop;
        case 'N': return Piece.WhiteKnight;
        case 'n': return Piece.BlackKnight;
        case 'P': return Piece.WhitePawn;
        case 'p': return Piece.BlackPawn;
        default: return Piece.Blocked;
      }
    }

    /// <summary>
    /// gibt die Position als zweistellige FEN-Schreibweise zurück (z.B. "e4")
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Position als zweistellige FEN-Schreibweise</returns>
    public static string PosChars(int pos)
    {
      return PosChars(pos % IBoard.Width, pos / IBoard.Width);
    }

    /// <summary>
    /// gibt die Position als zweistellige FEN-Schreibweise zurück (z.B. "e4")
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <returns>Position als zweistellige FEN-Schreibweise</returns>
    public static string PosChars(int x, int y)
    {
      return ((char)(x + 'a')).ToString() + (IBoard.Height - y);
    }

    /// <summary>
    /// liest eine Position anhand einer zweistelligen FEN-Schreibweise ein (z.B. "e4")
    /// </summary>
    /// <param name="str">Zeichenfolge, welche eingelesen werden soll</param>
    /// <returns>absolute Position auf dem Spielfeld</returns>
    public static int PosFromChars(string str)
    {
      if (str.Length != 2) return -1; // nur zweistellige Positionen erlaubt
      if (str[0] < 'a' || str[0] - 'a' >= IBoard.Width) return -1; // ungültige Spaltenangabe ("a"-"h" erwartet)
      if (str[1] < '1' || str[1] - '1' >= IBoard.Height) return -1; // ungültige Zeilenangabe ("1"-"8" erwartet)
      return str[0] - 'a' + (IBoard.Height + '0' - str[1]) * IBoard.Width;
    }

    /// <summary>
    /// iteriert alle gültigen Königspositionen
    /// </summary>
    /// <returns>Enumerable aller Positions-Paara</returns>
    public static IEnumerable<KeyValuePair<byte, byte>> IterateKingPairs()
    {
      for (byte k1 = 0; k1 < IBoard.Width * IBoard.Height; k1++)
      {
        for (byte k2 = 0; k2 < IBoard.Width * IBoard.Height; k2++)
        {
          int dist = Math.Max(Math.Abs(k1 % IBoard.Width - k2 % IBoard.Width), Math.Abs(k1 / IBoard.Width - k2 / IBoard.Width));
          if (dist < 2) continue;
          yield return new KeyValuePair<byte, byte>(k1, k2);
        }
      }
    }

    /// <summary>
    /// gibt an, ob es sich um ein weißes Feld handelt (z.B. für Läufer wichtig)
    /// </summary>
    /// <param name="pos">Position, welche geprüft werden soll</param>
    /// <returns>true, wenn das Feld weiß ist, sonst false = schwarz</returns>
    public static bool IsWhiteField(int pos)
    {
      return (pos % IBoard.Width + pos / IBoard.Width) % 2 == 0;
    }
  }
}
