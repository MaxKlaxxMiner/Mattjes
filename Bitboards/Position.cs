// ReSharper disable UnassignedField.Global
// ReSharper disable NotAccessedField.Global

using System;
using System.Text;

#pragma warning disable 649

namespace Mattjes.Bitboards
{
  public unsafe struct Position
  {
    public Stack* st;
    //    // Board / game representation.
    public fixed ulong byTypeBB[7]; // no reason to allocate 8 here
    public fixed ulong byColorBB[2];
    public Color sideToMove;
    //    uint8_t chess960;
    public fixed byte board[64];
    //    uint8_t pieceCount[16];
    //    uint8_t castlingRightsMask[64];
    //    uint8_t castlingRookSquare[16];
    //    Bitboard castlingPath[16];
    //    Key rootKeyFlip;
    //    uint16_t gamePly;
    //    bool hasRepeated;

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

      int cnt;
      for (int r = 7; r >= 0; r--)
      {
        for (int f = 0; f < 8; f++)
        {
          for (cnt = 0; f < 8 && Static.PieceOn(pos, (Square)(8 * r + f)) == Piece.None; f++) cnt++;
          if (cnt != 0) sb.Append('0' + cnt);
          if (f < 8) sb.Append(PieceToChar[(int)Static.PieceOn(pos, (Square)(8 * r + f))]);
        }
        if (r > 0) sb.Append('/');
      }

      sb.Append(' ');
      sb.Append(Static.Stm(pos) == Color.White ? 'w' : 'b');
      sb.Append(' ');

      //int cr = pos->st->castlingRights;

      //if (!is_chess960()) {
      //  if (cr & WHITE_OO) *str++ = 'K';
      //  if (cr & WHITE_OOO) *str++ = 'Q';
      //  if (cr & BLACK_OO) *str++ = 'k';
      //  if (cr & BLACK_OOO) *str++ = 'q';
      //} else {
      //  if (cr & WHITE_OO) *str++ = 'A' + file_of(castling_rook_square(make_castling_right(WHITE, KING_SIDE)));
      //  if (cr & WHITE_OOO) *str++ = 'A' + file_of(castling_rook_square(make_castling_right(WHITE, QUEEN_SIDE)));
      //  if (cr & BLACK_OO) *str++ = 'a' + file_of(castling_rook_square(make_castling_right(BLACK, KING_SIDE)));
      //  if (cr & BLACK_OOO) *str++ = 'a' + file_of(castling_rook_square(make_castling_right(BLACK, QUEEN_SIDE)));
      //}
      //if (!cr)
      //  *str++ = '-';

      //*str++ = ' ';
      //if (ep_square() != 0) {
      //  *str++ = 'a' + file_of(ep_square());
      //  *str++ = '1' + rank_of(ep_square());
      //} else {
      //  *str++ = '-';
      //}

      //sprintf(str, " %d %d", rule50_count(), 1 + (game_ply()-(stm() == BLACK)) / 2);

      return sb.ToString();
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
