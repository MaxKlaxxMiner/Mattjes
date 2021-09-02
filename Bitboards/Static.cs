using System;
using System.Diagnostics;
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable UnusedMember.Global

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

    public static PieceType TypeOfP(Piece p)
    {
      return (PieceType)((int)p & 7);
    }

    public static Color ColorOf(Piece p)
    {
      return (Color)((int)p >> 3);
    }

    public static Piece MakePiece(Color c, PieceType pt)
    {
      return (Piece)(((int)c << 3) + pt);
    }

    public static Square MakeSquare(int f, Rank r)
    {
      return (Square)(((int)r << 3) + f);
    }

    public static Rank RelativeRank(Color c, Rank r)
    {
      return (Rank)((int)r ^ ((int)c * 7));
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

    // attackers_to() computes a bitboard of all pieces which attack a given
    // square. Slider attacks use the occupied bitboard to indicate occupancy.

    public static ulong Pieces(Position* pos)
    {
      return pos->byTypeBB[0];
    }

    public static ulong AttackersToOcc(Position* pos, Square s, ulong occupied)
    {
      throw new NotImplementedException();
      //return (attacks_from_pawn(s, BLACK) & pieces_cp(WHITE, PAWN))
      //        | (attacks_from_pawn(s, WHITE) & pieces_cp(BLACK, PAWN))
      //        | (attacks_from_knight(s) & pieces_p(KNIGHT))
      //        | (attacks_bb_rook(s, occupied) & pieces_pp(ROOK, QUEEN))
      //        | (attacks_bb_bishop(s, occupied) & pieces_pp(BISHOP, QUEEN))
      //        | (attacks_from_king(s) & pieces_p(KING));
    }

    public static ulong AttackersTo(Position* pos, Square s)
    {
      return AttackersToOcc(pos, s, Pieces(pos));
    }

    public static void SetCastlingRight(Position* pos, Color c, Square rfrom)
    {
      var kfrom = SquareOf(pos, c, PieceType.King);
      var cs = kfrom < rfrom ? Castling.KingSide : Castling.QueenSide;
      var cr = (Castling)((int)Castling.WhiteOO << ((cs == Castling.QueenSide ? 1 : 0) + 2 * (int)c));

      var kto = RelativeSquare(c, cs == Castling.KingSide ? Square.G1 : Square.C1);
      var rto = RelativeSquare(c, cs == Castling.KingSide ? Square.F1 : Square.D1);

      pos->st->castlingRights |= cr;

      pos->castlingRightsMask[(int)kfrom] |= (byte)cr;
      pos->castlingRightsMask[(int)rfrom] |= (byte)cr;
      pos->castlingRookSquare[(int)cr] = (byte)rfrom;

      for (var s = (Square)Math.Min((int)rfrom, (int)rto); s <= (Square)Math.Max((int)rfrom, (int)rto); s++)
      {
        if (s != kfrom && s != rfrom)
        {
          pos->castlingPath[(int)cr] |= SqBb(s);
        }
      }

      for (var s = (Square)Math.Min((int)kfrom, (int)kto); s <= (Square)Math.Max((int)kfrom, (int)kto); s++)
      {
        if (s != kfrom && s != rfrom)
        {
          pos->castlingPath[(int)cr] |= SqBb(s);
        }
      }
    }

    public static Piece PieceOn(Position* pos, Square s)
    {
      return (Piece)pos->board[(int)s];
    }

    public static void PutPiece(Position* pos, Color c, Piece piece, Square s)
    {
      pos->board[(int)s] = (byte)piece;
      pos->byTypeBB[0] |= SqBb(s);
      pos->byTypeBB[(int)TypeOfP(piece)] |= SqBb(s);
      pos->byColorBB[(int)c] |= SqBb(s);
    }

    public static Square EpSquare(Position* pos)
    {
      return (Square)pos->st->epSquare;
    }

    public static Square RelativeSquare(Color c, Square s)
    {
      return (Square)((int)s ^ (int)c * 56);
    }

    public static int FileOf(Square s)
    {
      return (int)s & 7;
    }

    public static int RankOf(Square s)
    {
      return (int)s >> 3;
    }

    public static int Rule50Count(Position* pos)
    {
      return pos->st->rule50;
    }

    public static int GamePly(Position* pos)
    {
      return pos->gamePly;
    }
  }
}
