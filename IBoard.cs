using System.Collections.Generic;
using System.Linq;
using System.Text;
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable UnusedMethodReturnValue.Global

namespace Mattjes
{
  /// <summary>
  /// Interface, welches die Funktionen eines Schachbrettes darstellt
  /// </summary>
  public abstract class IBoard
  {
    #region # // --- consts ---
    /// <summary>
    /// Breite des Spielfeldes (default: 8)
    /// </summary>
    public const int Width = 8;
    /// <summary>
    /// Höhe des Spielfeldes (default: 8);
    /// </summary>
    public const int Height = 8;
    #endregion

    #region # // --- IBoard Interface ---
    /// <summary>
    /// leert das Spielfeld
    /// </summary>
    public abstract void Clear();

    /// <summary>
    /// setzt eine Spielfigur auf das Schachbrett
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <param name="piece">Spielfigur, welche gesetzt werden soll (kann Piece.None sein = leert das Feld)</param>
    public abstract void SetField(int pos, Piece piece);

    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    public abstract Piece GetField(int pos);

    /// <summary>
    /// gibt an, ob Weiß am Zug ist
    /// </summary>
    public abstract bool WhiteMove { get; set; }
    /// <summary>
    /// Anzahl der Halbzüge seit der letzten "Aktion" (Figur wurde geschlagen oder ein Bauer wurde bewegt)
    /// </summary>
    public abstract int HalfmoveClock { get; set; }
    /// <summary>
    /// aktuelle Spielzug Nummer (ganze Züge)
    /// </summary>
    public abstract int MoveNumber { get; set; }

    /// <summary>
    /// gibt an, ob Weiß die kurze Rochrade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public abstract bool WhiteCanCastleKingside { get; set; }
    /// <summary>
    /// gibt an, ob Weiß die lange Rochrade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public abstract bool WhiteCanCastleQueenside { get; set; }
    /// <summary>
    /// gibt an, ob Schwarz die kurze Rochrade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public abstract bool BlackCanCastleKingside { get; set; }
    /// <summary>
    /// gibt an, ob Schwarz die lange Rochrade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public abstract bool BlackCanCastleQueenside { get; set; }
    /// <summary>
    /// Position des übersprungenen Feldes eines Bauern, welcher beim vorherigen Zug zwei Feldern vorgerückt ist (für "en pasant"), sonst = -1
    /// </summary>
    public abstract int EnPassantPos { get; set; }

    /// <summary>
    /// führt einen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    public abstract bool DoMove(Move move, bool onlyCheck = false);

    /// <summary>
    /// berechnet alle erlaubten Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    public abstract IEnumerable<Move> GetMoves();
    #endregion

    #region # // --- Helper Methods ---
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
      return PosChars(pos % Width, pos / Width);
    }

    /// <summary>
    /// gibt die Position als zweistellige FEN-Schreibweise zurück (z.B. "e4")
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <returns>Position als zweistellige FEN-Schreibweise</returns>
    public static string PosChars(int x, int y)
    {
      return ((char)(x + 'a')).ToString() + (Height - y);
    }

    /// <summary>
    /// liest eine Position anhand einer zweistelligen FEN-Schreibweise ein (z.B. "e4")
    /// </summary>
    /// <param name="str">Zeichenfolge, welche eingelesen werden soll</param>
    /// <returns>absolute Position auf dem Spielfeld</returns>
    public static int PosFromChars(string str)
    {
      if (str.Length != 2) return -1; // nur zweistellige Positionen erlaubt
      if (str[0] < 'a' || str[0] - 'a' >= Width) return -1; // ungültige Spaltenangabe ("a"-"h" erwartet)
      if (str[1] < '1' || str[1] - '1' >= Height) return -1; // ungültige Zeilenangabe ("1"-"8" erwartet)
      return str[0] - 'a' + (Height + '0' - str[1]) * Width;
    }
    #endregion

    #region # // --- FEN ---
    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="x">X-Position auf dem Schachbrett</param>
    /// <param name="y">Y-Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    Piece GetField(int x, int y)
    {
      if ((uint)x >= Width || (uint)y >= Height) return Piece.Blocked;

      return GetField(x + y * Width);
    }

    /// <summary>
    /// gibt das Schachbrett als FEN-Schreibweise zurück
    /// </summary>
    /// <returns>FEN</returns>
    public string GetFEN()
    {
      return new string(Enumerable.Range(0, Height).SelectMany(y => Enumerable.Range(-1, Width + 1).Select(x => PieceChar(GetField(x, y)))).ToArray()).TrimStart('/')
        .Replace('.', '1').Replace("11", "2").Replace("22", "4").Replace("44", "8").Replace("42", "6").Replace("61", "7").Replace("41", "5").Replace("21", "3")
        + (WhiteMove ? " w" : " b") + " "
        + (WhiteCanCastleKingside ? "K" : "")
        + (WhiteCanCastleQueenside ? "Q" : "")
        + (BlackCanCastleKingside ? "k" : "")
        + (BlackCanCastleQueenside ? "q" : "")
        + (!WhiteCanCastleKingside && !WhiteCanCastleQueenside && !BlackCanCastleKingside && !BlackCanCastleQueenside ? "-" : "") + " "
        + (EnPassantPos == -1 ? "-" : PosChars(EnPassantPos)) + " "
        + HalfmoveClock + " "
        + MoveNumber
        ;
    }

