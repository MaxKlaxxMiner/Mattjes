// ReSharper disable RedundantUsingDirective
// ReSharper disable UnusedMember.Global
// ReSharper disable PartialTypeWithSinglePart
// ReSharper disable UnusedType.Global
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
// ReSharper disable RedundantAssignment

namespace Mattjes
{
  public static partial class Tests
  {
    static int counter;
    static readonly HashSet<ulong> mateHashes = new HashSet<ulong>();

    static void MateReverseInternal(IBoard b, Piece[] pieces, int piecesIndex, Func<IBoard, bool> validPosition)
    {
      Debug.Assert(pieces.Length > 0);
      Debug.Assert(piecesIndex < pieces.Length);

      var piece = pieces[piecesIndex++];

      for (int pos = 0; pos < IBoard.Width * IBoard.Height; pos++)
      {
        if (b.GetField(pos) != Piece.None) continue; // Feld schon besetzt?

        b.SetField(pos, piece);

        if ((piece & Piece.White) != Piece.None || !b.IsChecked(b.GetKingPos(Piece.White), Piece.Black)) // der eigene König darf nicht durch eine gegnerische Figur selbst im Schach stehen
        {
          if (BoardTools.IsMate(b) && validPosition(b))
          {
            counter++;
            if (mateHashes.Add(b.GetChecksum()))
            {
              //BoardTools.PrintBoard(b);
              //PrintMateCounter();
              //Console.ReadLine();
            }
          }
          if (piecesIndex < pieces.Length)
          {
            MateReverseInternal(b, pieces, piecesIndex, validPosition);
          }
        }

        b.SetField(pos, Piece.None);
      }
    }

    static void PrintMateCounter()
    {
      Console.WriteLine();
      Console.WriteLine("    Mate-Counter: {0:N0}", counter);
      Console.WriteLine();
      Console.WriteLine("    Unique-Mates: {0:N0}", mateHashes.Count);
      Console.WriteLine();
    }

    public static void MateReverse()
    {
      //IBoard b = new BoardReference();
      IBoard b = new BoardKingOptimized3();

      Func<IBoard, bool> validPosition = board => true;

      //var pieces = new[] { Piece.WhiteQueen };
      //var pieces = new[] { Piece.WhiteQueen, Piece.WhiteQueen };
      //var pieces = new[] { Piece.WhiteRook };
      //var pieces = new[] { Piece.WhiteRook, Piece.WhiteRook };
      //var pieces = new[] { Piece.WhiteBishop, Piece.WhiteBishop };
      //var pieces = new[] { Piece.WhiteKnight, Piece.WhiteKnight };
      //var pieces = new[] { Piece.WhiteKnight, Piece.WhiteKnight, Piece.WhiteKnight };

      // --- Springer + Läufer Mattsuche ---
      var pieces = new[] { Piece.WhiteBishop, Piece.WhiteKnight };
      validPosition = board => // Prüfung, damit nur schwarze-feldrige Läufer erlaubt sind
      {
        for (int i = 0; i < IBoard.Width * IBoard.Height; i++)
        {
          if (board.GetField(i) == Piece.WhiteBishop && BoardTools.IsWhiteField(i)) return false;
        }
        return true;
      };

      // --- Dame gegen Springer + Läufer ---
      //var pieces = new[] { Piece.WhiteQueen, Piece.BlackBishop, Piece.BlackKnight };
      //validPosition = board => // Prüfung, damit nur schwarze-feldrige Läufer erlaubt sind
      //{
      //  for (int i = 0; i < IBoard.Width * IBoard.Height; i++)
      //  {
      //    if (board.GetField(i) == Piece.BlackBishop && BoardTools.IsWhiteField(i)) return false;
      //  }
      //  return true;
      //};

      b.Clear();
      b.WhiteMove = false;

      Console.WriteLine();
      var kingPairs = BoardTools.IterateKingPairs().ToArray();
      int statusTick = 0;
      for (var i = 0; i < kingPairs.Length; i++)
      {
        b.SetField(kingPairs[i].Key, Piece.BlackKing);
        b.SetField(kingPairs[i].Value, Piece.WhiteKing);

        MateReverseInternal(b, pieces, 0, validPosition);

        b.SetField(kingPairs[i].Key, Piece.None);
        b.SetField(kingPairs[i].Value, Piece.None);

        if (Environment.TickCount > statusTick || i == kingPairs.Length - 1)
        {
          statusTick = Environment.TickCount + 250;
          Console.WriteLine("    scan mate positions: {0:N1} % (found: {1:N0})", i * 100.0 / kingPairs.Length, mateHashes.Count);
        }
      }

      PrintMateCounter();
    }
  }
}
