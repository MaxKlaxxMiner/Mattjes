﻿using System;
using System.Collections.Generic;
using System.Diagnostics;
// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Local

namespace Mattjes
{
  /// <summary>
  /// Merkt sich ein Schachbrett
  /// </summary>
  public sealed class BoardIndexed : IBoard
  {
    #region # // --- values ---
    /// <summary>
    /// merkt sich alle Spielfelder mit den jeweiligen Spielfiguren
    /// </summary>
    readonly Piece[] fields = new Piece[Width * Height];

    /// <summary>
    /// merkt sich die Positionen der weißen Schachfiguren
    /// </summary>
    readonly ushort[] whiteIndex = new ushort[Width * 2];
    /// <summary>
    /// merkt sich die Anzahl der weißen Figuren auf dem Schachbrett
    /// </summary>
    int whitePieceCount;

    /// <summary>
    /// merkt sich die Positionen der schwarzen Schachfiguren
    /// </summary>
    readonly ushort[] blackIndex = new ushort[Height * 2];
    /// <summary>
    /// merkt sich die Anzahl der schwarzen Figuren auf dem Schachbrett
    /// </summary>
    int blackPieceCount;
    #endregion

    #region # // --- Index-Funktionen ---
    /// <summary>
    /// berechnet den Index-Schlüssel
    /// </summary>
    /// <param name="piece">Figur, welche verrechnet werden soll</param>
    /// <param name="pos">zugehörige Position der Figur</param>
    /// <returns>berechneter Schlüssel</returns>
    static ushort IndexKey(Piece piece, int pos)
    {
      return (ushort)((byte)piece << 8 | pos);
    }

    /// <summary>
    /// fügt ein Element sortiert einem Index hinzu
    /// </summary>
    /// <param name="index">Index, welches befüllt werden soll</param>
    /// <param name="length">befüllte Länge vom Index</param>
    /// <param name="key">Schlüssel, welcher hinzugefügt werden soll</param>
    static void IndexAdd(ushort[] index, int length, ushort key)
    {
      while (length > 0 && key < index[length - 1])
      {
        index[length] = index[length - 1];
        length--;
      }
      index[length] = key;
    }

    /// <summary>
    /// entfernt ein Element wieder aus dem Index
    /// </summary>
    /// <param name="index">Index, welches geleert werden soll</param>
    /// <param name="length">befüllte Länge vom Index</param>
    /// <param name="key">Schlüssel, welcher entfernt werden soll</param>
    static void IndexRemove(ushort[] index, int length, ushort key)
    {
      for (int i = 0; i < index.Length; i++)
      {
        if (index[i] == key)
        {
          for (int f = i + 1; f < length; f++)
          {
            index[f - 1] = index[f];
          }
          return;
        }
        Debug.Assert(i < length);
      }
      throw new Exception("Index-Entry not found!");
    }

    /// <summary>
    /// fügt eine weiße Figur dem Index hinzu
    /// </summary>
    /// <param name="key">Index-Key, welcher hinzugefügt werden soll</param>
    void WhiteIndexAdd(ushort key)
    {
      IndexAdd(whiteIndex, whitePieceCount++, key);
    }

    /// <summary>
    /// fügt eine schwarze Figur dem Index hinzu
    /// </summary>
    /// <param name="key">Index-Key, welcher hinzugefügt werden soll</param>
    void BlackIndexAdd(ushort key)
    {
      IndexAdd(blackIndex, blackPieceCount++, key);
    }

    /// <summary>
    /// fügt eine Figur dem Index hinzu
    /// </summary>
    /// <param name="key">Index-Key, welcher hinzugefügt werden soll</param>
    void IndexAddi(ushort key)
    {
      if ((key & (uint)Piece.White << 8) != 0)
      {
        WhiteIndexAdd(key);
      }
      else
      {
        BlackIndexAdd(key);
      }
    }

    /// <summary>
    /// entfernt eine weiße Figur aus dem Index
    /// </summary>
    /// <param name="key">Index-Key, welcher entfernt werden soll</param>
    void WhiteIndexRemove(ushort key)
    {
      IndexRemove(whiteIndex, whitePieceCount--, key);
    }

    /// <summary>
    /// entfernt eine schwarze Figur aus dem Index
    /// </summary>
    /// <param name="key">Index-Key, welcher entfernt werden soll</param>
    void BlackIndexRemove(ushort key)
    {
      IndexRemove(blackIndex, blackPieceCount--, key);
    }

    /// <summary>
    /// entfernt eine Figur aus dem Index
    /// </summary>
    /// <param name="key">Index-Key, welcher entfernt werden soll</param>
    void IndexRemovi(ushort key)
    {
      if ((key & (uint)Piece.White << 8) != 0)
      {
        WhiteIndexRemove(key);
      }
      else
      {
        BlackIndexRemove(key);
      }
    }

    /// <summary>
    /// aktualisiert eine weiße Figur im Index
    /// </summary>
    /// <param name="piece">Figur, welche aktualisiert werden soll</param>
    /// <param name="oldPos">alte Position der Figur</param>
    /// <param name="newPos">neue Position der Figur</param>
    void WhiteIndexUpdate(Piece piece, int oldPos, int newPos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.White);     // Figur muss eine gültige Farbe haben
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(oldPos != newPos);

      ushort oldIndex = (ushort)((byte)piece << 8 | oldPos);
      ushort newIndex = (ushort)((byte)piece << 8 | newPos);
      for (int i = 0; i < whiteIndex.Length; i++)
      {
        if (whiteIndex[i] == oldIndex)
        {
          while (i > 0 && whiteIndex[i - 1] > newIndex)
          {
            whiteIndex[i] = whiteIndex[i - 1];
            i--;
          }
          while (i < whitePieceCount - 1 && whiteIndex[i + 1] < newIndex)
          {
            whiteIndex[i] = whiteIndex[i + 1];
            i++;
          }
          whiteIndex[i] = newIndex;
          break;
        }
        Debug.Assert(i < whitePieceCount);
      }
    }

    /// <summary>
    /// aktualisiert eine Figur im Index
    /// </summary>
    /// <param name="piece">Figur, welche aktualisiert werden soll</param>
    /// <param name="oldPos">alte Position der Figur</param>
    /// <param name="newPos">neue Position der Figur</param>
    void BlackIndexUpdate(Piece piece, int oldPos, int newPos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.Black);     // Figur muss eine gültige Farbe haben
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(oldPos != newPos);

