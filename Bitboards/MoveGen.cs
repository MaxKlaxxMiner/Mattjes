// ReSharper disable UnusedMember.Global
// ReSharper disable UnusedType.Global

namespace Mattjes.Bitboards
{
  public unsafe class MoveGen : Static
  {
    public static ExtMove* GenerateLegal(Position* pos, ExtMove* list)
    {
      var us = Stm(pos);
      var pinned = BlockersForKing(pos, us) & PiecesC(pos, us);
      var ksq = SquareOf(pos, us, PieceType.King);
      var cur = list;

      list = Checkers(pos) ? GenerateEvasions(pos, list)
                           : GenerateNonEvasions(pos, list);
      while (cur != list)
      {
        if ((pinned != 0 && (pinned & SqBb(FromSq(cur->move))) != 0
             || FromSq(cur->move) == ksq
             || TypeOfM(cur->move) == MoveType.EnPassant)
             && !IsLegal(pos, cur->move))
          cur->move = (--list)->move;
        else
          ++cur;
      }

      return list;
    }
  }
}