    /// <summary>
    /// setzt das komplette Schachbrett
    /// </summary>
    /// <param name="fen">FEN-Zeichenfolge</param>
    /// <returns>true, wenn die Stellung erfolgreich gesetzt werden konnte</returns>
    public bool SetFEN(string fen)
    {
      Clear();

      var splits = fen.Trim().Split(' ');
      if (splits.Length != 6) return false; // ungültige Anzahl der Elemente
      var lines = splits[0].Split('/');
      if (lines.Length != Height) return false; // Anzahl der Zeilen stimmt nicht überein

      // --- Figuren einlesen (1 / 6) ---
      for (int y = 0; y < lines.Length; y++)
      {
        int x = 0;
        foreach (char c in lines[y])
        {
          var p = PieceFromChar(c);
          if (p == Piece.Blocked) return false; // ungültiges Zeichen als Spielerfigur erkannt
          if (p == Piece.None) // leere Felder?
          {
            x += c - '0'; // alle leeren Felder überspringen
            continue;
          }
          SetField(x + y * Width, p);
          x++;
        }
        if (x != Width) return false; // Anzahl der Spalten ungültig
      }

      // --- wer ist am Zug? (2 / 6) ---
      switch (splits[1])
      {
        case "w": /* whiteMove = true; */ break;
        case "b": WhiteMove = false; break;
        default: return false; // ungültiger Wert
      }

      // --- Rochademöglichkeiten einlesen (3 / 6) ---
      switch (splits[2])
      {
        case "-": break;
        case "q": BlackCanCastleQueenside = true; break;
        case "k": BlackCanCastleKingside = true; break;
        case "kq": BlackCanCastleKingside = true; BlackCanCastleQueenside = true; break;
        case "Q": WhiteCanCastleQueenside = true; break;
        case "Qq": WhiteCanCastleQueenside = true; BlackCanCastleQueenside = true; break;
        case "Qk": WhiteCanCastleQueenside = true; BlackCanCastleKingside = true; break;
        case "Qkq": WhiteCanCastleQueenside = true; BlackCanCastleKingside = true; BlackCanCastleQueenside = true; break;
        case "K": WhiteCanCastleKingside = true; break;
        case "Kq": WhiteCanCastleKingside = true; BlackCanCastleQueenside = true; break;
        case "Kk": WhiteCanCastleKingside = true; BlackCanCastleKingside = true; break;
        case "Kkq": WhiteCanCastleKingside = true; BlackCanCastleKingside = true; BlackCanCastleQueenside = true; break;
        case "KQ": WhiteCanCastleKingside = true; WhiteCanCastleQueenside = true; break;
        case "KQq": WhiteCanCastleKingside = true; WhiteCanCastleQueenside = true; BlackCanCastleQueenside = true; break;
        case "KQk": WhiteCanCastleKingside = true; WhiteCanCastleQueenside = true; BlackCanCastleKingside = true; break;
        case "KQkq": WhiteCanCastleKingside = true; WhiteCanCastleQueenside = true; BlackCanCastleKingside = true; BlackCanCastleQueenside = true; break;
        default: return false; // ungültige Rochraden-Angabe
      }

      // --- "en passant" einlesen (4 / 6) ---
      EnPassantPos = PosFromChars(splits[3]);
      if (EnPassantPos == -1)
      {
        if (splits[3] != "-") return false; // ungültige Angabe
      }

      // --- Anzahl der Halbzüge einlesen, seit dem letzten Bauernzug oder Schlagen einer Figur (5 / 6) ---
      int tmp;
      if (!int.TryParse(splits[4], out tmp) || tmp < 0 || tmp > 999)
      {
        return false; // ungültige Zahl erkannt
      }
      HalfmoveClock = tmp;

      // --- Zugnummer einlesen (6 / 6) ---
      if (!int.TryParse(splits[5], out tmp) || tmp < 1 || tmp > 999)
      {
        return false; // ungültige Zahl erkannt
      }
      MoveNumber = tmp;

      return true;
    }
    #endregion

    #region # // --- ToString() ---
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
          sb.Append(PieceChar(GetField(x + y * Width)));
        }
        sb.AppendLine();
      }
      return sb.ToString();
    }

    /// <summary>
    /// gibt das Spielfeld als lesbare Zeichenkette zurück und markiert bestimmte Felder
    /// </summary>
    /// <param name="markerPosis">Position, welche markiert werden sollen</param>
    /// <param name="markerBit">Bit, welches für die Markierung verwendet werden soll (XOR)</param>
    /// <returns>lesbare Zeichenkette</returns>
    public string ToString(IEnumerable<int> markerPosis, char markerBit)
    {
      var marker = new HashSet<int>(markerPosis);
      var sb = new StringBuilder();
      for (int y = 0; y < Height; y++)
      {
        sb.Append("    ");
        for (int x = 0; x < Width; x++)
        {
          if (marker.Contains(x + y * Width))
          {
            sb.Append((char)(PieceChar(GetField(x + y * Width)) ^ markerBit));
          }
          else
          {
            sb.Append(PieceChar(GetField(x + y * Width)));
          }
        }
        sb.AppendLine();
      }
      return sb.ToString();
    }
    #endregion
  }
}
