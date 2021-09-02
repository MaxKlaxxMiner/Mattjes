using System;
using System.Text;
// ReSharper disable UnusedMember.Global
// ReSharper disable UnassignedField.Global
// ReSharper disable NotAccessedField.Global
#pragma warning disable 649

namespace Mattjes.Bitboards
{
  public unsafe struct Position
  {
    public Stack* st;
    // Board / game representation.
    public fixed ulong byTypeBB[7]; // no reason to allocate 8 here
    public fixed ulong byColorBB[2];
    public Color sideToMove;
    public fixed byte board[64];
    public fixed byte pieceCount[16];
    public fixed byte castlingRightsMask[64];
    public fixed byte castlingRookSquare[16];
    public fixed ulong castlingPath[16];
    public ulong rootKeyFlip;
    public ushort gamePly;
    public bool hasRepeated;

    //    ExtMove *moveList;

    //    // Relevant mainly to the search of the root position.
    //    RootMoves *rootMoves;
    //public Stack* stack;
    //    uint64_t nodes;
    //    uint64_t tbHits;
    //    uint64_t ttHitAverage;
    //    int pvIdx, pvLast;
    //    int selDepth, nmpMinPly;
    //    Color nmpColor;
    //    Depth rootDepth;
    //    Depth completedDepth;
    //    Score contempt;
    //    int failedHighCnt;

    //    // Pointers to thread-specific tables.
    //    CounterMoveStat *counterMoves;
    //    ButterflyHistory *mainHistory;
    //    LowPlyHistory *lowPlyHistory;
    //    CapturePieceToHistory *captureHistory;
    //    PawnEntry *pawnTable;
    //    MaterialEntry *materialTable;
    //    CounterMoveHistoryStat *counterMoveHistory;

    //    // Thread-control data.
    //    uint64_t bestMoveChanges;
    //    atomic_bool resetCalls;
    //    int callsCnt;
    //    int action;
    //    int threadIdx;
    //#ifndef _WIN32
    //    pthread_t nativeThread;
    //    pthread_mutex_t mutex;
    //    pthread_cond_t sleepCondition;
    //#else
    //    HANDLE nativeThread;
    //    HANDLE startEvent, stopEvent;
    //#endif
    //    void *stackAllocation;

    static readonly string PieceToChar = " PNBRQK  pnbrqk";

    public static string PosFen(Position* pos)
    {
      var sb = new StringBuilder();

      for (int r = 7; r >= 0; r--)
      {
        for (int f = 0; f < 8; f++)
        {
          int cnt;
          for (cnt = 0; f < 8 && Static.PieceOn(pos, (Square)(8 * r + f)) == Piece.None; f++) cnt++;
          if (cnt != 0) sb.Append('0' + cnt);
          if (f < 8) sb.Append(PieceToChar[(int)Static.PieceOn(pos, (Square)(8 * r + f))]);
        }
        if (r > 0) sb.Append('/');
      }

      sb.Append(' ');
      sb.Append(Static.Stm(pos) == Color.White ? 'w' : 'b');
      sb.Append(' ');

      var cr = pos->st->castlingRights;

      if ((cr & Castling.WhiteOO) != Castling.NoCastling) sb.Append('K');
      if ((cr & Castling.WhiteOOO) != Castling.NoCastling) sb.Append('Q');
      if ((cr & Castling.BlackOO) != Castling.NoCastling) sb.Append('k');
      if ((cr & Castling.BlackOOO) != Castling.NoCastling) sb.Append('q');
      if (cr == Castling.NoCastling) sb.Append('-');

      sb.Append(' ');
      if (Static.EpSquare(pos) != Square.Null)
      {
        sb.Append((char)('a' + Static.FileOf(Static.EpSquare(pos))));
        sb.Append((char)('1' + Static.RankOf(Static.EpSquare(pos))));
      }
      else
      {
        sb.Append('-');
      }

      sb.Append(' ').Append(Static.Rule50Count(pos)).Append(' ').Append(1 + (Static.GamePly(pos) - (Static.Stm(pos) == Color.Black ? 1 : 0)) / 2);

      return sb.ToString();
    }

