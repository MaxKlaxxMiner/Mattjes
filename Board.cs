using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
// ReSharper disable UnusedMethodReturnValue.Global
// ReSharper disable UnusedType.Global
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable NotAccessedField.Global
// ReSharper disable UnusedMember.Global
// ReSharper disable ArrangeRedundantParentheses

namespace Mattjes
{
  /// <summary>
  /// Merkt sich ein Schachbrett
  /// </summary>
  public sealed class Board
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

    #region # // --- values ---
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
    /// merkt sich die Position des übersprungenen Feldes eines Bauern, welcher beim vorherigen Zug zwei Feldern vorgerückt ist (für en pasant), sonst = -1
    /// </summary>
    public int enPassantPos = -1;
    #endregion

    #region # // --- SetField / GetField / Clear ---
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
    /// <returns>true, wenn erfolgreich</returns>
    public bool SetField(int x, int y, Piece piece)
    {
      if ((uint)x >= Width || (uint)y >= Height) return false;

      fields[x + y * Width] = piece;
      return true;
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
    /// leert das Spielfeld
    /// </summary>
    public void Clear()
    {
      Array.Clear(fields, 0, fields.Length);
      whiteMove = true;
      halfmovesSinceLastAction = 0;
      moveNumber = 1;
      whiteCanCastleKingside = false;
      whiteCanCastleQueenside = false;
      blackCanCastleKingside = false;
      blackCanCastleQueenside = false;
      enPassantPos = -1;
    }
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
        + (enPassantPos == -1 ? "-" : PosChars(enPassantPos)) + " "
        + halfmovesSinceLastAction + " "
        + moveNumber
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
          SetField(x, y, p);
          x++;
        }
        if (x != Width) return false; // Anzahl der Spalten ungültig
      }

      // --- wer ist am Zug? (2 / 6) ---
      switch (splits[1])
      {
        case "w": /* whiteMove = true; */ break;
        case "b": whiteMove = false; break;
        default: return false; // ungültiger Wert
      }

      // --- Rochademöglichkeiten einlesen (3 / 6) ---
      switch (splits[2])
      {
        case "-": break;
        case "q": blackCanCastleQueenside = true; break;
        case "k": blackCanCastleKingside = true; break;
        case "kq": blackCanCastleKingside = blackCanCastleQueenside = true; break;
        case "Q": whiteCanCastleQueenside = true; break;
        case "Qq": whiteCanCastleQueenside = blackCanCastleQueenside = true; break;
        case "Qk": whiteCanCastleQueenside = blackCanCastleKingside = true; break;
        case "Qkq": whiteCanCastleQueenside = blackCanCastleKingside = blackCanCastleQueenside = true; break;
        case "K": whiteCanCastleKingside = true; break;
        case "Kq": whiteCanCastleKingside = blackCanCastleQueenside = true; break;
        case "Kk": whiteCanCastleKingside = blackCanCastleKingside = true; break;
        case "Kkq": whiteCanCastleKingside = blackCanCastleKingside = blackCanCastleQueenside = true; break;
        case "KQ": whiteCanCastleKingside = whiteCanCastleQueenside = true; break;
        case "KQq": whiteCanCastleKingside = whiteCanCastleQueenside = blackCanCastleQueenside = true; break;
        case "KQk": whiteCanCastleKingside = whiteCanCastleQueenside = blackCanCastleKingside = true; break;
        case "KQkq": whiteCanCastleKingside = whiteCanCastleQueenside = blackCanCastleKingside = blackCanCastleQueenside = true; break;
        default: return false; // ungültige Rochraden-Angabe
      }

      // --- "en passant" einlesen (4 / 6) ---
      enPassantPos = PosFromChars(splits[3]);
      if (enPassantPos == -1)
      {
        if (splits[3] != "-") return false; // ungültige Angabe
      }

      // --- Anzahl der Halbzüge einlesen, seit dem letzten Bauernzug oder Schlagen einer Figur (5 / 6) ---
      if (!int.TryParse(splits[4], out halfmovesSinceLastAction) || halfmovesSinceLastAction < 0 || halfmovesSinceLastAction > 999)
      {
        return false; // ungültige Zahl erkannt
      }

      // --- Zugnummer einlesen (6 / 6) ---
      if (!int.TryParse(splits[5], out moveNumber) || moveNumber < 1 || moveNumber > 999)
      {
        return false; // ungültige Zahl erkannt
      }

      return true;
    }
    #endregion

    /// <summary>
    /// prüft die theoretischen Bewegungsmöglichkeiten einer Spielfigur auf einem bestimmten Feld
    /// </summary>
    /// <param name="pos">Position auf dem Spielfeld mit der zu testenden Figur</param>
    /// <returns>Aufzählung der theretisch begehbaren Felder</returns>
    public IEnumerable<int> ScanMove(int pos)
    {
      var piece = fields[pos];
      if (piece == Piece.None) yield break; // keine Figur auf dem Spielfeld?
      var color = piece & Piece.Colors;
      Debug.Assert(color == (whiteMove ? Piece.White : Piece.Black)); // passt die Figur-Farbe zum Zug?

      int posX = pos % Width;
      int posY = pos / Width;
      switch (piece & Piece.BasicPieces)
      {
        #region # case Piece.King: // König
        case Piece.King:
        {
          if (posX > 0) // nach links
          {
            if (posY > 0 && (fields[pos - (Width + 1)] & color) == Piece.None) yield return pos - (Width + 1); // links-oben
            if ((fields[pos - 1] & color) == Piece.None) yield return pos - 1; // links
            if (posY < Height - 1 && (fields[pos + (Width - 1)] & color) == Piece.None) yield return pos + (Width - 1); // links-unten
          }
          if (posX < Width - 1) // nach rechts
          {
            if (posY > 0 && (fields[pos - (Width - 1)] & color) == Piece.None) yield return pos - (Width - 1); // rechts-oben
            if ((fields[pos + 1] & color) == Piece.None) yield return pos + 1; // rechts
            if (posY < Height - 1 && (fields[pos + (Width + 1)] & color) == Piece.None) yield return pos + (Width + 1); // rechts-unten
          }
          if (posY > 0 && (fields[pos - Width] & color) == Piece.None) yield return pos - Width; // oben
          if (posY < Height - 1 && (fields[pos + Width] & color) == Piece.None) yield return pos + Width; // unten
        } break;
        #endregion

        #region # case Piece.Queen: // Dame
        case Piece.Queen:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // links-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY - i < 0) break;
            int p = pos - (Width * i + i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // links-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY + i >= Height) break;
            int p = pos + (Width * i - i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY - i < 0) break;
            int p = pos - (Width * i - i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY + i >= Height) break;
            int p = pos + (Width * i + i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.Rook: // Turm
        case Piece.Rook:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.Bishop: // Läufer
        case Piece.Bishop:
        {
          // links-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY - i < 0) break;
            int p = pos - (Width * i + i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // links-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY + i >= Height) break;
            int p = pos + (Width * i - i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY - i < 0) break;
            int p = pos - (Width * i - i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY + i >= Height) break;
            int p = pos + (Width * i + i);
            var f = fields[p];
            if ((f & color) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.Knight: // Springer
        case Piece.Knight:
        {
          if (posX > 0) // 1 nach links
          {
            if (posY > 1 && (fields[pos - (Width * 2 + 1)] & color) == Piece.None) yield return pos - (Width * 2 + 1); // -1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 - 1)] & color) == Piece.None) yield return pos + (Width * 2 - 1); // -1, +2
            if (posX > 1) // 2 nach links
            {
              if (posY > 0 && (fields[pos - (Width + 2)] & color) == Piece.None) yield return pos - (Width + 2); // -2, -1
              if (posY < Height - 1 && (fields[pos + (Width - 2)] & color) == Piece.None) yield return pos + (Width - 2); // -2, +1
            }
          }
          if (posX < Width - 1) // 1 nach rechts
          {
            if (posY > 1 && (fields[pos - (Width * 2 - 1)] & color) == Piece.None) yield return pos - (Width * 2 - 1); // +1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 + 1)] & color) == Piece.None) yield return pos + (Width * 2 + 1); // +1, +2
            if (posX < Width - 2) // 2 nach rechts
            {
              if (posY > 0 && (fields[pos - (Width - 2)] & color) == Piece.None) yield return pos - (Width - 2); // +2, +1
              if (posY < Height - 1 && (fields[pos + (Width + 2)] & color) == Piece.None) yield return pos + (Width + 2); // +2, -1
            }
          }
        } break;
        #endregion

        #region # case Piece.Pawn: // Bauer
        case Piece.Pawn:
        {
          if (posY < 1 || posY >= Height - 1) break; // ungültige Position

          if (color == Piece.White) // weißer Bauer = nach oben laufen
          {
            if (fields[pos - Width] == Piece.None) // Laufweg frei?
            {
              yield return pos - Width;
              if (posY == 6 && fields[pos - Width * 2] == Piece.None) yield return pos - Width * 2; // Doppelzug
            }
            if (posX > 0 && (enPassantPos == pos - (Width + 1) || (fields[pos - (Width + 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width + 1); // nach links-oben schlagen
            if (posX < Width - 1 && (enPassantPos == pos - (Width - 1) || (fields[pos - (Width - 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width - 1); // nach rechts-oben schlagen
          }
          else // schwarzer Bauer = nach unten laufen
          {
            if (fields[pos + Width] == Piece.None) // Laufweg frei?
            {
              yield return pos + Width;
              if (posY == 1 && fields[pos + Width * 2] == Piece.None) yield return pos + Width * 2; // Doppelzug
            }
            if (posX > 0 && (enPassantPos == pos + (Width - 1) || (fields[pos + (Width - 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width - 1); // nach links-unten schlagen
            if (posX < Width - 1 && (enPassantPos == pos + (Width + 1) || (fields[pos + (Width + 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width + 1); // nach rechts-unten schlagen
          }
        } break;
        #endregion
      }
    }

    /// <summary>
    /// prüft, ob ein bestimmtes Spielfeld unter Schach steht
    /// </summary>
    /// <param name="pos">Position, welche geprüft werden soll</param>
    /// <param name="checkerColor">zu prüfende Spielerfarbe, welche das Schach geben könnte (nur <see cref="Piece.White"/> oder <see cref="Piece.Black"/> erlaubt)</param>
    /// <returns>true, wenn das Feld angegriffen wird und unter Schach steht</returns>
    public bool IsChecked(int pos, Piece checkerColor)
    {
      int posX = pos % Width;
      int posY = pos / Width;

      // --- Bauern und König prüfen ---
      if (checkerColor == Piece.White)
      {
        if (posX > 0) // nach links
        {
          if (posY > 0 && fields[pos - (Width + 1)] == Piece.WhiteKing) return true; // links-oben
          if (fields[pos - 1] == Piece.WhiteKing) return true; // links
          if (posY < Height - 1 && (fields[pos + (Width - 1)] == Piece.WhiteKing || fields[pos + (Width - 1)] == Piece.WhitePawn)) return true; // links-unten
        }
        if (posX < Width - 1) // nach rechts
        {
          if (posY > 0 && fields[pos - (Width - 1)] == Piece.WhiteKing) return true; // rechts-oben
          if (fields[pos + 1] == Piece.WhiteKing) return true; // rechts
          if (posY < Height - 1 && (fields[pos + (Width + 1)] == Piece.WhiteKing || fields[pos + (Width + 1)] == Piece.WhitePawn)) return true; // rechts-unten
        }
        if (posY > 0 && fields[pos - Width] == Piece.WhiteKing) return true; // oben
        if (posY < Height - 1 && fields[pos + Width] == Piece.WhiteKing) return true; // unten
      }
      else
      {
        if (posX > 0) // nach links
        {
          if (posY > 0 && (fields[pos - (Width + 1)] == Piece.BlackKing || fields[pos - (Width + 1)] == Piece.BlackPawn)) return true; // links-oben
          if (fields[pos - 1] == Piece.BlackKing) return true; // links
          if (posY < Height - 1 && fields[pos + (Width - 1)] == Piece.BlackKing) return true; // links-unten
        }
        if (posX < Width - 1) // nach rechts
        {
          if (posY > 0 && (fields[pos - (Width - 1)] == Piece.BlackKing || fields[pos - (Width - 1)] == Piece.BlackPawn)) return true; // rechts-oben
          if (fields[pos + 1] == Piece.BlackKing) return true; // rechts
          if (posY < Height - 1 && fields[pos + (Width + 1)] == Piece.BlackKing) return true; // rechts-unten
        }
        if (posY > 0 && fields[pos - Width] == Piece.BlackKing) return true; // oben
        if (posY < Height - 1 && fields[pos + Width] == Piece.BlackKing) return true; // unten
      }

      // --- Springer prüfen ---
      {
        var knight = checkerColor | Piece.Knight;
        if (posX > 0) // 1 nach links
        {
          if (posY > 1 && fields[pos - (Width * 2 + 1)] == knight) return true; // -1, -2
          if (posY < Height - 2 && fields[pos + (Width * 2 - 1)] == knight) return true; // -1, +2
          if (posX > 1) // 2 nach links
          {
            if (posY > 0 && fields[pos - (Width + 2)] == knight) return true; // -2, -1
            if (posY < Height - 1 && fields[pos + (Width - 2)] == knight) return true; // -2, +1
          }
        }
        if (posX < Width - 1) // 1 nach rechts
        {
          if (posY > 1 && fields[pos - (Width * 2 - 1)] == knight) return true; // +1, -2
          if (posY < Height - 2 && fields[pos + (Width * 2 + 1)] == knight) return true; // +1, +2
          if (posX < Width - 2) // 2 nach rechts
          {
            if (posY > 0 && fields[pos - (Width - 2)] == knight) return true; // +2, +1
            if (posY < Height - 1 && fields[pos + (Width + 2)] == knight) return true; // +2, -1
          }
        }
      }

      // --- horizontale und vertikale Wege prüfen ---
      {
        for (int i = 1; i < Width; i++) // links
        {
          if (posX - i < 0) break;
          var f = fields[pos - i];
          if (f == Piece.None) continue;
          if ((f & (Piece.Rook | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Width; i++) // rechts
        {
          if (posX + i >= Width) break;
          var f = fields[pos + i];
          if (f == Piece.None) continue;
          if ((f & (Piece.Rook | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Height; i++) // oben
        {
          if (posY - i < 0) break;
          var f = fields[pos - Width * i];
          if (f == Piece.None) continue;
          if ((f & (Piece.Rook | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Height; i++) // unten
        {
          if (posY + i >= Height) break;
          var f = fields[pos + Width * i];
          if (f == Piece.None) continue;
          if ((f & (Piece.Rook | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
      }

      // --- diagonale Wege prüfen ---
      {
        for (int i = 1; i < Math.Max(Width, Height); i++) // links-oben
        {
          if (posX - i < 0 || posY - i < 0) break;
          var f = fields[pos - (Width * i + i)];
          if (f == Piece.None) continue;
          if ((f & (Piece.Bishop | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Math.Max(Width, Height); i++) // links-unten
        {
          if (posX - i < 0 || posY + i >= Height) break;
          var f = fields[pos + (Width * i - i)];
          if (f == Piece.None) continue;
          if ((f & (Piece.Bishop | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Math.Max(Width, Height); i++) // rechts-oben
        {
          if (posX + i >= Width || posY - i < 0) break;
          var f = fields[pos - (Width * i - i)];
          if (f == Piece.None) continue;
          if ((f & (Piece.Bishop | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
        for (int i = 1; i < Math.Max(Width, Height); i++) // rechts-unten
        {
          if (posX + i >= Width || posY + i >= Height) break;
          var f = fields[pos + (Width * i + i)];
          if (f == Piece.None) continue;
          if ((f & (Piece.Bishop | Piece.Queen)) != Piece.None && (f & checkerColor) != Piece.None) return true;
          break;
        }
      }

      return false;
    }

    /// <summary>
    /// führt einen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    public bool DoMove(Move move, bool onlyCheck = false)
    {
      var piece = fields[move.fromPos];

      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // ist eine Figur auf dem Feld vorhanden?
      Debug.Assert(fields[move.toPos] == move.capturePiece); // stimmt die zu schlagende Figur mit dem Spielfeld überein?
      Debug.Assert((move.capturePiece & Piece.Colors) != (piece & Piece.Colors)); // wird keine eigene Figur gleicher Farbe geschlagen?
      Debug.Assert((piece & Piece.Colors) == (whiteMove ? Piece.White : Piece.Black)); // passt die Figur-Farbe zum Zug?

      // --- Zug durchführen ---
      fields[move.toPos] = piece;
      fields[move.fromPos] = Piece.None;

      if (move.toPos == enPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer schlägt "en passant"?
      {
        Debug.Assert(move.toPos % Width != move.fromPos % Width); // Spalte muss sich ändern
        Debug.Assert(move.capturePiece == Piece.None); // das Zielfeld enhält keine Figur (der geschlagene Bauer ist drüber oder drunter)
        int removePawnPos = whiteMove ? move.toPos + Width : move.toPos - Width; // Position des zu schlagenden Bauern berechnen
        Debug.Assert(fields[removePawnPos] == (whiteMove ? Piece.BlackPawn : Piece.WhitePawn)); // es wird ein Bauer erwartet, welcher geschlagen wird
        fields[removePawnPos] = Piece.None; // Bauer entfernen
      }

      if (move.promoPiece != Piece.None) fields[move.toPos] = move.promoPiece;

      // --- prüfen, ob der König nach dem Zug im Schach steht ---
      var searchKing = (piece & Piece.Colors) | Piece.King;
      for (int kingPos = 0; kingPos < fields.Length; kingPos++)
      {
        if (fields[kingPos] == searchKing) // König gefunden?
        {
          if (!onlyCheck) // wird ein echter Zug durchgeführt?
          {
            if (kingPos == move.toPos && Math.Abs(move.toPos - move.fromPos) == 2) // wurde der König mit einer Rochade bewegt (zwei Felder seitlich)?
            {
              switch (kingPos)
              {
                case 2: // lange Rochade mit dem schwarzen König
                {
                  Debug.Assert(blackCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[0] == Piece.BlackRook && fields[1] == Piece.None && fields[2] == Piece.BlackKing && fields[3] == Piece.None && fields[4] == Piece.None); // Felder prüfen
                  fields[0] = Piece.None; fields[3] = Piece.BlackRook; // Turm bewegen
                } break;
                case 6: // kurze Rochade mit dem schwarzen König
                {
                  Debug.Assert(blackCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[4] == Piece.None && fields[5] == Piece.None && fields[6] == Piece.BlackKing && fields[7] == Piece.BlackRook); // Felder prüfen
                  fields[7] = Piece.None; fields[5] = Piece.BlackRook; // Turm bewegen
                } break;
                case 58: // lange Rochade mit dem weißen König
                {
                  Debug.Assert(whiteCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[56] == Piece.WhiteRook && fields[57] == Piece.None && fields[58] == Piece.WhiteKing && fields[59] == Piece.None && fields[60] == Piece.None); // Felder prüfen
                  fields[56] = Piece.None; fields[59] = Piece.WhiteRook; // Turm bewegen
                } break;
                case 62: // kurze Rochade mit dem weißen König
                {
                  Debug.Assert(whiteCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[60] == Piece.None && fields[61] == Piece.None && fields[62] == Piece.WhiteKing && fields[63] == Piece.WhiteRook); // Felder prüfen
                  fields[63] = Piece.None; fields[61] = Piece.WhiteRook; // Turm bewegen
                } break;
                default: throw new Exception(); // Rochade war unmöglich
              }
              break; // weiterer Schach-Checks nach Rochade nicht notwendig
            }
          }

          if (IsChecked(kingPos, whiteMove ? Piece.Black : Piece.White)) // prüfen, ob der eigene König vom Gegner angegriffen wird und noch im Schach steht
          {
            // --- Zug rückgängig machen ---
            fields[move.toPos] = move.capturePiece;
            fields[move.fromPos] = piece;
            if (move.toPos == enPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer hat "en passant" geschlagen?
            {
              if (whiteMove)
              {
                fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
              }
              else
              {
                fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
              }
            }
            return false; // Zug war nicht erlaubt, da der König sonst im Schach stehen würde
          }
          break;
        }
      }

      if (onlyCheck) // Zug sollte nur geprüft werden?
      {
        // --- Zug rückgängig machen ---
        fields[move.toPos] = move.capturePiece;
        fields[move.fromPos] = piece;
        if (move.toPos == enPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer hat "en passant" geschlagen?
        {
          if (whiteMove)
          {
            fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
          }
          else
          {
            fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
          }
        }
        return true;
      }

      enPassantPos = -1;
      if ((piece & Piece.Pawn) != Piece.None && Math.Abs(move.toPos - move.fromPos) == Width * 2) // wurde ein Bauer zwei Felder weit gezogen -> "en passant" vormerken
      {
        enPassantPos = (move.fromPos + move.toPos) / 2;
        int posX = enPassantPos % Width;
        bool opPawn = false;
        if (whiteMove)
        {
          if (posX > 0 && fields[enPassantPos - Width - 1] == Piece.BlackPawn) opPawn = true;
          if (posX < Width - 1 && fields[enPassantPos - Width + 1] == Piece.BlackPawn) opPawn = true;
        }
        else
        {
          if (posX > 0 && fields[enPassantPos + Width - 1] == Piece.WhitePawn) opPawn = true;
          if (posX < Width - 1 && fields[enPassantPos + Width + 1] == Piece.WhitePawn) opPawn = true;
        }
        if (!opPawn) enPassantPos = -1; // kein "en passant" möglich, da kein gegenerischer Bauer in der Nähe ist
      }

      // prüfen, ob durch den Zug Rochaden ungültig werden
      switch (move.fromPos)
      {
        case 0: blackCanCastleQueenside = false; break; // linker schwarzer Turm wurde mindestens das erste Mal bewegt
        case 4: blackCanCastleQueenside = blackCanCastleKingside = false; break; // schwarzer König wurde mindestens das erste Mal bewegt
        case 7: blackCanCastleKingside = false; break; // rechter schwarzer Turm wurde mindestens das erste Mal bewegt
        case 56: whiteCanCastleQueenside = false; break; // linker weißer Turm wurde mindestens das erste Mal bewegt
        case 60: whiteCanCastleQueenside = whiteCanCastleKingside = false; break; // weißer König wurde mindestens das erste Mal bewegt
        case 63: whiteCanCastleKingside = false; break; // rechter weißer Turm wurde mindestens das erste Mal bewegt
      }

      whiteMove = !whiteMove; // Farbe welchseln, damit der andere Spieler am Zug ist
      halfmovesSinceLastAction++;
      if (piece == Piece.Pawn || move.capturePiece != Piece.None) halfmovesSinceLastAction = 0; // beim Bauernzug oder Schlagen einer Figur: 50-Züge Regel zurücksetzen
      if (whiteMove) moveNumber++; // Züge weiter hochzählen

      return true;
    }

    /// <summary>
    /// berechnet alle erlaubten Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    public IEnumerable<Move> GetMoves()
    {
      var color = whiteMove ? Piece.White : Piece.Black;

      for (int pos = 0; pos < fields.Length; pos++)
      {
        var piece = fields[pos];
        if ((piece & Piece.Colors) != color) continue; // Farbe der Figur passt nicht zum Zug oder das Feld ist leer

        if ((piece & Piece.Pawn) != Piece.None && ((pos < Width * 2 && whiteMove) || (pos >= Height * Width - Width * 2 && !whiteMove)))
        {
          // Promotion-Zug gefunden? (ein Bauer hat das Ziel erreicht und wird umgewandelt)
          foreach (int movePos in ScanMove(pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
          {
            var move = new Move(pos, movePos, fields[movePos], color | Piece.Queen);
            if (DoMove(move, true)) // gültigen Zug gefunden?
            {
              yield return move;
              // weitere Wahlmöglichkeiten als gültige Züge zurück geben
              move.promoPiece = color | Piece.Rook;
              yield return move;
              move.promoPiece = color | Piece.Bishop;
              yield return move;
              move.promoPiece = color | Piece.Knight;
              yield return move;
            }
          }
        }
        else
        {
          foreach (int movePos in ScanMove(pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
          {
            var move = new Move(pos, movePos, fields[movePos], Piece.None);
            if (DoMove(move, true)) // gültigen Zug gefunden?
            {
              yield return move;
            }
          }

          // Rochade-Züge prüfen
          if (pos == 60 && piece == Piece.WhiteKing) // der weiße König steht noch auf der Startposition?
          {
            if (whiteCanCastleQueenside // lange Rochade O-O-O möglich?
                && fields[57] == Piece.None && fields[58] == Piece.None && fields[59] == Piece.None // sind die Felder noch frei?
                && !IsChecked(58, Piece.Black) && !IsChecked(59, Piece.Black) && !IsChecked(60, Piece.Black)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[56] == Piece.WhiteRook); // weißer Turm sollte noch in der Ecke stehen
              yield return new Move(pos, pos - 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
            if (whiteCanCastleKingside // kurze Rochade O-O möglich?
                && fields[61] == Piece.None && fields[62] == Piece.None // sind die Felder noch frei?
                && !IsChecked(60, Piece.Black) && !IsChecked(61, Piece.Black) && !IsChecked(62, Piece.Black)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[63] == Piece.WhiteRook); // weißer Turm solle noch in der Ecke stehen
              yield return new Move(pos, pos + 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
          }
          else if (pos == 4 && piece == Piece.BlackKing) // der weiße König steht noch auf der Startposition?
          {
            if (blackCanCastleQueenside // lange Rochade O-O-O möglich?
                && fields[1] == Piece.None && fields[2] == Piece.None && fields[3] == Piece.None // sind die Felder noch frei?
                && !IsChecked(2, Piece.White) && !IsChecked(3, Piece.White) && !IsChecked(4, Piece.White)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[0] == Piece.BlackRook); // schwarzer Turm sollte noch in der Ecke stehen
              yield return new Move(pos, pos - 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
            if (blackCanCastleKingside // kurze Rochade O-O möglich?
                && fields[5] == Piece.None && fields[6] == Piece.None // sind die Felder noch frei?
                && !IsChecked(4, Piece.White) && !IsChecked(5, Piece.White) && !IsChecked(6, Piece.White)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[7] == Piece.BlackRook); // schwarzer Turm solle noch in der Ecke stehen
              yield return new Move(pos, pos + 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
          }
        }
      }
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
            sb.Append((char)(PieceChar(GetField(x, y)) ^ markerBit));
          }
          else
          {
            sb.Append(PieceChar(GetField(x, y)));
          }
        }
        sb.AppendLine();
      }
      return sb.ToString();
    }
  }
}
