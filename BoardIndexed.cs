using System;
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
    /// fügt eine weiße Figur dem Index hinzu
    /// </summary>
    /// <param name="piece">Figur, welche hinzugefügt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void WhiteIndexAdd(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.White);     // Figur muss von der weißen Farbe sein
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(whitePieceCount < whiteIndex.Length);

      ushort add = (ushort)((byte)piece << 8 | pos);

      int insert = whitePieceCount++;
      while (insert > 0 && add < whiteIndex[insert - 1])
      {
        whiteIndex[insert] = whiteIndex[insert - 1];
        insert--;
      }
      whiteIndex[insert] = add;
    }

    /// <summary>
    /// fügt eine schwarze Figur dem Index hinzu
    /// </summary>
    /// <param name="piece">Figur, welche hinzugefügt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void BlackIndexAdd(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.Black);     // Figur muss von der schwarzen Farbe sein
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(blackPieceCount < blackIndex.Length);

      ushort add = (ushort)((byte)piece << 8 | pos);

      int insert = blackPieceCount++;
      while (insert > 0 && add < blackIndex[insert - 1])
      {
        blackIndex[insert] = blackIndex[insert - 1];
        insert--;
      }
      blackIndex[insert] = add;
    }

    /// <summary>
    /// fügt eine Figur dem Index hinzu
    /// </summary>
    /// <param name="piece">Figur, welche hinzugefügt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void IndexAdd(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) != Piece.None);      // Figur muss eine gültige Farbe haben
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben

      if ((piece & Piece.Colors) == Piece.White)
      {
        WhiteIndexAdd(piece, pos);
      }
      else
      {
        BlackIndexAdd(piece, pos);
      }
    }

    /// <summary>
    /// entfernt eine weiße Figur aus dem Index
    /// </summary>
    /// <param name="piece">Figur, welche entfernt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void WhiteIndexRemove(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.White);     // Figur muss von der weißen Farbe sein
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(whitePieceCount > 0);

      ushort del = (ushort)((byte)piece << 8 | pos);
      for (int i = 0; i < whiteIndex.Length; i++)
      {
        if (whiteIndex[i] == del)
        {
          for (int f = i + 1; f < whitePieceCount; f++)
          {
            whiteIndex[f - 1] = whiteIndex[f];
          }
          whitePieceCount--;
          return;
        }
        Debug.Assert(i < whitePieceCount);
      }
      throw new Exception("Index-Entry not found!");
    }

    /// <summary>
    /// entfernt eine schwarze Figur aus dem Index
    /// </summary>
    /// <param name="piece">Figur, welche entfernt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void BlackIndexRemove(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) == Piece.Black);     // Figur muss von der schwarzen Farbe sein
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben
      Debug.Assert(blackPieceCount > 0);

      ushort del = (ushort)((byte)piece << 8 | pos);
      for (int i = 0; i < blackIndex.Length; i++)
      {
        if (blackIndex[i] == del)
        {
          for (int f = i + 1; f < blackPieceCount; f++)
          {
            blackIndex[f - 1] = blackIndex[f];
          }
          blackPieceCount--;
          return;
        }
        Debug.Assert(i < blackPieceCount);
      }
      throw new Exception("Index-Entry not found!");
    }

    /// <summary>
    /// entfernt eine Figur aus dem Index
    /// </summary>
    /// <param name="piece">Figur, welche entfernt werden soll</param>
    /// <param name="pos">Position der Figur</param>
    void IndexRemove(Piece piece, int pos)
    {
      Debug.Assert((piece & Piece.Colors) != Piece.None);      // Figur muss eine gültige Farbe haben
      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // Figur muss einen gültigen Wert haben

      if ((piece & Piece.Colors) == Piece.White)
      {
        WhiteIndexRemove(piece, pos);
      }
      else
      {
        BlackIndexRemove(piece, pos);
      }
    }

    /// <summary>
    /// aktualisiert eine Figur im Index
    /// </summary>
    /// <param name="piece">Figur, welche aktualisiert werden soll</param>
    /// <param name="oldPos">alte Position der Figur</param>
    /// <param name="newPos">neue Position der Figur</param>
    void IndexUpdate(Piece piece, int oldPos, int newPos)
    {
      IndexRemove(piece, oldPos);
      IndexAdd(piece, newPos);
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
      if (oldPiece != Piece.None) IndexRemove(oldPiece, pos);

      Debug.Assert(IndexValidation());

      fields[pos] = piece;
      if (piece != Piece.None) IndexAdd(piece, pos);

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
    /// prüft die theoretischen Bewegungsmöglichkeiten einer Spielfigur auf einem bestimmten Feld
    /// </summary>
    /// <param name="pos">Position auf dem Spielfeld mit der zu testenden Figur</param>
    /// <returns>Aufzählung der theretisch begehbaren Felder</returns>
    IEnumerable<int> ScanMove(int pos)
    {
      var piece = fields[pos];
      if (piece == Piece.None) yield break; // keine Figur auf dem Spielfeld?
      var color = piece & Piece.Colors;
      Debug.Assert(color == (WhiteMove ? Piece.White : Piece.Black)); // passt die Figur-Farbe zum Zug?

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
            if (posX > 0 && (EnPassantPos == pos - (Width + 1) || (fields[pos - (Width + 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width + 1); // nach links-oben schlagen
            if (posX < Width - 1 && (EnPassantPos == pos - (Width - 1) || (fields[pos - (Width - 1)] & Piece.Colors) == Piece.Black)) yield return pos - (Width - 1); // nach rechts-oben schlagen
          }
          else // schwarzer Bauer = nach unten laufen
          {
            if (fields[pos + Width] == Piece.None) // Laufweg frei?
            {
              yield return pos + Width;
              if (posY == 1 && fields[pos + Width * 2] == Piece.None) yield return pos + Width * 2; // Doppelzug
            }
            if (posX > 0 && (EnPassantPos == pos + (Width - 1) || (fields[pos + (Width - 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width - 1); // nach links-unten schlagen
            if (posX < Width - 1 && (EnPassantPos == pos + (Width + 1) || (fields[pos + (Width + 1)] & Piece.Colors) == Piece.White)) yield return pos + (Width + 1); // nach rechts-unten schlagen
          }
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
      kingColor |= Piece.King;
      for (int i = 0; i < fields.Length; i++)
      {
        if (fields[i] == kingColor) return i;
      }
      return -1;
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
        IndexAdd((Piece)b, i);
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
    /// führt einen Zug durch und gibt true zurück, wenn dieser erfolgreich war
    /// </summary>
    /// <param name="move">Zug, welcher ausgeführt werden soll</param>
    /// <param name="onlyCheck">optional: gibt an, dass der Zug nur geprüft aber nicht durchgeführt werden soll (default: false)</param>
    /// <returns>true, wenn erfolgreich, sonst false</returns>
    public override bool DoMove(Move move, bool onlyCheck = false)
    {
      var piece = fields[move.fromPos];

      Debug.Assert((piece & Piece.BasicPieces) != Piece.None); // ist eine Figur auf dem Feld vorhanden?
      Debug.Assert(fields[move.toPos] == move.capturePiece); // stimmt die zu schlagende Figur mit dem Spielfeld überein?
      Debug.Assert((move.capturePiece & Piece.Colors) != (piece & Piece.Colors)); // wird keine eigene Figur gleicher Farbe geschlagen?
      Debug.Assert((piece & Piece.Colors) == (WhiteMove ? Piece.White : Piece.Black)); // passt die Figur-Farbe zum Zug?

      // --- Zug durchführen ---
      if (fields[move.toPos] != Piece.None) IndexRemove(fields[move.toPos], move.toPos);
      fields[move.toPos] = piece;
      fields[move.fromPos] = Piece.None;
      IndexUpdate(piece, move.fromPos, move.toPos);

      Debug.Assert(IndexValidation());

      if (move.toPos == EnPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer schlägt "en passant"?
      {
        Debug.Assert(move.toPos % Width != move.fromPos % Width); // Spalte muss sich ändern
        Debug.Assert(move.capturePiece == Piece.None); // das Zielfeld enhält keine Figur (der geschlagene Bauer ist drüber oder drunter)
        int removePawnPos = WhiteMove ? move.toPos + Width : move.toPos - Width; // Position des zu schlagenden Bauern berechnen
        Debug.Assert(fields[removePawnPos] == (WhiteMove ? Piece.BlackPawn : Piece.WhitePawn)); // es wird ein Bauer erwartet, welcher geschlagen wird

        IndexRemove(fields[removePawnPos], removePawnPos);
        fields[removePawnPos] = Piece.None; // Bauer entfernen

        Debug.Assert(IndexValidation());
      }

      if (move.promoPiece != Piece.None)
      {
        fields[move.toPos] = move.promoPiece;
        IndexRemove(piece, move.toPos);
        IndexAdd(move.promoPiece, move.toPos);

        Debug.Assert(IndexValidation());
      }

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
                  Debug.Assert(BlackCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[0] == Piece.BlackRook && fields[1] == Piece.None && fields[2] == Piece.BlackKing && fields[3] == Piece.None && fields[4] == Piece.None); // Felder prüfen
                  fields[0] = Piece.None; fields[3] = Piece.BlackRook; // Turm bewegen
                  IndexUpdate(Piece.BlackRook, 0, 3);
                } break;
                case 6: // kurze Rochade mit dem schwarzen König
                {
                  Debug.Assert(BlackCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[4] == Piece.None && fields[5] == Piece.None && fields[6] == Piece.BlackKing && fields[7] == Piece.BlackRook); // Felder prüfen
                  fields[7] = Piece.None; fields[5] = Piece.BlackRook; // Turm bewegen
                  IndexUpdate(Piece.BlackRook, 7, 5);
                } break;
                case 58: // lange Rochade mit dem weißen König
                {
                  Debug.Assert(WhiteCanCastleQueenside); // lange Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[56] == Piece.WhiteRook && fields[57] == Piece.None && fields[58] == Piece.WhiteKing && fields[59] == Piece.None && fields[60] == Piece.None); // Felder prüfen
                  fields[56] = Piece.None; fields[59] = Piece.WhiteRook; // Turm bewegen
                  IndexUpdate(Piece.WhiteRook, 56, 59);
                } break;
                case 62: // kurze Rochade mit dem weißen König
                {
                  Debug.Assert(WhiteCanCastleKingside); // kurze Rochade sollte noch erlaubt sein
                  Debug.Assert(fields[60] == Piece.None && fields[61] == Piece.None && fields[62] == Piece.WhiteKing && fields[63] == Piece.WhiteRook); // Felder prüfen
                  fields[63] = Piece.None; fields[61] = Piece.WhiteRook; // Turm bewegen
                  IndexUpdate(Piece.WhiteRook, 63, 61);
                } break;
                default: throw new Exception(); // Rochade war unmöglich
              }
              Debug.Assert(IndexValidation());
              break; // weiterer Schach-Checks nach Rochade nicht notwendig
            }
          }

          if (IsChecked(kingPos, WhiteMove ? Piece.Black : Piece.White)) // prüfen, ob der eigene König vom Gegner angegriffen wird und noch im Schach steht
          {
            // --- Zug rückgängig machen ---
            IndexRemove(fields[move.toPos], move.toPos);
            fields[move.toPos] = move.capturePiece;
            if (move.capturePiece != Piece.None) IndexAdd(move.capturePiece, move.toPos);
            fields[move.fromPos] = piece; IndexAdd(piece, move.fromPos);
            if (move.toPos == EnPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer hat "en passant" geschlagen?
            {
              if (WhiteMove)
              {
                fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
                IndexAdd(Piece.BlackPawn, move.toPos + Width);
              }
              else
              {
                fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
                IndexAdd(Piece.WhitePawn, move.toPos - Width);
              }
            }
            Debug.Assert(IndexValidation());
            return false; // Zug war nicht erlaubt, da der König sonst im Schach stehen würde
          }
          break;
        }
      }

      if (onlyCheck) // Zug sollte nur geprüft werden?
      {
        // --- Zug rückgängig machen ---
        IndexRemove(fields[move.toPos], move.toPos);
        fields[move.toPos] = move.capturePiece;
        if (move.capturePiece != Piece.None) IndexAdd(move.capturePiece, move.toPos);
        fields[move.fromPos] = piece; IndexAdd(piece, move.fromPos);
        if (move.toPos == EnPassantPos && (piece & Piece.Pawn) != Piece.None) // ein Bauer hat "en passant" geschlagen?
        {
          if (WhiteMove)
          {
            fields[move.toPos + Width] = Piece.BlackPawn; // schwarzen Bauer wieder zurück setzen
            IndexAdd(Piece.BlackPawn, move.toPos + Width);
          }
          else
          {
            fields[move.toPos - Width] = Piece.WhitePawn; // weißen Bauer wieder zurück setzen
            IndexAdd(Piece.WhitePawn, move.toPos - Width);
          }
        }
        Debug.Assert(IndexValidation());
        return true;
      }

      EnPassantPos = -1;
      if ((piece & Piece.Pawn) != Piece.None && Math.Abs(move.toPos - move.fromPos) == Width * 2) // wurde ein Bauer zwei Felder weit gezogen -> "en passant" vormerken
      {
        EnPassantPos = (move.fromPos + move.toPos) / 2;
        int posX = EnPassantPos % Width;
        bool opPawn = false;
        if (WhiteMove)
        {
          if (posX > 0 && fields[EnPassantPos - Width - 1] == Piece.BlackPawn) opPawn = true;
          if (posX < Width - 1 && fields[EnPassantPos - Width + 1] == Piece.BlackPawn) opPawn = true;
        }
        else
        {
          if (posX > 0 && fields[EnPassantPos + Width - 1] == Piece.WhitePawn) opPawn = true;
          if (posX < Width - 1 && fields[EnPassantPos + Width + 1] == Piece.WhitePawn) opPawn = true;
        }
        if (!opPawn) EnPassantPos = -1; // kein "en passant" möglich, da kein gegenerischer Bauer in der Nähe ist
      }

      // prüfen, ob durch den Zug Rochaden ungültig werden
      switch (move.fromPos)
      {
        case 0: BlackCanCastleQueenside = false; break; // linker schwarzer Turm wurde mindestens das erste Mal bewegt
        case 4: BlackCanCastleQueenside = false; BlackCanCastleKingside = false; break; // schwarzer König wurde mindestens das erste Mal bewegt
        case 7: BlackCanCastleKingside = false; break; // rechter schwarzer Turm wurde mindestens das erste Mal bewegt
        case 56: WhiteCanCastleQueenside = false; break; // linker weißer Turm wurde mindestens das erste Mal bewegt
        case 60: WhiteCanCastleQueenside = false; WhiteCanCastleKingside = false; break; // weißer König wurde mindestens das erste Mal bewegt
        case 63: WhiteCanCastleKingside = false; break; // rechter weißer Turm wurde mindestens das erste Mal bewegt
      }
      switch (move.toPos)
      {
        case 0: BlackCanCastleQueenside = false; break; // linker schwarzer Turm wurde geschlagen
        case 7: BlackCanCastleKingside = false; break; // rechter schwarzer Turm wurde geschlagen
        case 56: WhiteCanCastleQueenside = false; break; // linker weißer Turm wurde geschlagen
        case 63: WhiteCanCastleKingside = false; break; // rechter weißer Turm wurde geschlagen
      }

      WhiteMove = !WhiteMove; // Farbe welchseln, damit der andere Spieler am Zug ist
      HalfmoveClock++;
      if (piece == Piece.Pawn || move.capturePiece != Piece.None) HalfmoveClock = 0; // beim Bauernzug oder Schlagen einer Figur: 50-Züge Regel zurücksetzen
      if (WhiteMove) MoveNumber++; // Züge weiter hochzählen

      return true;
    }

    /// <summary>
    /// macht einen bestimmten Zug wieder Rückgängig (info: <see cref="IBoard.HalfmoveClock"/> kann nach dem Rückwärts-Zug einen negativen Wert enthalten, da dieser nicht immer rekonstruiert werden kann)
    /// </summary>
    /// <param name="move">Zug, welcher rückgängig gemacht werden soll</param>
    /// <param name="lastBoardInfos">Spielbrettinformationen der vorherigen Stellung</param>
    public override void DoMoveBackward(Move move, BoardInfo lastBoardInfos)
    {
      // --- Figur zurückziehen ---
      var piece = fields[move.toPos];
      fields[move.fromPos] = piece; // Figur zurücksetzen
      IndexUpdate(piece, move.toPos, move.fromPos);
      fields[move.toPos] = move.capturePiece; // eventuell geschlagene Figur wiederherstellen
      if (move.capturePiece != Piece.None) IndexAdd(move.capturePiece, move.toPos);

      Debug.Assert(IndexValidation());

      // --- Bauer Umwandlung: promotion ---
      if (move.promoPiece != Piece.None)
      {
        Debug.Assert(piece == move.promoPiece); // umgewandelte Figur sollte übereinstimmen
        IndexRemove(fields[move.fromPos], move.fromPos);
        fields[move.fromPos] = (piece & Piece.Colors) | Piece.Pawn; // Figur zum Bauern zurück verwandeln :)
        IndexAdd(fields[move.fromPos], move.fromPos);
        Debug.Assert(IndexValidation());
      }

      // --- Bauer hat "en passant" geschlagen ---
      if ((piece & Piece.Pawn) != Piece.None
          && move.fromPos % Width != move.toPos % Width
          && move.capturePiece == Piece.None) // hat der Bauer seitlich ein "Nichts" geschlagen? -> der gegenerische Bauer wurde dann ein Feld drüber/drunter entfernt
      {
        Debug.Assert((lastBoardInfos & BoardInfo.EnPassantMask) != BoardInfo.EnPassantNone); // war "en passant" vorher erlaubt?
        if (WhiteMove)
        {
          fields[(uint)(lastBoardInfos & BoardInfo.EnPassantMask) - Width] = Piece.WhitePawn;
          IndexAdd(Piece.WhitePawn, (int)((uint)(lastBoardInfos & BoardInfo.EnPassantMask) - Width));
        }
        else
        {
          fields[(uint)(lastBoardInfos & BoardInfo.EnPassantMask) + Width] = Piece.BlackPawn;
          IndexAdd(Piece.BlackPawn, (int)((uint)(lastBoardInfos & BoardInfo.EnPassantMask) + Width));
        }
        Debug.Assert(IndexValidation());
      }

      // --- eine Rochade wurde gemacht ---
      if ((piece & Piece.King) != Piece.None && Math.Abs(move.fromPos % Width - move.toPos % Width) > 1) // wurde ein König mehr als 1 Feld seitlich bewegt?
      {
        switch (move.toPos)
        {
          case 2: // schwarze lange Rochade auf der Damen-Seite (O-O-O)
          {
            Debug.Assert(fields[0] == Piece.None && fields[1] == Piece.None && fields[2] == Piece.None && fields[3] == Piece.BlackRook && fields[4] == Piece.BlackKing); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.BlackCanCastleQueenside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[0] = Piece.BlackRook; fields[3] = Piece.None; // Turm zurück in die Ecke setzen
            IndexUpdate(Piece.BlackRook, 3, 0);
          } break;

          case 6: // schwarze kurze Rochade auf der Königs-Seite (O-O)
          {
            Debug.Assert(fields[4] == Piece.BlackKing && fields[5] == Piece.BlackRook && fields[6] == Piece.None && fields[7] == Piece.None);
            Debug.Assert((lastBoardInfos & BoardInfo.BlackCanCastleKingside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[7] = Piece.BlackRook; fields[5] = Piece.None; // Turm zurück in die Ecke setzen
            IndexUpdate(Piece.BlackRook, 5, 7);
          } break;

          case 58: // weiße lange Rochade auf der Damen-Seite (O-O-O)
          {
            Debug.Assert(fields[56] == Piece.None && fields[57] == Piece.None && fields[58] == Piece.None && fields[59] == Piece.WhiteRook && fields[60] == Piece.WhiteKing); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.WhiteCanCastleQueenside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[56] = Piece.WhiteRook; fields[59] = Piece.None; // Turm zurück in die Ecke setzen
            IndexUpdate(Piece.WhiteRook, 59, 56);
          } break;

          case 62: // weiße kurze Rochade auf der Königs-Seite (O-O)
          {
            Debug.Assert(fields[60] == Piece.WhiteKing && fields[61] == Piece.WhiteRook && fields[62] == Piece.None && fields[63] == Piece.None); // passen die Felder?
            Debug.Assert((lastBoardInfos & BoardInfo.WhiteCanCastleKingside) != BoardInfo.None); // war Rochade vorher erlaubt?
            fields[63] = Piece.WhiteRook; fields[61] = Piece.None; // Turm zurück in die Ecke setzen
            IndexUpdate(Piece.WhiteRook, 61, 63);
          } break;

          default: throw new Exception("invalid move"); // König hat sich seltsam bewegt
        }
        Debug.Assert(IndexValidation());
      }

      // --- Spielbrett Infos anpassen ---
      if (WhiteMove) MoveNumber--;
      WhiteMove = !WhiteMove;
      BoardInfos = lastBoardInfos;
    }

    /// <summary>
    /// berechnet alle erlaubten Zugmöglichkeiten und gibt diese zurück
    /// </summary>
    /// <returns>Aufzählung der Zugmöglichkeiten</returns>
    public override IEnumerable<Move> GetMoves()
    {
      var color = WhiteMove ? Piece.White : Piece.Black;

      for (int pos = 0; pos < fields.Length; pos++)
      {
        var piece = fields[pos];
        if ((piece & Piece.Colors) != color) continue; // Farbe der Figur passt nicht zum Zug oder das Feld ist leer

        if ((piece & Piece.Pawn) != Piece.None && ((pos < Width * 2 && WhiteMove) || (pos >= Height * Width - Width * 2 && !WhiteMove)))
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
          else if (pos == 4 && piece == Piece.BlackKing) // der weiße König steht noch auf der Startposition?
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
    #endregion
  }
}
