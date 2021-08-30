// ReSharper disable UnassignedField.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes.Bitboards
{
  public unsafe struct Stack
  {
    //      // Copied when making a move
    //#ifndef NNUE_PURE
    //      Key pawnKey;
    //#endif
    //      Key materialKey;
    //#ifndef NNUE_PURE
    //      Score psq;
    //#endif
    //      union {
    //        uint16_t nonPawnMaterial[2];
    //        uint32_t nonPawn;
    //      };
    //      union {
    //        struct {
    //          uint8_t pliesFromNull;
    //          uint8_t rule50;
    //        };
    //        uint16_t plyCounters;
    //      };
    //      uint8_t castlingRights;

    //      // Not copied when making a move
    //      uint8_t capturedPiece;
    //      uint8_t epSquare;
    //      Key key;
    public ulong checkersBB;

    //      // Original search stack data
    //      Move* pv;
    //      PieceToHistory *history;
    //      Move currentMove;
    //      Move excludedMove;
    //      Move killers[2];
    //      Value staticEval;
    //      Value statScore;
    //      int moveCount;
    //      bool ttPv;
    //      bool ttHit;
    //      uint8_t ply;

    //      // MovePicker data
    //      uint8_t stage;
    //      uint8_t recaptureSquare;
    //      uint8_t mp_ply;
    //      Move countermove;
    //      Depth depth;
    //      Move ttMove;
    //      Value threshold;
    //      Move mpKillers[2];
    //      ExtMove *cur, *endMoves, *endBadCaptures;

    //      // CheckInfo data
    public fixed ulong blockersForKing[2];
    //      union {
    //        struct {
    //          Bitboard pinnersForKing[2];
    //        };
    //        struct {
    //          Bitboard dummy;           // pinnersForKing[WHITE]
    //          Bitboard checkSquares[7]; // element 0 is pinnersForKing[BLACK]
    //        };
    //      };
    //      Square ksq;

    //#ifdef NNUE
    //      // NNUE data
    //      Accumulator accumulator;
    //      DirtyPiece dirtyPiece;
    //#endif
  }
}