      ushort oldIndex = (ushort)((byte)piece << 8 | oldPos);
      ushort newIndex = (ushort)((byte)piece << 8 | newPos);
      for (int i = 0; i < blackIndex.Length; i++)
      {
        if (blackIndex[i] == oldIndex)
        {
          while (i > 0 && blackIndex[i - 1] > newIndex)
          {
            blackIndex[i] = blackIndex[i - 1];
            i--;
          }
          while (i < blackPieceCount - 1 && blackIndex[i + 1] < newIndex)
          {
            blackIndex[i] = blackIndex[i + 1];
            i++;
          }
          blackIndex[i] = newIndex;
          break;
        }
        Debug.Assert(i < blackPieceCount);
      }
    }

    /// <summary>
    /// aktualisiert eine Figur im Index
    /// </summary>
    /// <param name="piece">Figur, welche aktualisiert werden soll</param>
    /// <param name="oldPos">alte Position der Figur</param>
    /// <param name="newPos">neue Position der Figur</param>
    void IndexUpdati(Piece piece, int oldPos, int newPos)
    {
      Debug.Assert((piece & Piece.Colors) != Piece.None);      // Figur muss eine gültige Farbe haben
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(oldPos != newPos);

      if ((piece & Piece.Colors) == Piece.White)
      {
        WhiteIndexUpdate(piece, oldPos, newPos);
      }
      else
      {
        BlackIndexUpdate(piece, oldPos, newPos);
      }
    }

    /// <summary>
    /// prüft, ob die Index-Einträge alle gültig sind
    /// </summary>
    /// <returns>true, wenn die Validierung erfolgreich war</returns>
    bool IndexValidation()
    {
      var whitePieces = new List<ushort>();
      var blackPieces = new List<ushort>();

      for (int i = 0; i < fields.Length; i++)
      {
        var piece = fields[i];
        if (piece == Piece.None) continue;
        if ((piece & Piece.BasicPieces) == Piece.None)
        {
          Debug.Fail("color without piece? [" + i + "] - " + piece);
          return false;
        }
        if ((piece & Piece.Colors) == Piece.White)
        {
          whitePieces.Add((ushort)((byte)piece << 8 | i));
        }
        else if ((piece & Piece.Colors) == Piece.Black)
        {
          blackPieces.Add((ushort)((byte)piece << 8 | i));
        }
        else
        {
          Debug.Fail("piece without color? [" + i + "] - " + piece);
          return false;
        }
      }

      whitePieces.Sort();
      blackPieces.Sort();

      if (whitePieces.Count != whitePieceCount)
      {
        Debug.Fail("white pieces not match index: " + whitePieces.Count + " != " + whitePieceCount);
        return false;
      }

      if (blackPieces.Count != blackPieceCount)
      {
        Debug.Fail("black pieces not match index: " + blackPieces.Count + " != " + blackPieceCount);
        return false;
      }

      for (int i = 0; i < whitePieceCount; i++)
      {
        if (whitePieces[i] != whiteIndex[i])
        {
          Debug.Fail("white piece not match index: 0x" + whitePieces[i].ToString("x4") + " != 0x" + whiteIndex[i].ToString("x4"));
          return false;
        }
      }

      for (int i = 0; i < blackPieceCount; i++)
      {
        if (blackPieces[i] != blackIndex[i])
        {
          Debug.Fail("white piece not match index: 0x" + blackPieces[i].ToString("x4") + " != 0x" + blackIndex[i].ToString("x4"));
          return false;
        }
      }

      return true;
    }
    #endregion

    #region # // --- SetField / GetField / Clear ---
    /// <summary>
    /// leert das Spielfeld
    /// </summary>
    public override void Clear()
    {
      Array.Clear(fields, 0, fields.Length);
      WhiteMove = true;
      HalfmoveClock = 0;
      MoveNumber = 1;
      WhiteCanCastleKingside = false;
      WhiteCanCastleQueenside = false;
      BlackCanCastleKingside = false;
      BlackCanCastleQueenside = false;
      EnPassantPos = -1;
      whitePieceCount = 0;
      blackPieceCount = 0;
    }

    /// <summary>
    /// setzt eine Spielfigur auf das Schachbrett
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <param name="piece">Spielfigur, welche gesetzt werden soll (kann Piece.None sein = leert das Feld)</param>
    public override void SetField(int pos, Piece piece)
    {
      if ((uint)pos >= Width * Height) throw new ArgumentOutOfRangeException("pos");

      var oldPiece = fields[pos];
      if (oldPiece != Piece.None) IndexRemovi(IndexKey(oldPiece, pos));

      Debug.Assert(IndexValidation());

      fields[pos] = piece;
      if (piece != Piece.None) IndexAddi(IndexKey(piece, pos));

      Debug.Assert(IndexValidation());
    }

    /// <summary>
    /// gibt die aktuelle Spielfigur auf dem Schachbrett zurück
    /// </summary>
    /// <param name="pos">Position auf dem Schachbrett</param>
    /// <returns>Spielfigur auf dem Feld</returns>
    public override Piece GetField(int pos)
    {
      if ((uint)pos >= Width * Height) return Piece.Blocked;

      return fields[pos];
    }

    /// <summary>
    /// gibt an, ob Weiß am Zug ist
    /// </summary>
    public override bool WhiteMove { get; set; }

    /// <summary>
    /// Anzahl der Halbzüge seit der letzten "Aktion" (Figur wurde geschlagen oder ein Bauer wurde bewegt)
    /// </summary>
    public override int HalfmoveClock { get; set; }

    /// <summary>
    /// aktuelle Spielzug Nummer (ganze Züge)
    /// </summary>
    public override int MoveNumber { get; set; }

    /// <summary>
    /// gibt an, ob Weiß die kurze Rochade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public override bool WhiteCanCastleKingside { get; set; }

    /// <summary>
    /// gibt an, ob Weiß die lange Rochade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public override bool WhiteCanCastleQueenside { get; set; }

    /// <summary>
    /// gibt an, ob Schwarz die kurze Rochade "O-O" auf der Königsseite noch machen kann
    /// </summary>
    public override bool BlackCanCastleKingside { get; set; }

    /// <summary>
    /// gibt an, ob Schwarz die lange Rochade "O-O-O" auf der Damenseite noch machen kann
    /// </summary>
    public override bool BlackCanCastleQueenside { get; set; }

    /// <summary>
    /// Position des übersprungenen Feldes eines Bauern, welcher beim vorherigen Zug zwei Feldern vorgerückt ist (für "en pasant"), sonst = -1
    /// </summary>
    public override int EnPassantPos { get; set; }

    /// <summary>
    /// fragt zusätzliche Spielbrettinformationen ab oder setzt diese ("en passant", Rochade-Möglichkeiten und Halfmove-Counter für 50-Züge Regel)
    /// </summary>
    public override BoardInfo BoardInfos
    {
      get
      {
        return (BoardInfo)(byte)(sbyte)EnPassantPos
          | (WhiteCanCastleKingside ? BoardInfo.WhiteCanCastleKingside : BoardInfo.None)
          | (WhiteCanCastleQueenside ? BoardInfo.WhiteCanCastleQueenside : BoardInfo.None)
          | (BlackCanCastleKingside ? BoardInfo.BlackCanCastleKingside : BoardInfo.None)
          | (BlackCanCastleQueenside ? BoardInfo.BlackCanCastleQueenside : BoardInfo.None)
          | (BoardInfo)(HalfmoveClock << 16);
      }
      set
      {
        EnPassantPos = (sbyte)(byte)(value & BoardInfo.EnPassantNone);
        WhiteCanCastleKingside = (value & BoardInfo.WhiteCanCastleKingside) != BoardInfo.None;
        WhiteCanCastleQueenside = (value & BoardInfo.WhiteCanCastleQueenside) != BoardInfo.None;
        BlackCanCastleKingside = (value & BoardInfo.BlackCanCastleKingside) != BoardInfo.None;
        BlackCanCastleQueenside = (value & BoardInfo.BlackCanCastleQueenside) != BoardInfo.None;
        HalfmoveClock = (int)(value & BoardInfo.HalfmoveCounterMask) >> 16;
      }
    }

    #endregion

    #region # // --- Move ---
    /// <summary>
    /// prüft die theoretischen Bewegungsmöglichkeiten einer weißen Spielfigur auf einem bestimmten Feld
    /// </summary>
    /// <param name="piece">Figur, welche gezogen wird</param>
    /// <param name="pos">Position auf dem Spielfeld mit der zu testenden Figur</param>
    /// <returns>Aufzählung der theretisch begehbaren Felder</returns>
    IEnumerable<int> ScanWhiteMove(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.White);

      int posX = pos % Width;
      int posY = pos / Width;
      switch (piece)
      {
        #region # case Piece.WhiteKing: // König
        case Piece.WhiteKing:
        {
          if (posX > 0) // nach links
          {
            if (posY > 0 && (fields[pos - (Width + 1)] & Piece.White) == Piece.None) yield return pos - (Width + 1); // links-oben
            if ((fields[pos - 1] & Piece.White) == Piece.None) yield return pos - 1; // links
            if (posY < Height - 1 && (fields[pos + (Width - 1)] & Piece.White) == Piece.None) yield return pos + (Width - 1); // links-unten
          }
          if (posX < Width - 1) // nach rechts
          {
            if (posY > 0 && (fields[pos - (Width - 1)] & Piece.White) == Piece.None) yield return pos - (Width - 1); // rechts-oben
            if ((fields[pos + 1] & Piece.White) == Piece.None) yield return pos + 1; // rechts
            if (posY < Height - 1 && (fields[pos + (Width + 1)] & Piece.White) == Piece.None) yield return pos + (Width + 1); // rechts-unten
          }
          if (posY > 0 && (fields[pos - Width] & Piece.White) == Piece.None) yield return pos - Width; // oben
          if (posY < Height - 1 && (fields[pos + Width] & Piece.White) == Piece.None) yield return pos + Width; // unten
        } break;
        #endregion

        #region # case Piece.WhiteQueen: // Dame
        case Piece.WhiteQueen:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } goto case Piece.WhiteBishop;
        #endregion

        #region # case Piece.WhiteRook: // Turm
        case Piece.WhiteRook:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.WhiteBishop: // Läufer
        case Piece.WhiteBishop:
        {
          // links-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY - i < 0) break;
            int p = pos - (Width * i + i);
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // links-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY + i >= Height) break;
            int p = pos + (Width * i - i);
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY - i < 0) break;
            int p = pos - (Width * i - i);
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY + i >= Height) break;
            int p = pos + (Width * i + i);
            var f = fields[p];
            if ((f & Piece.White) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.WhiteKnight: // Springer
        case Piece.WhiteKnight:
        {
          if (posX > 0) // 1 nach links
          {
            if (posY > 1 && (fields[pos - (Width * 2 + 1)] & Piece.White) == Piece.None) yield return pos - (Width * 2 + 1); // -1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 - 1)] & Piece.White) == Piece.None) yield return pos + (Width * 2 - 1); // -1, +2
            if (posX > 1) // 2 nach links
            {
              if (posY > 0 && (fields[pos - (Width + 2)] & Piece.White) == Piece.None) yield return pos - (Width + 2); // -2, -1
              if (posY < Height - 1 && (fields[pos + (Width - 2)] & Piece.White) == Piece.None) yield return pos + (Width - 2); // -2, +1
            }
          }
          if (posX < Width - 1) // 1 nach rechts
          {
            if (posY > 1 && (fields[pos - (Width * 2 - 1)] & Piece.White) == Piece.None) yield return pos - (Width * 2 - 1); // +1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 + 1)] & Piece.White) == Piece.None) yield return pos + (Width * 2 + 1); // +1, +2
            if (posX < Width - 2) // 2 nach rechts
            {
              if (posY > 0 && (fields[pos - (Width - 2)] & Piece.White) == Piece.None) yield return pos - (Width - 2); // +2, +1
              if (posY < Height - 1 && (fields[pos + (Width + 2)] & Piece.White) == Piece.None) yield return pos + (Width + 2); // +2, -1
            }
          }
        } break;
        #endregion

        #region # case Piece.WhitePawn: // Bauer
        case Piece.WhitePawn:
        {
          if (posY < 1 || posY >= Height - 1) break; // ungültige Position

          if (fields[pos - Width] == Piece.None) // Laufweg frei?
          {
            yield return pos - Width;
            if (posY == 6 && fields[pos - Width * 2] == Piece.None) yield return pos - Width * 2; // Doppelzug
          }
          if (posX > 0 && (EnPassantPos == pos - (Width + 1) || (fields[pos - (Width + 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width + 1); // nach links-oben schlagen
          if (posX < Width - 1 && (EnPassantPos == pos - (Width - 1) || (fields[pos - (Width - 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width - 1); // nach rechts-oben schlagen
        } break;
        #endregion
      }
    }

    /// <summary>
    /// prüft die theoretischen Bewegungsmöglichkeiten einer schwarzen Spielfigur auf einem bestimmten Feld
    /// </summary>
    /// <param name="piece">Figur, welche gezogen wird</param>
    /// <param name="pos">Position auf dem Spielfeld mit der zu testenden Figur</param>
    /// <returns>Aufzählung der theretisch begehbaren Felder</returns>
    IEnumerable<int> ScanBlackMove(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.Black);

      int posX = pos % Width;
      int posY = pos / Width;
      switch (piece)
      {
        #region # case Piece.BlackKing: // König
        case Piece.BlackKing:
        {
          if (posX > 0) // nach links
          {
            if (posY > 0 && (fields[pos - (Width + 1)] & Piece.Black) == Piece.None) yield return pos - (Width + 1); // links-oben
            if ((fields[pos - 1] & Piece.Black) == Piece.None) yield return pos - 1; // links
            if (posY < Height - 1 && (fields[pos + (Width - 1)] & Piece.Black) == Piece.None) yield return pos + (Width - 1); // links-unten
          }
          if (posX < Width - 1) // nach rechts
          {
            if (posY > 0 && (fields[pos - (Width - 1)] & Piece.Black) == Piece.None) yield return pos - (Width - 1); // rechts-oben
            if ((fields[pos + 1] & Piece.Black) == Piece.None) yield return pos + 1; // rechts
            if (posY < Height - 1 && (fields[pos + (Width + 1)] & Piece.Black) == Piece.None) yield return pos + (Width + 1); // rechts-unten
          }
          if (posY > 0 && (fields[pos - Width] & Piece.Black) == Piece.None) yield return pos - Width; // oben
          if (posY < Height - 1 && (fields[pos + Width] & Piece.Black) == Piece.None) yield return pos + Width; // unten
        } break;
        #endregion

        #region # case Piece.BlackQueen: // Dame
        case Piece.BlackQueen:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } goto case Piece.BlackBishop;
        #endregion

        #region # case Piece.BlackRook: // Turm
        case Piece.BlackRook:
        {
          // links
          for (int i = 1; i < Width; i++)
          {
            if (posX - i < 0) break;
            int p = pos - i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts
          for (int i = 1; i < Width; i++)
          {
            if (posX + i >= Width) break;
            int p = pos + i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // oben
          for (int i = 1; i < Height; i++)
          {
            if (posY - i < 0) break;
            int p = pos - Width * i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // unten
          for (int i = 1; i < Height; i++)
          {
            if (posY + i >= Height) break;
            int p = pos + Width * i;
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.BlackBishop: // Läufer
        case Piece.BlackBishop:
        {
          // links-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY - i < 0) break;
            int p = pos - (Width * i + i);
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // links-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX - i < 0 || posY + i >= Height) break;
            int p = pos + (Width * i - i);
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-oben
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY - i < 0) break;
            int p = pos - (Width * i - i);
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
          // rechts-unten
          for (int i = 1; i < Math.Max(Width, Height); i++)
          {
            if (posX + i >= Width || posY + i >= Height) break;
            int p = pos + (Width * i + i);
            var f = fields[p];
            if ((f & Piece.Black) != Piece.None) break;
            yield return p;
            if (f != Piece.None) break;
          }
        } break;
        #endregion

        #region # case Piece.BlackKnight: // Springer
        case Piece.BlackKnight:
        {
          if (posX > 0) // 1 nach links
          {
            if (posY > 1 && (fields[pos - (Width * 2 + 1)] & Piece.Black) == Piece.None) yield return pos - (Width * 2 + 1); // -1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 - 1)] & Piece.Black) == Piece.None) yield return pos + (Width * 2 - 1); // -1, +2
            if (posX > 1) // 2 nach links
            {
              if (posY > 0 && (fields[pos - (Width + 2)] & Piece.Black) == Piece.None) yield return pos - (Width + 2); // -2, -1
              if (posY < Height - 1 && (fields[pos + (Width - 2)] & Piece.Black) == Piece.None) yield return pos + (Width - 2); // -2, +1
            }
          }
          if (posX < Width - 1) // 1 nach rechts
          {
            if (posY > 1 && (fields[pos - (Width * 2 - 1)] & Piece.Black) == Piece.None) yield return pos - (Width * 2 - 1); // +1, -2
            if (posY < Height - 2 && (fields[pos + (Width * 2 + 1)] & Piece.Black) == Piece.None) yield return pos + (Width * 2 + 1); // +1, +2
            if (posX < Width - 2) // 2 nach rechts
            {
              if (posY > 0 && (fields[pos - (Width - 2)] & Piece.Black) == Piece.None) yield return pos - (Width - 2); // +2, +1
              if (posY < Height - 1 && (fields[pos + (Width + 2)] & Piece.Black) == Piece.None) yield return pos + (Width + 2); // +2, -1
            }
          }
        } break;
        #endregion

        #region # case Piece.BlackPawn: // Bauer
        case Piece.BlackPawn:
        {
          if (posY < 1 || posY >= Height - 1) break; // ungültige Position

          if (fields[pos + Width] == Piece.None) // Laufweg frei?
          {
            yield return pos + Width;
            if (posY == 1 && fields[pos + Width * 2] == Piece.None) yield return pos + Width * 2; // Doppelzug
          }
          if (posX > 0 && (EnPassantPos == pos + (Width - 1) || (fields[pos + (Width - 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width - 1); // nach links-unten schlagen
          if (posX < Width - 1 && (EnPassantPos == pos + (Width + 1) || (fields[pos + (Width + 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width + 1); // nach rechts-unten schlagen
        } break;
        #endregion
      }
    }

    /// <summary>
    /// fragt die aktuelle Position des Königs ab
    /// </summary>
    /// <param name="kingColor">Farbe des Königs, welche abgefragt werden soll</param>
    /// <returns>Position auf dem Spielbrett</returns>
    public override int GetKingPos(Piece kingColor)
    {
      return ((kingColor & Piece.White) != Piece.None ? whiteIndex[0] : blackIndex[0]) & 0xff;
    }

    /// <summary>
    /// prüft, ob ein bestimmtes Spielfeld unter Schach steht
    /// </summary>
    /// <param name="pos">Position, welche geprüft werden soll</param>
    /// <param name="checkerColor">zu prüfende Spielerfarbe, welche das Schach geben könnte (nur <see cref="Piece.White"/> oder <see cref="Piece.Black"/> erlaubt)</param>
    /// <returns>true, wenn das Feld angegriffen wird und unter Schach steht</returns>
    public override bool IsChecked(int pos, Piece checkerColor)
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
    /// fragt das gesamte Spielbrett ab
    /// </summary>
    /// <param name="array">Array, wohin die Daten des Spielbrettes gespeichert werden sollen</param>
    /// <param name="ofs">Startposition im Array</param>
    /// <returns>Anzahl der geschriebenen Bytes</returns>
    public override int GetFastFen(byte[] array, int ofs)
    {
      int p = 0;
      int gap = 0;
      foreach (var field in fields)
      {
        if (field == Piece.None)
        {
          gap++;
          continue;
        }
        if (gap > 0)
        {
          array[ofs + p++] = (byte)(uint)gap;
          gap = 0;
        }
        array[ofs + p++] = (byte)field;
      }
      if (gap > 0) array[ofs + p++] = (byte)(uint)gap;
      array[ofs + p++] = (byte)((WhiteMove ? 1u : 0) | (WhiteCanCastleKingside ? 2u : 0) | (WhiteCanCastleQueenside ? 4u : 0) | (BlackCanCastleKingside ? 8u : 0) | (BlackCanCastleQueenside ? 16u : 0));
      array[ofs + p++] = (byte)(sbyte)EnPassantPos;
      array[ofs + p++] = (byte)(uint)HalfmoveClock;
      array[ofs + p++] = (byte)(uint)(HalfmoveClock >> 8);
      array[ofs + p++] = (byte)(uint)MoveNumber;
      array[ofs + p++] = (byte)(uint)(MoveNumber >> 8);

      return p;
    }

    /// <summary>
    /// setzt das gesamte Spielbrett
    /// </summary>
    /// <param name="array">Array, worraus die Daten des Spielbrettes gelesen werden sollen</param>
    /// <param name="ofs">Startposition im Array</param>
    /// <returns>Anzahl der gelesenen Bytes</returns>
    public override int SetFastFen(byte[] array, int ofs)
    {
      int p = 0;
      byte b;
      whitePieceCount = 0;
      blackPieceCount = 0;
      for (int i = 0; i < fields.Length; i++)
      {
        b = array[ofs + p++];
        if (b < 64) // gap found?
        {
          fields[i] = Piece.None;
          while (--b != 0) fields[++i] = Piece.None;
          continue;
        }
        fields[i] = (Piece)b;
        IndexAddi(IndexKey((Piece)b, i));
      }
      b = array[ofs + p++];
      WhiteMove = (b & 1) != 0;
      WhiteCanCastleKingside = (b & 2) != 0;
      WhiteCanCastleQueenside = (b & 4) != 0;
      BlackCanCastleKingside = (b & 8) != 0;
      BlackCanCastleQueenside = (b & 16) != 0;
      EnPassantPos = (sbyte)array[ofs + p++];
      HalfmoveClock = array[ofs + p] | array[ofs + p + 1] << 8; p += sizeof(short);
      MoveNumber = array[ofs + p] | array[ofs + p + 1] << 8; p += sizeof(short);

      Debug.Assert(IndexValidation());

      return p;
    }

    /// <summary>
    /// generiert eine eindeutige Prüfsumme des Spielfeldes inkl. Zugnummern
    /// </summary>
    /// <returns>64-Bit Prüfsumme</returns>
    public override ulong GetFullChecksum()
    {
      return Crc64.Start.Crc64Update(fields) // Figuren auf dem Spielfeld
        .Crc64Update(WhiteMove)              // Spielerfarbe, welche am Zug ist
        .Crc64Update(WhiteCanCastleKingside).Crc64Update(WhiteCanCastleQueenside) // weiße Rochademöglichkeiten
        .Crc64Update(BlackCanCastleKingside).Crc64Update(BlackCanCastleQueenside) // schwarze Rochademöglichkeiten
        .Crc64Update(EnPassantPos)           // letzter doppelter Bauernzug für "en passant"
        /*.Crc64Update(HalfmoveClock)*/      // 50 Züge-Regel ignorieren
        .Crc64Update(MoveNumber); // Zugnummern
    }

    /// <summary>
    /// generiert eine eindeutige Prüfsumme des Spielfeldes ohne Zugnummern
    /// </summary>
    /// <returns>64-Bit Prüfsumme</returns>
    public override ulong GetChecksum()
    {
      return Crc64.Start.Crc64Update(fields) // Figuren auf dem Spielfeld
        .Crc64Update(WhiteMove)              // Spielerfarbe, welche am Zug ist
        .Crc64Update(WhiteCanCastleKingside).Crc64Update(WhiteCanCastleQueenside) // weiße Rochademöglichkeiten
        .Crc64Update(BlackCanCastleKingside).Crc64Update(BlackCanCastleQueenside) // schwarze Rochademöglichkeiten
        .Crc64Update(EnPassantPos);          // letzter doppelter Bauernzug für "en passant"
    }

    /// <summary>
    /// führt einen weißen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    bool DoMoveWhite(Move move, bool onlyCheck)
    {
      var piece = fields[move.fromPos];

      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // ist eine Figur auf dem Feld vorhanden?
      Debug.Assert(fields[move.toPos] == move.capturePiece); // stimmt die zu schlagende Figur mit dem Spielfeld überein?
      Debug.Assert((move.capturePiece & Piece.Colors) != (piece & Piece.Colors)); // wird keine eigene Figur gleicher Farbe geschlagen?
      Debug.Assert((piece & Piece.Colors) == Piece.White); // passt die Figur-Farbe zum Zug?

      // --- Zug durchführen ---
      if (move.capturePiece != Piece.None) BlackIndexRemove(IndexKey(move.capturePiece, move.toPos));
      fields[move.toPos] = piece;
      fields[move.fromPos] = Piece.None;
      WhiteIndexUpdate(piece, move.fromPos, move.toPos);
      Debug.Assert(IndexValidation());

      if (piece == Piece.WhitePawn && move.toPos == EnPassantPos) // ein Bauer schlägt "en passant"?
      {
        Debug.Assert(move.toPos % Width != move.fromPos % Width); // Spalte muss sich ändern
        Debug.Assert(move.capturePiece == Piece.None); // das Zielfeld enhält keine Figur (der geschlagene Bauer ist drüber oder drunter)
        int removePawnPos = move.toPos + Width; // Position des zu schlagenden Bauern berechnen
        Debug.Assert(fields[removePawnPos] == Piece.BlackPawn); // es wird ein Bauer erwartet, welcher geschlagen wird

        fields[removePawnPos] = Piece.None; // Bauer entfernen
        BlackIndexRemove(IndexKey(Piece.BlackPawn, removePawnPos));
        Debug.Assert(IndexValidation());
      }

      if (move.promoPiece != Piece.None)
      {
        fields[move.toPos] = move.promoPiece;
        WhiteIndexRemove(IndexKey(Piece.WhitePawn, move.toPos));
        WhiteIndexAdd(IndexKey(move.promoPiece, move.toPos));
        Debug.Assert(IndexValidation());
      }

      // --- prüfen, ob der König nach dem Zug im Schach steht ---
      {
        int kingPos = whiteIndex[0] & 0xff;
        if (!onlyCheck && kingPos == move.toPos && Math.Abs(move.toPos - move.fromPos) == 2) // wurde der König mit einer Rochade bewegt (zwei Felder seitlich)?
        {
          switch (kingPos)
          {
            case 58: // lange Rochade mit dem weißen König
            {
              Debug.Assert(WhiteCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
              Debug.Assert(fields[56] == Piece.WhiteRook && fields[57] == Piece.None && fields[58] == Piece.WhiteKing && fields[59] == Piece.None && fields[60] == Piece.None); // Felder prüfen
              fields[56] = Piece.None; fields[59] = Piece.WhiteRook; // Turm bewegen
              WhiteIndexUpdate(Piece.WhiteRook, 56, 59);
            } break;
            case 62: // kurze Rochade mit dem weißen König
            {
              Debug.Assert(WhiteCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
              Debug.Assert(fields[60] == Piece.None && fields[61] == Piece.None && fields[62] == Piece.WhiteKing && fields[63] == Piece.WhiteRook); // Felder prüfen
              fields[63] = Piece.None; fields[61] = Piece.WhiteRook; // Turm bewegen
              WhiteIndexUpdate(Piece.WhiteRook, 63, 61);
            } break;
            default: throw new Exception(); // Rochade war unmöglich
          }
          Debug.Assert(IndexValidation());
        }
        else if (IsChecked(kingPos, Piece.Black)) // prüfen, ob der eigene König vom Gegner angegriffen wird und noch im Schach steht
        {
          // --- Zug rückgängig machen ---
          WhiteIndexRemove(IndexKey(fields[move.toPos], move.toPos));
          fields[move.toPos] = move.capturePiece;
          if (move.capturePiece != Piece.None) BlackIndexAdd(IndexKey(move.capturePiece, move.toPos));
          fields[move.fromPos] = piece; WhiteIndexAdd(IndexKey(piece, move.fromPos));
          if (piece == Piece.WhitePawn && move.toPos == EnPassantPos) // ein Bauer hat "en passant" geschlagen?
          {
            fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
            BlackIndexAdd(IndexKey(Piece.BlackPawn, move.toPos + Width));
          }
          Debug.Assert(IndexValidation());
          return false; // Zug war nicht erlaubt, da der König sonst im Schach stehen würde
        }
      }

      if (onlyCheck) // Zug sollte nur geprüft werden?
      {
        // --- Zug rückgängig machen ---
        WhiteIndexRemove(IndexKey(fields[move.toPos], move.toPos));
        fields[move.toPos] = move.capturePiece;
        if (move.capturePiece != Piece.None) BlackIndexAdd(IndexKey(move.capturePiece, move.toPos));
        fields[move.fromPos] = piece; WhiteIndexAdd(IndexKey(piece, move.fromPos));
        if (piece == Piece.WhitePawn && move.toPos == EnPassantPos) // ein Bauer hat "en passant" geschlagen?
        {
          fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
          BlackIndexAdd(IndexKey(Piece.BlackPawn, move.toPos + Width));
        }
        Debug.Assert(IndexValidation());
        return true;
      }

      EnPassantPos = -1;
      if (piece == Piece.WhitePawn && Math.Abs(move.toPos - move.fromPos) == Width * 2) // wurde ein Bauer zwei Felder weit gezogen -> "en passant" vormerken
      {
        EnPassantPos = (move.fromPos + move.toPos) / 2;
        int posX = EnPassantPos % Width;
        bool opPawn = posX > 0 && fields[EnPassantPos - Width - 1] == Piece.BlackPawn
                   || posX < Width - 1 && fields[EnPassantPos - Width + 1] == Piece.BlackPawn;
        if (!opPawn) EnPassantPos = -1; // kein "en passant" möglich, da kein gegenerischer Bauer in der Nähe ist
      }

      // prüfen, ob durch den Zug Rochaden ungültig werden
      switch (move.fromPos)
      {
        case 56: WhiteCanCastleQueenside = false; break; // linker weißer Turm wurde mindestens das erste Mal bewegt
        case 60: WhiteCanCastleQueenside = false; WhiteCanCastleKingside = false; break; // weißer König wurde mindestens das erste Mal bewegt
        case 63: WhiteCanCastleKingside = false; break; // rechter weißer Turm wurde mindestens das erste Mal bewegt
      }
      switch (move.toPos)
      {
        case 0: BlackCanCastleQueenside = false; break; // linker schwarzer Turm wurde geschlagen
        case 7: BlackCanCastleKingside = false; break; // rechter schwarzer Turm wurde geschlagen
      }

      WhiteMove = false; // Farbe welchseln, damit der andere Spieler am Zug ist
      HalfmoveClock++;
      if (piece == Piece.WhitePawn || move.capturePiece != Piece.None) HalfmoveClock = 0; // beim Bauernzug oder Schlagen einer Figur: 50-Züge Regel zurücksetzen

      return true;
    }

    /// <summary>
    /// führt einen schwarzen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    bool DoMoveBlack(Move move, bool onlyCheck)
    {
      var piece = fields[move.fromPos];

      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // ist eine Figur auf dem Feld vorhanden?
      Debug.Assert(fields[move.toPos] == move.capturePiece); // stimmt die zu schlagende Figur mit dem Spielfeld überein?
      Debug.Assert((move.capturePiece & Piece.Colors) != (piece & Piece.Colors)); // wird keine eigene Figur gleicher Farbe geschlagen?
      Debug.Assert((piece & Piece.Colors) == Piece.Black); // passt die Figur-Farbe zum Zug?

      // --- Zug durchführen ---
      if (move.capturePiece != Piece.None) WhiteIndexRemove(IndexKey(move.capturePiece, move.toPos));
      fields[move.toPos] = piece;
      fields[move.fromPos] = Piece.None;
      BlackIndexUpdate(piece, move.fromPos, move.toPos);
      Debug.Assert(IndexValidation());

      if (piece == Piece.BlackPawn && move.toPos == EnPassantPos) // ein Bauer schlägt "en passant"?
      {
        Debug.Assert(move.toPos % Width != move.fromPos % Width); // Spalte muss sich ändern
        Debug.Assert(move.capturePiece == Piece.None); // das Zielfeld enhält keine Figur (der geschlagene Bauer ist drüber oder drunter)
        int removePawnPos = move.toPos - Width; // Position des zu schlagenden Bauern berechnen
        Debug.Assert(fields[removePawnPos] == Piece.WhitePawn); // es wird ein Bauer erwartet, welcher geschlagen wird

        WhiteIndexRemove(IndexKey(Piece.WhitePawn, removePawnPos));
        fields[removePawnPos] = Piece.None; // Bauer entfernen
        Debug.Assert(IndexValidation());
      }

      if (move.promoPiece != Piece.None)
      {
        fields[move.toPos] = move.promoPiece;
        BlackIndexRemove(IndexKey(Piece.BlackPawn, move.toPos));
        BlackIndexAdd(IndexKey(move.promoPiece, move.toPos));
        Debug.Assert(IndexValidation());
      }

      // --- prüfen, ob der König nach dem Zug im Schach steht ---
      {
        int kingPos = blackIndex[0] & 0xff;
        if (!onlyCheck && kingPos == move.toPos && Math.Abs(move.toPos - move.fromPos) == 2) // wurde der König mit einer Rochade bewegt (zwei Felder seitlich)?
        {
          switch (kingPos)
          {
            case 2: // lange Rochade mit dem schwarzen König
            {
              Debug.Assert(BlackCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
              Debug.Assert(fields[0] == Piece.BlackRook && fields[1] == Piece.None && fields[2] == Piece.BlackKing && fields[3] == Piece.None && fields[4] == Piece.None); // Felder prüfen
              fields[0] = Piece.None; fields[3] = Piece.BlackRook; // Turm bewegen
              BlackIndexUpdate(Piece.BlackRook, 0, 3);
            } break;
            case 6: // kurze Rochade mit dem schwarzen König
            {
              Debug.Assert(BlackCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
              Debug.Assert(fields[4] == Piece.None && fields[5] == Piece.None && fields[6] == Piece.BlackKing && fields[7] == Piece.BlackRook); // Felder prüfen
              fields[7] = Piece.None; fields[5] = Piece.BlackRook; // Turm bewegen
              BlackIndexUpdate(Piece.BlackRook, 7, 5);
            } break;
            default: throw new Exception(); // Rochade war unmöglich
          }
          Debug.Assert(IndexValidation());
        }
        else if (IsChecked(kingPos, Piece.White)) // prüfen, ob der eigene König vom Gegner angegriffen wird und noch im Schach steht
        {
          // --- Zug rückgängig machen ---
          BlackIndexRemove(IndexKey(fields[move.toPos], move.toPos));
          fields[move.toPos] = move.capturePiece;
          if (move.capturePiece != Piece.None) WhiteIndexAdd(IndexKey(move.capturePiece, move.toPos));
          fields[move.fromPos] = piece; BlackIndexAdd(IndexKey(piece, move.fromPos));
          if (piece == Piece.BlackPawn && move.toPos == EnPassantPos) // ein Bauer hat "en passant" geschlagen?
          {
            fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
            WhiteIndexAdd(IndexKey(Piece.WhitePawn, move.toPos - Width));
          }
          Debug.Assert(IndexValidation());
          return false; // Zug war nicht erlaubt, da der König sonst im Schach stehen würde
        }
      }

      if (onlyCheck) // Zug sollte nur geprüft werden?
      {
        // --- Zug rückgängig machen ---
        BlackIndexRemove(IndexKey(fields[move.toPos], move.toPos));
        fields[move.toPos] = move.capturePiece;
        if (move.capturePiece != Piece.None) WhiteIndexAdd(IndexKey(move.capturePiece, move.toPos));
        fields[move.fromPos] = piece; BlackIndexAdd(IndexKey(piece, move.fromPos));
        if (piece == Piece.BlackPawn && move.toPos == EnPassantPos) // ein Bauer hat "en passant" geschlagen?
        {
          fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
          WhiteIndexAdd(IndexKey(Piece.WhitePawn, move.toPos - Width));
        }
        Debug.Assert(IndexValidation());
        return true;
      }

      EnPassantPos = -1;
      if (piece == Piece.BlackPawn && Math.Abs(move.toPos - move.fromPos) == Width * 2) // wurde ein Bauer zwei Felder weit gezogen -> "en passant" vormerken
      {
        EnPassantPos = (move.fromPos + move.toPos) / 2;
        int posX = EnPassantPos % Width;
        bool opPawn = posX > 0 && fields[EnPassantPos + Width - 1] == Piece.WhitePawn
                   || posX < Width - 1 && fields[EnPassantPos + Width + 1] == Piece.WhitePawn;
        if (!opPawn) EnPassantPos = -1; // kein "en passant" möglich, da kein gegenerischer Bauer in der Nähe ist
      }

      // prüfen, ob durch den Zug Rochaden ungültig werden
      switch (move.fromPos)
      {
        case 0: BlackCanCastleQueenside = false; break; // linker schwarzer Turm wurde mindestens das erste Mal bewegt
        case 4: BlackCanCastleQueenside = false; BlackCanCastleKingside = false; break; // schwarzer König wurde mindestens das erste Mal bewegt
        case 7: BlackCanCastleKingside = false; break; // rechter schwarzer Turm wurde mindestens das erste Mal bewegt
      }
      switch (move.toPos)
      {
        case 56: WhiteCanCastleQueenside = false; break; // linker weißer Turm wurde geschlagen
        case 63: WhiteCanCastleKingside = false; break; // rechter weißer Turm wurde geschlagen
      }

      WhiteMove = true; // Farbe welchseln, damit der andere Spieler am Zug ist
      HalfmoveClock++;
      if (piece == Piece.BlackPawn || move.capturePiece != Piece.None) HalfmoveClock = 0; // beim Bauernzug oder Schlagen einer Figur: 50-Züge Regel zurücksetzen
      MoveNumber++; // Züge weiter hochzählen

      return true;
    }

    /// <summary>
    /// führt einen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    public override bool DoMove(Move move, bool onlyCheck = false)
    {
      return WhiteMove ? DoMoveWhite(move, onlyCheck) : DoMoveBlack(move, onlyCheck);
    }

    /// <summary>
    /// macht einen bestimmten weißen Zug wieder Rückgängig
    /// </summary>
    /// <param name="move">Zug, welcher rückgängig gemacht werden soll</param>
    /// <param name="lastBoardInfos">Spielbrettinformationen der vorherigen Stellung</param>
    void DoMoveWhiteBackward(Move move, BoardInfo lastBoardInfos)
    {
      // --- Figur zurückziehen ---
      var piece = fields[move.toPos];
      fields[move.fromPos] = piece; // Figur zurücksetzen
      WhiteIndexUpdate(piece, move.toPos, move.fromPos);
      fields[move.toPos] = move.capturePiece; // eventuell geschlagene Figur wiederherstellen
      if (move.capturePiece != Piece.None) BlackIndexAdd(IndexKey(move.capturePiece, move.toPos));

      Debug.Assert(IndexValidation());

      // --- Bauer Umwandlung: promotion ---
      if (move.promoPiece != Piece.None)
      {
        Debug.Assert(piece == move.promoPiece); // umgewandelte Figur sollte übereinstimmen
        WhiteIndexRemove(IndexKey(piece, move.fromPos));
        fields[move.fromPos] = Piece.WhitePawn; // Figur zum Bauern zurück verwandeln :)
        WhiteIndexAdd(IndexKey(Piece.WhitePawn, move.fromPos));
        Debug.Assert(IndexValidation());
      }

      // --- Bauer hat "en passant" geschlagen ---
      if (piece == Piece.WhitePawn
          && move.fromPos % Width != move.toPos % Width
          && move.capturePiece == Piece.None) // hat der Bauer seitlich ein "Nichts" geschlagen? -> der gegenerische Bauer wurde dann ein Feld drüber/drunter entfernt
      {
        Debug.Assert((lastBoardInfos & BoardInfo.EnPassantMask) != BoardInfo.EnPassantNone); // war "en passant" vorher erlaubt?
        fields[(uint)(lastBoardInfos & BoardInfo.EnPassantMask) + Width] = Piece.BlackPawn;
        BlackIndexAdd(IndexKey(Piece.BlackPawn, (int)((uint)(lastBoardInfos & BoardInfo.EnPassantMask) + Width)));
        Debug.Assert(IndexValidation());
      }

      // --- eine Rochade wurde gemacht ---
      if (piece == Piece.WhiteKing && Math.Abs(move.fromPos % Width - move.toPos % Width) > 1) // wurde ein König mehr als 1 Feld seitlich bewegt?
      {
        switch (move.toPos)
        {
          case 58: // weiße lange Rochade auf der Damen-Seite (O-O-O)
          {
            Debug.Assert(fields[56] == Piece.None && fields[57] == Piece.None && fields[58] == Piece.None && fields[59] == Piece.WhiteRook && fields[60] == Piece.WhiteKing); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.WhiteCanCastleQueenside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[56] = Piece.WhiteRook; fields[59] = Piece.None; // Turm zurück in die Ecke setzen
            WhiteIndexUpdate(Piece.WhiteRook, 59, 56);
          } break;

          case 62: // weiße kurze Rochade auf der Königs-Seite (O-O)
          {
            Debug.Assert(fields[60] == Piece.WhiteKing && fields[61] == Piece.WhiteRook && fields[62] == Piece.None && fields[63] == Piece.None); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.WhiteCanCastleKingside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[63] = Piece.WhiteRook; fields[61] = Piece.None; // Turm zurück in die Ecke setzen
            WhiteIndexUpdate(Piece.WhiteRook, 61, 63);
          } break;

          default: throw new Exception("invalid move"); // ungültige Rochade?
        }
        Debug.Assert(IndexValidation());
      }

      // --- Spielbrett Infos anpassen ---
      WhiteMove = true;
      BoardInfos = lastBoardInfos;
    }

    /// <summary>
    /// macht einen bestimmten schwarzen Zug wieder Rückgängig
    /// </summary>
    /// <param name="move">Zug, welcher rückgängig gemacht werden soll</param>
    /// <param name="lastBoardInfos">Spielbrettinformationen der vorherigen Stellung</param>
    void DoMoveBlackBackward(Move move, BoardInfo lastBoardInfos)
    {
      // --- Figur zurückziehen ---
      var piece = fields[move.toPos];
      fields[move.fromPos] = piece; // Figur zurücksetzen
      BlackIndexUpdate(piece, move.toPos, move.fromPos);
      fields[move.toPos] = move.capturePiece; // eventuell geschlagene Figur wiederherstellen
      if (move.capturePiece != Piece.None) WhiteIndexAdd(IndexKey(move.capturePiece, move.toPos));

      Debug.Assert(IndexValidation());

      // --- Bauer Umwandlung: promotion ---
      if (move.promoPiece != Piece.None)
      {
        Debug.Assert(piece == move.promoPiece); // umgewandelte Figur sollte übereinstimmen
        BlackIndexRemove(IndexKey(piece, move.fromPos));
        fields[move.fromPos] = Piece.BlackPawn; // Figur zum Bauern zurück verwandeln :)
        WhiteIndexAdd(IndexKey(Piece.BlackPawn, move.fromPos));
        Debug.Assert(IndexValidation());
      }

      // --- Bauer hat "en passant" geschlagen ---
      if (piece == Piece.BlackPawn
          && move.fromPos % Width != move.toPos % Width
          && move.capturePiece == Piece.None) // hat der Bauer seitlich ein "Nichts" geschlagen? -> der gegenerische Bauer wurde dann ein Feld drüber/drunter entfernt
      {
        Debug.Assert((lastBoardInfos & BoardInfo.EnPassantMask) != BoardInfo.EnPassantNone); // war "en passant" vorher erlaubt?
        fields[(uint)(lastBoardInfos & BoardInfo.EnPassantMask) - Width] = Piece.WhitePawn;
        WhiteIndexAdd(IndexKey(Piece.WhitePawn, (int)((uint)(lastBoardInfos & BoardInfo.EnPassantMask) - Width)));
        Debug.Assert(IndexValidation());
      }

      // --- eine Rochade wurde gemacht ---
      if (piece == Piece.BlackKing && Math.Abs(move.fromPos % Width - move.toPos % Width) > 1) // wurde ein König mehr als 1 Feld seitlich bewegt?
      {
        switch (move.toPos)
        {
          case 2: // schwarze lange Rochade auf der Damen-Seite (O-O-O)
          {
            Debug.Assert(fields[0] == Piece.None && fields[1] == Piece.None && fields[2] == Piece.None && fields[3] == Piece.BlackRook && fields[4] == Piece.BlackKing); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.BlackCanCastleQueenside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[0] = Piece.BlackRook; fields[3] = Piece.None; // Turm zurück in die Ecke setzen
            BlackIndexUpdate(Piece.BlackRook, 3, 0);
          } break;

          case 6: // schwarze kurze Rochade auf der Königs-Seite (O-O)
          {
            Debug.Assert(fields[4] == Piece.BlackKing && fields[5] == Piece.BlackRook && fields[6] == Piece.None && fields[7] == Piece.None);
            Debug.Assert((lastBoardInfos & BoardInfo.BlackCanCastleKingside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[7] = Piece.BlackRook; fields[5] = Piece.None; // Turm zurück in die Ecke setzen
            BlackIndexUpdate(Piece.BlackRook, 5, 7);
          } break;

          default: throw new Exception("invalid move"); // ungültige Rochade?
        }
        Debug.Assert(IndexValidation());
      }

      // --- Spielbrett Infos anpassen ---
      MoveNumber--;
      WhiteMove = false;
      BoardInfos = lastBoardInfos;
    }

    /// <summary>
    /// macht einen bestimmten Zug wieder Rückgängig
    /// </summary>
    /// <param name="move">Zug, welcher rückgängig gemacht werden soll</param>
    /// <param name="lastBoardInfos">Spielbrettinformationen der vorherigen Stellung</param>
    public override void DoMoveBackward(Move move, BoardInfo lastBoardInfos)
    {
      if (WhiteMove)
      {
        DoMoveBlackBackward(move, lastBoardInfos);
      }
      else
      {
        DoMoveWhiteBackward(move, lastBoardInfos);
      }
    }

    /// <summary>
    /// berechnet alle erlaubten weißen Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    IEnumerable<Move> GetWhiteMoves()
    {
      for (int index = 0; index < whitePieceCount; index++)
      {
        var key = whiteIndex[index];
        int pos = key & 0xff;
        var piece = (Piece)(byte)(key >> 8);
        Debug.Assert((piece & Piece.Colors) == Piece.White);

        if (piece == Piece.WhitePawn && pos < Width * 2)
        {
          // Promotion-Zug gefunden? (ein Bauer hat das Ziel erreicht und wird umgewandelt)
          foreach (int movePos in ScanWhiteMove(piece, pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
          {
            var move = new Move(pos, movePos, fields[movePos], Piece.WhiteQueen);
            if (DoMove(move, true)) // gültigen Zug gefunden?
            {
              yield return move;
              // weitere Wahlmöglichkeiten als gültige Züge zurück geben
              move.promoPiece = Piece.WhiteRook;
              yield return move;
              move.promoPiece = Piece.WhiteBishop;
              yield return move;
              move.promoPiece = Piece.WhiteKnight;
              yield return move;
            }
          }
        }
        else
        {
          foreach (int movePos in ScanWhiteMove(piece, pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
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
            if (WhiteCanCastleQueenside // lange Rochade O-O-O möglich?
                && fields[57] == Piece.None && fields[58] == Piece.None && fields[59] == Piece.None // sind die Felder noch frei?
                && !IsChecked(58, Piece.Black) && !IsChecked(59, Piece.Black) && !IsChecked(60, Piece.Black)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[56] == Piece.WhiteRook); // weißer Turm sollte noch in der Ecke stehen
              yield return new Move(pos, pos - 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
            if (WhiteCanCastleKingside // kurze Rochade O-O möglich?
                && fields[61] == Piece.None && fields[62] == Piece.None // sind die Felder noch frei?
                && !IsChecked(60, Piece.Black) && !IsChecked(61, Piece.Black) && !IsChecked(62, Piece.Black)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[63] == Piece.WhiteRook); // weißer Turm solle noch in der Ecke stehen
              yield return new Move(pos, pos + 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
          }
        }
      }
    }

    /// <summary>
    /// berechnet alle erlaubten schwarzen Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    IEnumerable<Move> GetBlackMoves()
    {
      for (int index = 0; index < blackPieceCount; index++)
      //for (int index = blackPieceCount - 1; index >= 0; index--)
      {
        var key = blackIndex[index];
        int pos = key & 0xff;
        var piece = (Piece)(byte)(key >> 8);
        Debug.Assert((piece & Piece.Colors) == Piece.Black);

        if (piece == Piece.BlackPawn && pos >= Height * Width - Width * 2)
        {
          // Promotion-Zug gefunden? (ein Bauer hat das Ziel erreicht und wird umgewandelt)
          foreach (int movePos in ScanBlackMove(piece, pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
          {
            var move = new Move(pos, movePos, fields[movePos], Piece.BlackQueen);
            if (DoMove(move, true)) // gültigen Zug gefunden?
            {
              yield return move;
              // weitere Wahlmöglichkeiten als gültige Züge zurück geben
              move.promoPiece = Piece.BlackRook;
              yield return move;
              move.promoPiece = Piece.BlackBishop;
              yield return move;
              move.promoPiece = Piece.BlackKnight;
              yield return move;
            }
          }
        }
        else
        {
          foreach (int movePos in ScanBlackMove(piece, pos)) // alle theoretischen Bewegungsmöglichkeiten prüfen
          {
            var move = new Move(pos, movePos, fields[movePos], Piece.None);
            if (DoMove(move, true)) // gültigen Zug gefunden?
            {
              yield return move;
            }
          }

          // Rochade-Züge prüfen
          if (pos == 4 && piece == Piece.BlackKing) // der weiße König steht noch auf der Startposition?
          {
            if (BlackCanCastleQueenside // lange Rochade O-O-O möglich?
                && fields[1] == Piece.None && fields[2] == Piece.None && fields[3] == Piece.None // sind die Felder noch frei?
                && !IsChecked(2, Piece.White) && !IsChecked(3, Piece.White) && !IsChecked(4, Piece.White)) // steht der König und seine Laufwege auch nicht im Schach?
            {
              Debug.Assert(fields[0] == Piece.BlackRook); // schwarzer Turm sollte noch in der Ecke stehen
              yield return new Move(pos, pos - 2, Piece.None, Piece.None); // König läuft zwei Felder = Rochade
            }
            if (BlackCanCastleKingside // kurze Rochade O-O möglich?
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
    /// berechnet alle erlaubten Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    public override IEnumerable<Move> GetMoves()
    {
      return WhiteMove ? GetWhiteMoves() : GetBlackMoves();
    }
    #endregion
  }
}
