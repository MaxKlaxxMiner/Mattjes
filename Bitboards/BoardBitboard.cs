using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;

namespace Mattjes.Bitboards
{
  public unsafe class BoardBitboard
  {
    readonly Position* pos;

    public BoardBitboard()
    {
      pos = (Position*)Marshal.AllocHGlobal(sizeof(Position));
      //#ifndef NNUE_PURE
      //      pos->pawnTable = calloc(PAWN_ENTRIES * sizeof(PawnEntry), 1);
      //      pos->materialTable = calloc(8192 * sizeof(MaterialEntry), 1);
      //#endif
      //      pos->counterMoves = calloc(sizeof(CounterMoveStat), 1);
      //      pos->mainHistory = calloc(sizeof(ButterflyHistory), 1);
      //      pos->captureHistory = calloc(sizeof(CapturePieceToHistory), 1);
      //      pos->lowPlyHistory = calloc(sizeof(LowPlyHistory), 1);
      //      pos->rootMoves = calloc(sizeof(RootMoves), 1);
      //      pos->stackAllocation = calloc(63 + (MAX_PLY + 110) * sizeof(Stack), 1);
      //      pos->moveList = calloc(10000 * sizeof(ExtMove), 1);
      //pos->stack = (Stack *)(((uintptr_t)pos->stackAllocation + 0x3f) & ~0x3f);
      //  pos->threadIdx = idx;
      //  pos->counterMoveHistory = cmhTables[t];

      //  atomic_store(&pos->resetCalls, false);
      //  pos->selDepth = pos->callsCnt = 0;
    }

    ~BoardBitboard()
    {
      Marshal.FreeHGlobal((IntPtr)pos);
    }
  }
}
