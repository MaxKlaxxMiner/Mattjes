using System;
using System.Diagnostics;
// ReSharper disable MemberCanBePrivate.Global

namespace Mattjes.Bitboards
{
  public unsafe class Static
  {
    public static Color Stm(Position* pos)
    {
      return pos->sideToMove;
    }

    public static ulong BlockersForKing(Position* pos, Color c)
    {
      return pos->st->blockersForKing[(int)c];
    }

    public static uint CountTrailingZeros(ulong input)
    {
      if (input == 0) return 64;

      uint n = 0;

      if ((input & 0xffffffff) == 0) { n = 32; input >>= 32; }
      if ((input & 0xffff) == 0) { n += 16; input >>= 16; }
      if ((input & 0xff) == 0) { n += 8; input >>= 8; }
      if ((input & 0xf) == 0) { n += 4; input >>= 4; }
      if ((input & 3) == 0) { n += 2; input >>= 2; }
      if ((input & 1) == 0) { ++n; }

      return n;
    }

    public static Square Lsb(ulong b)
    {
      Debug.Assert(b != 0);
      return (Square)CountTrailingZeros(b);
    }

    public static ulong PiecesP(Position* pos, PieceType p)
    {
      return pos->byTypeBB[(int)p];
    }

    public static ulong PiecesC(Position* pos, Color c)
    {
      return pos->byColorBB[(int)c];
    }

    public static ulong PiecesCp(Position* pos, Color c, PieceType p)
    {
      return PiecesP(pos, p) & PiecesC(pos, c);
    }

    public static Square SquareOf(Position* pos, Color c, PieceType p)
    {
      return Lsb(PiecesCp(pos, c, p));
    }

    public static bool Checkers(Position* pos)
    {
      return pos->st->checkersBB != 0;
    }

    public static ExtMove* GenerateEvasions(Position* pos, ExtMove* list)
    {
      throw new NotImplementedException();
    }

    public static ExtMove* GenerateNonEvasions(Position* pos, ExtMove* list)
    {
      throw new NotImplementedException();
    }

    public static Square FromSq(Move m)
    {
      return (Square)(((uint)m >> 6) & 0x3f);
    }

    public static ulong SqBb(Square s)
    {
      throw new NotImplementedException();
      //return SquareBB[s];
    }

    public static MoveType TypeOfM(Move m)
    {
      return (MoveType)((uint)m >> 14);
    }

    public static bool IsLegal(Position* pos, Move m)
    {
      throw new NotImplementedException();
      //assert(move_is_ok(m));

      //Color us = stm();
      //Square from = from_sq(m);
      //Square to = to_sq(m);

      //assert(color_of(moved_piece(m)) == us);
      //assert(piece_on(square_of(us, KING)) == make_piece(us, KING));

      //// En passant captures are a tricky special case. Because they are rather
      //// uncommon, we do it simply by testing whether the king is attacked after
      //// the move is made.
      //if (unlikely(type_of_m(m) == ENPASSANT)) {
      //  Square ksq = square_of(us, KING);
      //  Square capsq = to ^ 8;
      //  Bitboard occupied = pieces() ^ sq_bb(from) ^ sq_bb(capsq) ^ sq_bb(to);

      //  assert(to == ep_square());
      //  assert(moved_piece(m) == make_piece(us, PAWN));
      //  assert(piece_on(capsq) == make_piece(!us, PAWN));
      //  assert(piece_on(to) == 0);

      //  return   !(attacks_bb_rook  (ksq, occupied) & pieces_cpp(!us, QUEEN, ROOK))
      //        && !(attacks_bb_bishop(ksq, occupied) & pieces_cpp(!us, QUEEN, BISHOP));
      //}

      //// Check legality of castling moves.
      //if (unlikely(type_of_m(m) == CASTLING)) {
      //  // to > from works both for standard chess and for Chess960.
      //  to = relative_square(us, to > from ? SQ_G1 : SQ_C1);
      //  int step = to > from ? WEST : EAST;

      //  for (Square s = to; s != from; s += step)
      //    if (attackers_to(s) & pieces_c(!us))
      //      return false;

      //  // For Chess960, verify that moving the castling rook does not discover
      //  // some hidden checker, e.g. on SQ_A1 when castling rook is on SQ_B1.
      //  return !is_chess960() || !(blockers_for_king(pos, us) & sq_bb(to_sq(m)));
      //}

      //// If the moving piece is a king, check whether the destination
      //// square is attacked by the opponent. Castling moves are checked
      //// for legality during move generation.
      //if (pieces_p(KING) & sq_bb(from))
      //  return !(attackers_to_occ(pos, to, pieces() ^ sq_bb(from)) & pieces_c(!us));

      //// A non-king move is legal if and only if it is not pinned or it
      //// is moving along the ray towards or away from the king.
      //return   !(blockers_for_king(pos, us) & sq_bb(from))
      //      ||  aligned(m, square_of(us, KING));
    }

    public static Piece PieceOn(Position* pos, Square s)
    {
      return (Piece)pos->board[(int)s];
    }
  }
}
