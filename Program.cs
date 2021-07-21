using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Mattjes
{
  class Program
  {
    static void Main(string[] args)
    {
      var b = new Board();
      b.SetField(0, Piece.BlackRook);
      b.SetField(1, Piece.BlackKnight);
      b.SetField(2, Piece.BlackBishop);
      b.SetField(3, Piece.BlackQueen);
      b.SetField(4, Piece.BlackKing);
      b.SetField(5, Piece.BlackBishop);
      b.SetField(6, Piece.BlackKnight);
      b.SetField(7, Piece.BlackRook);

      b.SetField(8, Piece.BlackPawn);
      b.SetField(9, Piece.BlackPawn);
      b.SetField(10, Piece.BlackPawn);
      b.SetField(11, Piece.BlackPawn);
      b.SetField(12, Piece.BlackPawn);
      b.SetField(13, Piece.BlackPawn);
      b.SetField(14, Piece.BlackPawn);
      b.SetField(15, Piece.BlackPawn);

      b.SetField(48, Piece.WhitePawn);
      b.SetField(49, Piece.WhitePawn);
      b.SetField(50, Piece.WhitePawn);
      b.SetField(51, Piece.WhitePawn);
      b.SetField(52, Piece.WhitePawn);
      b.SetField(53, Piece.WhitePawn);
      b.SetField(54, Piece.WhitePawn);
      b.SetField(55, Piece.WhitePawn);

      b.SetField(56, Piece.WhiteRook);
      b.SetField(57, Piece.WhiteKnight);
      b.SetField(58, Piece.WhiteBishop);
      b.SetField(59, Piece.WhiteQueen);
      b.SetField(60, Piece.WhiteKing);
      b.SetField(61, Piece.WhiteBishop);
      b.SetField(62, Piece.WhiteKnight);
      b.SetField(63, Piece.WhiteRook);

      b.whiteCanCastleKingside = true;
      b.whiteCanCastleQueenside = true;
      b.blackCanCastleKingside = true;
      b.blackCanCastleQueenside = true;

      Console.WriteLine();
      Console.WriteLine(b);
      Console.WriteLine();
      Console.WriteLine("    " + b.GetFEN());
      Console.WriteLine();
    }
  }
}
