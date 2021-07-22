﻿using System;
using System.Linq;
using System.Text;
// ReSharper disable UnusedMethodReturnValue.Global

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
      halfmovesSinceLastAction = 0;
      moveNumber = 1;
      whiteCanCastleKingside = false;
      whiteCanCastleQueenside = false;
      blackCanCastleKingside = false;
      blackCanCastleQueenside = false;
      enPassantPos = 0;
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
    /// wandelt ein ASCII-Zeichen zu einer Spielfigur um
    /// </summary>
    /// <param name="c">Zeichen, welches eingelesen werden soll</param>
    /// <returns>fertige Spielfigur oder <see cref="Piece.Blocked"/> wenn ungültig</returns>
    static Piece PieceFromChar(char c)
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
      return ((char)(x + 'a')).ToString() + (Height - y);
    }

    /// <summary>
    /// liest eine Position anhand einer zweistelligen FEN-Schreibweise ein (z.B. "e4")
    /// </summary>
    /// <param name="str">Zeichenfolge, welche eingelesen werden soll</param>
    /// <returns></returns>
    static int PosFromChars(string str)
    {
      if (str.Length != 2) return -1; // nur zweistellige Positionen erlaubt
      if (str[0] < 'a' || str[0] - 'a' >= Width) return -1; // ungültige Spaltenangabe ("a"-"h" erwartet)
      if (str[1] < '1' || str[1] - '1' >= Height) return -1; // ungültige Zeilenangabe ("1"-"8" erwartet)
      return str[0] - 'a' + (Height + '0' - str[1]) * Width;
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
        default: return false; // ungültige Rohraden-Angaben (todo: shredderFEN implementieren für Chess960)
      }

      // --- "en passant" einlesen (4 / 6) ---
      enPassantPos = PosFromChars(splits[3]);
      if (enPassantPos < 0)
      {
        if (splits[3] == "-")
        {
          enPassantPos = 0; // kein "en passant" gesetzt -> gültig
        }
        else
        {
          return false; // ungültige Angabe
        }
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