    public static void PosSet(Position* pos, string fen)
    {
      byte col, row, token;
      var sq = Square.A8;

      for (int i = 0; i < 7; i++) pos->byTypeBB[i] = 0;
      pos->byColorBB[0] = 0; pos->byColorBB[1] = 0;
      pos->sideToMove = Color.White;
      for (int i = 0; i < 64; i++) pos->board[i] = 0;
      for (int i = 0; i < 16; i++) pos->pieceCount[i] = 0;
      for (int i = 0; i < 64; i++) pos->castlingRightsMask[i] = 0;
      for (int i = 0; i < 16; i++) pos->castlingRookSquare[i] = 0;
      for (int i = 0; i < 16; i++) pos->castlingPath[i] = 0;
      pos->rootKeyFlip = 0;
      pos->gamePly = 0;
      pos->hasRepeated = false;

      //memset(st, 0, StateSize);
      //for (int i = 0; i < 16; i++)
      //  pos->pieceCount[i] = 0;

      //// Piece placement
      //while ((token = *fen++) && token != ' ') {
      //  if (token >= '0' && token <= '9')
      //    sq += token - '0'; // Advance the given number of files
      //  else if (token == '/')
      //    sq -= 16;
      //  else {
      //    for (int piece = 0; piece < 16; piece++)
      //      if (PieceToChar[piece] == token) {
      //        put_piece(pos, color_of(piece), piece, sq++);
      //        pos->pieceCount[piece]++;
      //        break;
      //      }
      //  }
      //}

      //// Active color
      //token = *fen++;
      //pos->sideToMove = token == 'w' ? WHITE : BLACK;
      //token = *fen++;

      //// Castling availability. Compatible with 3 standards: Normal FEN
      //// standard, Shredder-FEN that uses the letters of the columns on which
      //// the rooks began the game instead of KQkq and also X-FEN standard
      //// that, in case of Chess960, // if an inner rook is associated with
      //// the castling right, the castling tag is replaced by the file letter
      //// of the involved rook, as for the Shredder-FEN.
      //while ((token = *fen++) && !isspace(token)) {
      //  Square rsq;
      //  int c = islower(token) ? BLACK : WHITE;
      //  Piece rook = make_piece(c, ROOK);

      //  token = toupper(token);

      //  if (token == 'K')
      //    for (rsq = relative_square(c, SQ_H1); piece_on(rsq) != rook; --rsq);
      //  else if (token == 'Q')
      //    for (rsq = relative_square(c, SQ_A1); piece_on(rsq) != rook; ++rsq);
      //  else if (token >= 'A' && token <= 'H')
      //    rsq = make_square(token - 'A', relative_rank(c, RANK_1));
      //  else
      //    continue;

      //  set_castling_right(pos, c, rsq);
      //}

      //// En passant square. Ignore if no pawn capture is possible.
      //if (   ((col = *fen++) && (col >= 'a' && col <= 'h'))
      //    && ((row = *fen++) && (row == (stm() == WHITE ? '6' : '3'))))
      //{
      //  st->epSquare = make_square(col - 'a', row - '1');

      //  // We assume a legal FEN, i.e. if epSquare is present, then the previous
      //  // move was a legal double pawn push.
      //  if (!(attackers_to(st->epSquare) & pieces_cp(stm(), PAWN)))
      //    st->epSquare = 0;
      //}
      //else
      //  st->epSquare = 0;

      //// Halfmove clock and fullmove number
      //st->rule50 = strtol(fen, &fen, 10);
      //pos->gamePly = strtol(fen, NULL, 10);

      //// Convert from fullmove starting from 1 to ply starting from 0,
      //// handle also common incorrect FEN with fullmove = 0.
      //pos->gamePly = max(2 * (pos->gamePly - 1), 0) + (stm() == BLACK);

      //pos->chess960 = isChess960;
      //set_state(pos, st);

      //assert(pos_is_ok(pos, &failed_step));
    }

    public static void PrintPos(Position* pos)
    {
      char[] fen = new char[128];
      throw new NotImplementedException();
      //pos_fen(pos, fen);

      //flockfile(stdout);
      //printf("\n +---+---+---+---+---+---+---+---+\n");

      //for (int r = 7; r >= 0; r--) {
      //  for (int f = 0; f <= 7; f++)
      //    printf(" | %c", PieceToChar[pos->board[8 * r + f]]);

      //  printf(" | %d\n +---+---+---+---+---+---+---+---+\n", r + 1);
      //}

      //printf("   a   b   c   d   e   f   g   h\n\nFen: %s\nKey: %16"PRIX64"\nCheckers: ", fen, key());

      //char buf[16];
      //for (Bitboard b = checkers(); b; )
      //  printf("%s ", uci_square(buf, pop_lsb(&b)));

      //if (popcount(pieces()) <= TB_MaxCardinality && !can_castle_cr(ANY_CASTLING)) {
      //  int s1, s2;
      //  int wdl = TB_probe_wdl(pos, &s1);
      //  int dtz = TB_probe_dtz(pos, &s2);
      //  printf("\nTablebases WDL: %4d (%d)\nTablebases DTZ: %4d (%d)", wdl, s1, dtz, s2);
      //  if (s1 && wdl != 0) {
      //    Value dtm = TB_probe_dtm(pos, wdl, &s1);
      //    printf("\nTablebases DTM: %s (%d)", uci_value(buf, dtm), s1);
      //  }
      //}
      //printf("\n");
      //fflush(stdout);
      //funlockfile(stdout);
    }
  }
}
