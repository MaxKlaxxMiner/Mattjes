// ReSharper disable RedundantUsingDirective
// ReSharper disable UnusedMember.Global
// ReSharper disable PartialTypeWithSinglePart
// ReSharper disable UnusedType.Global
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;

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
        if (BoardTools.IsMate(b) && validPosition(b))
        {
          counter++;
          ulong cs = b.GetChecksum();
          if (!mateHashes.Add(cs)) continue;
          //Console.WriteLine();
          //BoardTools.PrintBoard(b);
          //Console.WriteLine();
          //Console.ReadLine();
        }
        if (piecesIndex < pieces.Length)
        {
          MateReverseInternal(b, pieces, piecesIndex, validPosition);
        }

        b.SetField(pos, Piece.None);
      }
    }

    public static void MateReverse()
    {
      //IBoard b = new BoardReference();
      IBoard b = new BoardKingOptimized3();

      //var pieces = new[] { Piece.WhiteQueen };
      //var pieces = new[] { Piece.WhiteRook };
      var pieces = new[] { Piece.WhiteRook, Piece.WhiteRook };
      //var pieces = new[] { Piece.WhiteBishop, Piece.WhiteBishop };
      Func<IBoard, bool> validPosition = board => true;

      b.Clear();
      b.WhiteMove = false;

      foreach (var kingPairs in BoardTools.IterateKingPairs())
      {
        b.SetField(kingPairs.Key, Piece.BlackKing);
        b.SetField(kingPairs.Value, Piece.WhiteKing);

        MateReverseInternal(b, pieces, 0, validPosition);

        b.SetField(kingPairs.Key, Piece.None);
        b.SetField(kingPairs.Value, Piece.None);
      }

      Console.WriteLine();
      Console.WriteLine("    Mate-Counter: {0:N0}", counter);
      Console.WriteLine();
      Console.WriteLine("    Unique-Mates: {0:N0}", mateHashes.Count);
      Console.WriteLine();
    }
  }
}
