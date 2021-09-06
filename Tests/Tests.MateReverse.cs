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

    static void MateReverseInternal(IBoard b, Piece[] pieces, int piecesIndex)
    {
      Debug.Assert(pieces.Length > 0);
      Debug.Assert(piecesIndex < pieces.Length);
      piecesIndex++;

      if (piecesIndex < pieces.Length)
      {
        throw new NotImplementedException();
      }

      var piece = pieces[pieces.Length - 1];

      for (int pos = 0; pos < IBoard.Width * IBoard.Height; pos++)
      {
        if (b.GetField(pos) != Piece.None) continue; // Feld schon besetzt?

        b.SetField(pos, piece);
        if (BoardTools.IsMate(b))
        {
          Console.WriteLine();
          BoardTools.PrintBoard(b);
          Console.WriteLine("    Counter: {0:N0}", ++counter);
          //Console.ReadLine();

        }
        b.SetField(pos, Piece.None);
      }
    }

    public static void MateReverse()
    {
      //IBoard b = new BoardReference();
      IBoard b = new BoardKingOptimized3();

      var pieces = new[] { Piece.WhiteQueen };

      b.Clear();
      b.WhiteMove = false;

      foreach (var kingPairs in BoardTools.IterateKingPairs())
      {
        b.SetField(kingPairs.Key, Piece.BlackKing);
        b.SetField(kingPairs.Value, Piece.WhiteKing);

        MateReverseInternal(b, pieces, 0);

        b.SetField(kingPairs.Key, Piece.None);
        b.SetField(kingPairs.Value, Piece.None);
      }

    }
  }
}
