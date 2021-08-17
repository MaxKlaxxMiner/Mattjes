using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
// ReSharper disable UnusedMember.Local

namespace Mattjes
{
  class Program
  {
    #region # // --- base functions ---
    static void PrintMarkedBoard(IBoard b, IEnumerable<int> marker)
    {
      var tmp = b.ToString(marker, (char)0x1000);
      bool last = false;
      foreach (char c in tmp)
      {
        if ((c & 0x1000) != 0)
        {
          if (!last)
          {
            Console.ForegroundColor = ConsoleColor.Yellow;
            Console.BackgroundColor = ConsoleColor.DarkGray;
          }
          Console.Write((char)(c & 0xff));
          last = true;
        }
        else
        {
          if (last)
          {
            Console.ForegroundColor = ConsoleColor.Gray;
            Console.BackgroundColor = ConsoleColor.Black;
          }
          Console.Write(c);
          last = false;
        }
      }
    }

    static int PiecePoints(IBoard b)
    {
      int r = 0;
      for (var pos = 0; pos < IBoard.Width * IBoard.Height; pos++)
      {
        var p = b.GetField(pos);
        switch (p)
        {
          case Piece.BlackKing: if (pos == 2 || pos == 6) r -= 10; break;
          case Piece.WhiteKing: if (pos == 58 || pos == 62) r += 10; break;
          case Piece.WhiteQueen: r += 900; break;
          case Piece.BlackQueen: r -= 900; break;
          case Piece.WhiteRook: r += 500; break;
          case Piece.BlackRook: r -= 500; break;
          //case Piece.WhiteBishop: r += 300; break;
          case Piece.WhiteBishop: r += 300; if (pos == 58 || pos == 61) r--; break;
          //case Piece.BlackBishop: r -= 300; break;
          case Piece.BlackBishop: r -= 300; if (pos == 2 || pos == 5) r++; break;
          //case Piece.WhiteKnight: r += 300; break;
          case Piece.WhiteKnight: r += 300; if (pos == 57 || pos == 62) r -= 2; break;
          //case Piece.BlackKnight: r -= 300; break;
          case Piece.BlackKnight: r -= 300; if (pos == 1 || pos == 6) r += 2; break;
          //case Piece.WhitePawn: r += 100; break;
          //case Piece.BlackPawn: r -= 100; break;
          case Piece.WhitePawn: r += 100; r += (6 - pos / IBoard.Width) * (6 - pos / IBoard.Width); break;
          case Piece.BlackPawn: r -= 100; r -= (pos / IBoard.Width - 1) * (pos / IBoard.Width - 1); break;
        }
      }
      return r;
    }

    static int PiecePointsSimple(IBoard b)
    {
      int r = 0;
      for (var pos = 0; pos < IBoard.Width * IBoard.Height; pos++)
      {
        var p = b.GetField(pos);
        switch (p)
        {
          case Piece.WhiteQueen: r += 900; break;
          case Piece.BlackQueen: r -= 900; break;
          case Piece.WhiteRook: r += 500; break;
          case Piece.BlackRook: r -= 500; break;
          case Piece.WhiteBishop: r += 300; break;
          case Piece.BlackBishop: r -= 300; break;
          case Piece.WhiteKnight: r += 300; break;
          case Piece.BlackKnight: r -= 300; break;
          case Piece.WhitePawn: r += 100; break;
          case Piece.BlackPawn: r -= 100; break;
        }
      }
      return r;
    }

    static long nodeCounter;

    static int EndCheck(IBoard b, int depth)
    {
      if (b.WhiteMove)
      {
        return b.IsChecked(b.GetKingPos(Piece.White), Piece.Black) ? -25000 - depth * 100 : 1000;
      }
      else
      {
        return b.IsChecked(b.GetKingPos(Piece.Black), Piece.White) ? 25000 + depth * 100 : -1000;
      }
    }

    static bool IsChecked(IBoard b)
    {
      if (b.WhiteMove)
      {
        return b.IsChecked(b.GetKingPos(Piece.White), Piece.Black);
      }
      else
      {
        return b.IsChecked(b.GetKingPos(Piece.Black), Piece.White);
      }
    }
    #endregion

    #region # // --- Hash-Test ---
    static readonly Dictionary<ulong, int> HashTable = new Dictionary<ulong, int>();

    static int[] ScanMovePointsHashed(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      //var backupFen = new byte[64];
      //b.GetFastFen(backupFen, 0);
      var lastBoardInfos = b.BoardInfos;

      if (depth == 0)
      {
        for (int i = 0; i < moves.Length; i++)
        {
          if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

          nodeCounter++;

          ulong checkSum = b.GetFullChecksum();
          int points;
          if (!HashTable.TryGetValue(checkSum, out points))
          {
            int moveCount = b.GetMoves().Count();
            if (moveCount == 0)
            {
              points = EndCheck(b, depth);
            }
            else
            {
              points = PiecePoints(b);
              if (b.WhiteMove) points += moveCount; else points -= moveCount;
            }
            HashTable.Add(checkSum, points);
          }
          result[i] += points;

          b.DoMoveBackward(moves[i], lastBoardInfos);
          //b.SetFastFen(backupFen, 0);
        }

        return result;
      }

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

        nodeCounter++;

        ulong checkSum = b.GetFullChecksum();
        int points;
        if (!HashTable.TryGetValue(checkSum, out points))
        {
          var nextMoves = b.GetMoves().ToArray();

          if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
          {
            points = EndCheck(b, depth);
          }
          else
          {
            var pointArray = ScanMovePointsHashed(b, nextMoves, depth - 1);
            if (b.WhiteMove)
            {
              points = int.MinValue;
              foreach (int point in pointArray) if (point > points) points = point;
            }
            else
            {
              points = int.MaxValue;
              foreach (int point in pointArray) if (point < points) points = point;
            }
          }
          HashTable.Add(checkSum, points);
        }
        result[i] += points;

        b.DoMoveBackward(moves[i], lastBoardInfos);
        //b.SetFastFen(backupFen, 0);
      }

      return result;
    }
    #endregion

    #region # // --- Alpha/Beta Search ---
    static int ScanMovePointsMaxInternal(IBoard b, int depth, int alpha, int beta)
    {
      if (depth == 0)
      {
        int moveCount = b.GetMoves().Count();
        return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) + moveCount;
      }

      depth--;
      var nextMoves = b.GetMoves().ToArray();

      if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = alpha;

      foreach (var move in nextMoves)
      {
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMinInternal(b, depth, points, beta);
        if (point > points) points = point;

        b.DoMoveBackward(move, lastBoardInfos);

        if (points >= beta) break;
      }
      return points;
    }

    static int ScanMovePointsMinInternal(IBoard b, int depth, int alpha, int beta)
    {
      if (depth == 0)
      {
        int moveCount = b.GetMoves().Count();
        return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) - moveCount;
      }

      depth--;
      var nextMoves = b.GetMoves().ToArray();

      if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = beta;

      foreach (var move in nextMoves)
      {
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMaxInternal(b, depth, alpha, points);
        if (point < points) points = point;

        b.DoMoveBackward(move, lastBoardInfos);

        if (points <= alpha) break;
      }
      return points;
    }

    static int[] ScanMovePointsAlphaBeta(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      var lastBoardInfos = b.BoardInfos;

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");
        nodeCounter++;

        if (b.WhiteMove)
        {
          result[i] = ScanMovePointsMaxInternal(b, depth, int.MinValue, int.MaxValue);
        }
        else
        {
          result[i] = ScanMovePointsMinInternal(b, depth, int.MinValue, int.MaxValue);
        }

        b.DoMoveBackward(moves[i], lastBoardInfos);
      }

      return result;
    }

    static int[] ScanMovePointsAlphaBetaShort(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];
      for (int i = 0; i < result.Length; i++) result[i] = b.WhiteMove ? int.MinValue : int.MaxValue;

      var lastBoardInfos = b.BoardInfos;
      var nextMoves = moves.ToArray();
      if (depth > 3)
      {
        var tmpPoints = ScanMovePointsAlphaBetaShort(b, moves, 3);
        nextMoves = Enumerable.Range(0, moves.Length).OrderBy(i => tmpPoints[i]).Select(i => moves[i]).ToArray();
        if (b.WhiteMove) nextMoves = nextMoves.Reverse().ToArray();
      }

      int points;
      if (b.WhiteMove)
      {
        points = int.MinValue;
        for (var i = 0; i < nextMoves.Length; i++)
        {
          var move = nextMoves[i];
          if (!b.DoMove(move)) throw new Exception("invalid move?");
          nodeCounter++;

          int point = ScanMovePointsMinInternal(b, depth, points, int.MaxValue);
          if (point > points)
          {
            if (i > 0) // Zug nach vorne sortieren?
            {
              for (int sort = i; sort > 0; sort--)
              {
                nextMoves[sort] = nextMoves[sort - 1];
              }
              nextMoves[0] = move;
            }
            points = point;
          }

          b.DoMoveBackward(move, lastBoardInfos);
        }
      }
      else
      {
        points = int.MaxValue;
        for (var i = 0; i < nextMoves.Length; i++)
        {
          var move = nextMoves[i];
          if (!b.DoMove(move)) throw new Exception("invalid move?");
          nodeCounter++;

          int point = ScanMovePointsMaxInternal(b, depth, int.MinValue, points);
          if (point < points)
          {
            if (i > 0) // Zug nach vorne sortieren?
            {
              for (int sort = i; sort > 0; sort--)
              {
                nextMoves[sort] = nextMoves[sort - 1];
              }
              nextMoves[0] = move;
            }
            points = point;
          }

          b.DoMoveBackward(move, lastBoardInfos);
        }
      }

      for (int i = 0; i < moves.Length; i++)
      {
        if (moves[i].ToString() == nextMoves[0].ToString())
        {
          result[i] = points;
        }
      }
      return result;
    }
    #endregion

    #region # // --- Alpha/Beta Search + Move-Cache ---
    static readonly byte[] moveCache = new byte[2000 * 1048576];
    static uint moveCacheLen;
    static IntPtr moveCacheFix = IntPtr.Zero;
    static readonly Dictionary<ulong, uint> moveCacheDict = new Dictionary<ulong, uint>();

    static unsafe IList<Move> GetMoveCacheMoves(IBoard b, ulong checksum)
    {
      uint moveOfs;
      if (moveCacheDict.TryGetValue(checksum, out moveOfs))
      {
        return new MoveList((byte*)moveCacheFix + moveOfs);
      }

      if (moveCacheLen == moveCache.Length)
      {
        return b.GetMoves().ToArray();
      }

      moveOfs = moveCacheLen;
      moveCacheDict.Add(checksum, moveOfs);
      var moveList = new MoveList((byte*)moveCacheFix + moveOfs, b.GetMoves());
      moveCacheLen += moveList.ByteLength;
      Debug.Assert(moveCacheLen < moveCache.Length);

      if (moveCache.Length - moveCacheLen < 1024)
      {
        moveCacheLen = (uint)moveCache.Length;
      }

      return moveList;
    }

    static int ScanMovePointsMaxInternalMoveCache(IBoard b, int depth, int alpha, int beta)
    {
      if (depth == 0)
      {
        return b.GetMoves().Any() ? PiecePoints(b) : EndCheck(b, depth);
        //int moveCount = GetMoveCacheMoves(b, b.GetChecksum()).Count;
        //return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) + moveCount;
      }

      depth--;
      var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
      if (nextMoves.Count == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = alpha;

      for (var i = 0; i < nextMoves.Count; i++)
      {
        var move = nextMoves[i];
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMinInternalMoveCache(b, depth, points, beta);
        if (point > points)
        {
          if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
          {
            for (int sort = i; sort > 0; sort--)
            {
              nextMoves[sort] = nextMoves[sort - 1];
            }
            nextMoves[0] = move;
          }
          points = point;
        }

        b.DoMoveBackward(move, lastBoardInfos);

        if (points >= beta) break;
      }

      return points;
    }

    static int ScanMovePointsMinInternalMoveCache(IBoard b, int depth, int alpha, int beta)
    {
      if (depth == 0)
      {
        return b.GetMoves().Any() ? PiecePoints(b) : EndCheck(b, depth);
        //int moveCount = GetMoveCacheMoves(b, b.GetChecksum()).Count;
        //return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) - moveCount;
      }

      depth--;
      var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
      if (nextMoves.Count == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = beta;

      for (var i = 0; i < nextMoves.Count; i++)
      {
        var move = nextMoves[i];
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMaxInternalMoveCache(b, depth, alpha, points);
        if (point < points)
        {
          points = point;
          if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
          {
            for (int sort = i; sort > 0; sort--)
            {
              nextMoves[sort] = nextMoves[sort - 1];
            }
            nextMoves[0] = move;
          }
        }

        b.DoMoveBackward(move, lastBoardInfos);

        if (points <= alpha) break;
      }

      return points;
    }

    static unsafe int[] ScanMovePointsAlphaBetaMoveCache(IBoard b, Move[] moves, int depth)
    {
      fixed (byte* ptr = moveCache)
      {
        moveCacheFix = (IntPtr)ptr;

        var result = new int[moves.Length];

        var lastBoardInfos = b.BoardInfos;

        for (int i = 0; i < moves.Length; i++)
        {
          if (!b.DoMove(moves[i])) throw new Exception("invalid move?");
          nodeCounter++;

          if (b.WhiteMove)
          {
            result[i] = ScanMovePointsMaxInternalMoveCache(b, depth, int.MinValue, int.MaxValue);
          }
          else
          {
            result[i] = ScanMovePointsMinInternalMoveCache(b, depth, int.MinValue, int.MaxValue);
          }

          b.DoMoveBackward(moves[i], lastBoardInfos);
        }
        return result;
      }
    }

    static unsafe int[] ScanMovePointsAlphaBetaMoveCacheShort(IBoard b, Move[] moves, int depth)
    {
      if (depth < 3) return ScanMovePointsAlphaBetaMoveCache(b, moves, depth);
      if (depth == 3)
      {
        var movesCopy = moves.ToArray();
        var result = ScanMovePointsAlphaBetaMoveCache(b, moves, depth);
        var resultCopy = result.ToArray();
        for (int y = 0; y < movesCopy.Length; y++)
        {
          for (int x = 1; x < movesCopy.Length; x++)
          {
            if (b.WhiteMove)
            {
              if (resultCopy[x] > resultCopy[x - 1])
              {
                int t = resultCopy[x]; resultCopy[x] = resultCopy[x - 1]; resultCopy[x - 1] = t;
                var m = movesCopy[x]; movesCopy[x] = movesCopy[x - 1]; movesCopy[x - 1] = m;
              }
            }
            else
            {
              if (resultCopy[x] < resultCopy[x - 1])
              {
                int t = resultCopy[x]; resultCopy[x] = resultCopy[x - 1]; resultCopy[x - 1] = t;
                var m = movesCopy[x]; movesCopy[x] = movesCopy[x - 1]; movesCopy[x - 1] = m;
              }
            }
          }
        }
        var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
        Debug.Assert(movesCopy.Length == nextMoves.Count);
        for (int i = 0; i < movesCopy.Length; i++)
        {
          nextMoves[i] = movesCopy[i];
        }

        return result;
      }

      fixed (byte* ptr = moveCache)
      {
        moveCacheFix = (IntPtr)ptr;

        var result = new int[moves.Length];
        for (int i = 0; i < result.Length; i++) result[i] = b.WhiteMove ? int.MinValue : int.MaxValue;

        var lastBoardInfos = b.BoardInfos;

        var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
        int points;
        if (b.WhiteMove)
        {
          points = int.MinValue;
          for (var i = 0; i < nextMoves.Count; i++)
          {
            var move = nextMoves[i];
            if (!b.DoMove(move)) throw new Exception("invalid move?");
            nodeCounter++;

            int point = ScanMovePointsMinInternalMoveCache(b, depth, points, int.MaxValue);
            if (point > points)
            {
              if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
              {
                for (int sort = i; sort > 0; sort--)
                {
                  nextMoves[sort] = nextMoves[sort - 1];
                }
                nextMoves[0] = move;
              }
              points = point;
            }

            b.DoMoveBackward(move, lastBoardInfos);
          }
        }
        else
        {
          points = int.MaxValue;
          for (var i = 0; i < nextMoves.Count; i++)
          {
            var move = nextMoves[i];
            if (!b.DoMove(move)) throw new Exception("invalid move?");
            nodeCounter++;

            int point = ScanMovePointsMaxInternalMoveCache(b, depth, int.MinValue, points);
            if (point < points)
            {
              if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
              {
                for (int sort = i; sort > 0; sort--)
                {
                  nextMoves[sort] = nextMoves[sort - 1];
                }
                nextMoves[0] = move;
              }
              points = point;
            }

            b.DoMoveBackward(move, lastBoardInfos);
          }
        }

        for (int i = 0; i < moves.Length; i++)
        {
          if (moves[i].ToString() == nextMoves[0].ToString())
          {
            result[i] = points;
          }
        }
        return result;
      }
    }

    static int ScanMovePointsMaxInternalMoveCacheDynamic(IBoard b, int depth, int alpha, int beta, List<ulong> oldMoves)
    {
      ulong checksum = b.GetChecksum();
      for (int i = oldMoves.Count - 2; i >= 0; i -= 2) if (oldMoves[i] == checksum) return 0; // Stellungswiederholung = Remis

      if (depth == 0)
      {
        return b.GetMoves().Any() ? PiecePoints(b) : EndCheck(b, depth);
        //int moveCount = b.GetMoves().Count();
        //int moveCount = GetMoveCacheMoves(b, b.GetChecksum()).Count;
        //return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) + moveCount;
      }

      depth--;
      var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
      if (nextMoves.Count == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = alpha;

      oldMoves.Add(checksum);

      for (var i = 0; i < nextMoves.Count; i++)
      {
        var move = nextMoves[i];
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMinInternalMoveCacheDynamic(b, depth == 0 && move.capturePiece != Piece.None || IsChecked(b) ? 1 : depth, points, beta, oldMoves);
        if (point > points)
        {
          if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
          {
            for (int sort = i; sort > 0; sort--)
            {
              nextMoves[sort] = nextMoves[sort - 1];
            }
            nextMoves[0] = move;
          }
          points = point;
        }

        b.DoMoveBackward(move, lastBoardInfos);

        if (points >= beta) break;
      }

      oldMoves.RemoveAt(oldMoves.Count - 1);

      return points;
    }

    static int ScanMovePointsMinInternalMoveCacheDynamic(IBoard b, int depth, int alpha, int beta, List<ulong> oldMoves)
    {
      ulong checksum = b.GetChecksum();
      for (int i = oldMoves.Count - 2; i >= 0; i -= 2) if (oldMoves[i] == checksum) return 0; // Stellungswiederholung = Remis

      if (depth == 0)
      {
        return b.GetMoves().Any() ? PiecePoints(b) : EndCheck(b, depth);
        //int moveCount = b.GetMoves().Count();
        //int moveCount = GetMoveCacheMoves(b, checksum).Count;
        //return moveCount == 0 ? EndCheck(b, depth) : PiecePoints(b) - moveCount;
      }

      depth--;
      var nextMoves = GetMoveCacheMoves(b, checksum);
      if (nextMoves.Count == 0) // keine weiteren Zugmöglichkeit?
      {
        return EndCheck(b, depth);
      }

      var lastBoardInfos = b.BoardInfos;
      int points = beta;

      oldMoves.Add(checksum);

      for (var i = 0; i < nextMoves.Count; i++)
      {
        var move = nextMoves[i];
        if (!b.DoMove(move)) throw new Exception("invalid move?");
        nodeCounter++;

        int point = ScanMovePointsMaxInternalMoveCacheDynamic(b, depth == 0 && move.capturePiece != Piece.None || IsChecked(b) ? 1 : depth, alpha, points, oldMoves);
        if (point < points)
        {
          points = point;
          if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
          {
            for (int sort = i; sort > 0; sort--)
            {
              nextMoves[sort] = nextMoves[sort - 1];
            }
            nextMoves[0] = move;
          }
        }

        b.DoMoveBackward(move, lastBoardInfos);

        if (points <= alpha) break;
      }

      oldMoves.RemoveAt(oldMoves.Count - 1);

      return points;
    }

    static unsafe int[] ScanMovePointsAlphaBetaMoveCacheDynamic(IBoard b, Move[] moves, int depth)
    {
      if (depth < 2) return ScanMovePointsAlphaBetaMoveCache(b, moves, depth);
      if (depth == 2)
      {
        var movesCopy = moves.ToArray();
        var result = ScanMovePointsAlphaBetaMoveCache(b, moves, depth);
        var resultCopy = result.ToArray();
        for (int y = 0; y < movesCopy.Length; y++)
        {
          for (int x = 1; x < movesCopy.Length; x++)
          {
            if (b.WhiteMove)
            {
              if (resultCopy[x] > resultCopy[x - 1])
              {
                int t = resultCopy[x]; resultCopy[x] = resultCopy[x - 1]; resultCopy[x - 1] = t;
                var m = movesCopy[x]; movesCopy[x] = movesCopy[x - 1]; movesCopy[x - 1] = m;
              }
            }
            else
            {
              if (resultCopy[x] < resultCopy[x - 1])
              {
                int t = resultCopy[x]; resultCopy[x] = resultCopy[x - 1]; resultCopy[x - 1] = t;
                var m = movesCopy[x]; movesCopy[x] = movesCopy[x - 1]; movesCopy[x - 1] = m;
              }
            }
          }
        }
        var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
        Debug.Assert(movesCopy.Length == nextMoves.Count);
        for (int i = 0; i < movesCopy.Length; i++)
        {
          nextMoves[i] = movesCopy[i];
        }

        return result;
      }

      fixed (byte* ptr = moveCache)
      {
        moveCacheFix = (IntPtr)ptr;

        var result = new int[moves.Length];
        for (int i = 0; i < result.Length; i++) result[i] = b.WhiteMove ? int.MinValue : int.MaxValue;

        var lastBoardInfos = b.BoardInfos;

        var oldMoves = new List<ulong> { b.GetChecksum() };
        var nextMoves = GetMoveCacheMoves(b, b.GetChecksum());
        int points;
        if (b.WhiteMove)
        {
          points = int.MinValue;
          for (var i = 0; i < nextMoves.Count; i++)
          {
            var move = nextMoves[i];
            if (!b.DoMove(move)) throw new Exception("invalid move?");
            nodeCounter++;

            int point = ScanMovePointsMinInternalMoveCacheDynamic(b, depth, points, int.MaxValue, oldMoves);
            if (point > points)
            {
              if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
              {
                for (int sort = i; sort > 0; sort--)
                {
                  nextMoves[sort] = nextMoves[sort - 1];
                }
                nextMoves[0] = move;
              }
              points = point;
            }

            b.DoMoveBackward(move, lastBoardInfos);
          }
        }
        else
        {
          points = int.MaxValue;
          for (var i = 0; i < nextMoves.Count; i++)
          {
            var move = nextMoves[i];
            if (!b.DoMove(move)) throw new Exception("invalid move?");
            nodeCounter++;

            int point = ScanMovePointsMaxInternalMoveCacheDynamic(b, depth, int.MinValue, points, oldMoves);
            if (point < points)
            {
              if (i > 0 && nextMoves is MoveList) // Zug nach vorne sortieren?
              {
                for (int sort = i; sort > 0; sort--)
                {
                  nextMoves[sort] = nextMoves[sort - 1];
                }
                nextMoves[0] = move;
              }
              points = point;
            }

            b.DoMoveBackward(move, lastBoardInfos);
          }
        }

        for (int i = 0; i < moves.Length; i++)
        {
          if (moves[i].ToString() == nextMoves[0].ToString())
          {
            result[i] = points;
          }
        }
        return result;
      }
    }
    #endregion

    static void SpeedCheck(IBoard b)
    {
      var moves = b.GetMoves().ToArray();
      Console.WriteLine("    moves: " + moves.Length);

      for (int maxDepth = 0; maxDepth < 100; maxDepth++)
      {
        Console.WriteLine();
        for (int r = 0; r < 5; r++)
        {
          var time = Stopwatch.StartNew();
          nodeCounter = 0;
          //var points = ScanMovePointsAlphaBeta(b, moves, maxDepth);
          var points = ScanMovePointsAlphaBetaShort(b, moves, maxDepth);
          //var points = ScanMovePointsAlphaBetaMoveCacheDynamic(b, moves, maxDepth);
          //HashTable.Clear();
          //var points = ScanMovePointsHashed(b, moves, maxDepth);
          time.Stop();
          double durationMs = time.ElapsedTicks * 1000.0 / Stopwatch.Frequency;
          Console.WriteLine("    [{0}]{1,9:N1} ms  {2} ({3:N0} nps)", maxDepth, durationMs, string.Join(",", points.OrderBy(x => x).Select(x =>
            {
              if (x == int.MinValue) return "-".PadLeft(4);
              if (x == int.MaxValue) return "+".PadLeft(4);
              return x.ToString("N0").PadLeft(4);
            }
          )), nodeCounter / durationMs * 1000.0);
        }
      }

    }

    #region # --- Game-Modi ---
    static void LolGame(IBoard b)
    {
      var rnd = new Random(12345);
      bool first = true;
      for (; ; )
      {
        Console.WriteLine();
        Console.WriteLine("    FEN: " + b.GetFEN());
        var moves = b.GetMoves().ToArray();
        Console.WriteLine("    moves: " + moves.Length);

        if (moves.Length == 0)
        {
          Console.WriteLine("   ... end ...");
          return;
        }

        int next = 0;

        if (first) goto loop;

        var pointsList = new List<int[]>();
        int time = Environment.TickCount;
        int maxDepth = 0;
        nodeCounter = 0;
        if (moveCacheLen > moveCache.Length / 2)
        {
          moveCacheLen = 0;
          moveCacheDict.Clear();
          Console.WriteLine("    cleared move-cache");
        }
        if (moves.Length > 1)
        {
          for (; maxDepth < 100; maxDepth++)
          {
            Console.Write("    scan depth " + maxDepth + " ...");
            //pointsList.Add(ScanMovePointsAlphaBetaShort(b, moves, maxDepth));
            pointsList.Add(ScanMovePointsAlphaBetaMoveCacheDynamic(b, moves, maxDepth));
            int duration = Environment.TickCount - time;
            Console.WriteLine(" " + duration.ToString("N0") + " ms");
            if (duration > 5000) break;
          }
        }
        else pointsList.Add(new int[1]);
        time = Environment.TickCount - time + 1;

        int bestNext = b.WhiteMove ? int.MinValue : int.MaxValue;

        for (int i = 0; i < moves.Length; i++)
        {
          int points = pointsList[pointsList.Count - 1][i];
          //if (pointsList.Count > 1) points = (points + pointsList[pointsList.Count - 2][i]) / 2;
          if (b.WhiteMove)
          {
            if (points >= bestNext)
            {
              if (points > bestNext || rnd.Next(2) == 0)
              {
                bestNext = points;
                next = i;
              }
            }
          }
          else
          {
            if (points <= bestNext)
            {
              if (points < bestNext || rnd.Next(2) == 0)
              {
                bestNext = points;
                next = i;
              }
            }
          }
        }

        string ptsStr = (bestNext / 100.0).ToString("N2");
        if (bestNext >= 25000)
        {
          if (b.WhiteMove)
          {
            ptsStr = "+M" + (maxDepth - (bestNext - 25000) / 100 + 2) / 2;
          }
          else
          {
            ptsStr = "-M" + (maxDepth - (bestNext - 25000) / 100 + 1) / 2;
          }
        }
        if (bestNext <= -25000)
        {
          if (b.WhiteMove)
          {
            ptsStr = "-M" + (maxDepth - (-bestNext - 25000) / 100) / 2;
          }
          else
          {
            ptsStr = "+M" + (maxDepth - (-bestNext - 25000) / 100 + 1) / 2;
          }
        }
        Console.WriteLine("    selected: " + moves[next] + " (" + ptsStr + ")");
        Console.WriteLine("    nodes: " + nodeCounter.ToString("N0") + " (" + (nodeCounter * 1000 / time).ToString("N0") + " / s), depth: " + maxDepth);
        if (!b.DoMove(moves[next])) throw new Exception("invalid move?");
      loop:
        Console.WriteLine();
        PrintMarkedBoard(b, first ? new int[] { } : new[] { (int)moves[next].fromPos, moves[next].toPos });
        string fixMove = Console.ReadLine();
        if (fixMove.Length > 0)
        {
          if (fixMove.Length == 4)
          {
            int fromPos = IBoard.PosFromChars(fixMove.Substring(0, 2));
            int toPos = IBoard.PosFromChars(fixMove.Substring(2, 2));
            var m = b.GetMoves().FirstOrDefault(x => x.fromPos == fromPos && x.toPos == toPos);
            if (m.fromPos == fromPos && m.toPos == toPos)
            {
              b.DoMove(m);
              moves[next] = m;
              goto loop;
            }
          }
          Console.WriteLine("    invalid move: " + fixMove);
          goto loop;
        }
        first = false;
      }
    }
    #endregion

    // --- Alpha/Beta short (Default) ---
    // [0]      0,1 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -16 (198.031 nps)
    // [1]      0,9 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  29 (188.415 nps)
    // [2]      9,3 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -22 (324.403 nps)
    // [3]     54,6 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  33 (234.675 nps)
    // [4]    469,0 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  78 (319.221 nps)
    // [5] 10.328,9 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -59 (240.617 nps)

    // --- BoardKingOptimized ---
    // [0]      0,1 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -16 (213.665 nps)
    // [1]      0,7 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  29 (225.008 nps)
    // [2]      8,5 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -22 (352.459 nps)
    // [3]     42,1 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  33 (304.493 nps)
    // [4]    427,5 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  78 (350.218 nps)
    // [5]  7.553,3 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -59 (329.035 nps)

    // --- BoardIndexed (unfinished) ---
    // [0]      0,1 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -16 (181.774 nps)
    // [1]      1,2 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  29 (140.542 nps)
    // [2]     13,2 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -22 (227.994 nps)
    // [3]     78,1 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  33 (164.199 nps)
    // [4]    668,5 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,  78 (223.951 nps)
    // [5] 15.021,4 ms     -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -,   -, -59 (165.451 nps)

    static void Main(string[] args)
    {
      Console.WriteLine();
      Console.WriteLine();

      //IBoard b = new BoardReference();
      IBoard b = new BoardKingOptimized();
      //IBoard b = new BoardIndexed();

      //b.SetFEN("r1bqk1nr/pppp3p/2nb1p2/6p1/2B1P3/4QN2/PPP2PPP/RNB2RK1 b kq - 1 7");

      b.SetFEN("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"); // Startaufstellung
      //b.SetFEN("r3k2r/p2ppp1p/8/4Q3/8/2BB4/PPPPPPPP/R3K2R w KQkq - 0 1"); // Rochaden-Test: alle erlaubt
      //b.SetFEN("r3k1r1/p2ppp1p/8/B7/8/4P3/PPPP1P1P/RN2K2R b KQq - 0 1"); // Rochaden-Test: keine erlaubt

      //b.SetFEN("8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1"); // Dame + Dame Mattsuche (Matt in 3)
      //b.SetFEN("8/8/8/4k3/8/Q7/R7/K7 w - - 0 1"); // Dame + Turm Mattsuche (Matt in 5)
      //b.SetFEN("8/8/8/4k3/8/R7/R7/K7 w - - 0 1"); // Turm + Turm Mattsuche (Matt in 7)
      //b.SetFEN("7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1"); // Dame Mattsuche (Matt in 12)
      //b.SetFEN("8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1"); // Turm Mattsuche (Matt in 15)
      //b.SetFEN("8/8/4k3/8/8/8/8/K2BB3 w - - 0 1"); // Läufer + Läufer Mattsuche (Matt in 17)
      //b.SetFEN("8/8/8/8/3k4/8/N7/KB6 w - - 0 1"); // Läufer + Springer Mattsuche (Matt in 31)
      //b.SetFEN("8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1"); // Dame gegen Läufer + Springer Mattsuche (Matt in 39)
      //b.SetFEN("5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1"); // Bauern-Test (Matt in 6)

      //LolGame(b);
      SpeedCheck(b);

      //Console.WriteLine();
      //foreach (var move in b.GetMoves())
      //{
      //  PrintMarkedBoard(b, new[] { (int)move.fromPos, move.toPos });
      //  if (move.promoPiece != Piece.None) Console.WriteLine("    promotion: " + move.promoPiece);
      //  Console.WriteLine();
      //  Console.WriteLine();
      //}

      Console.WriteLine();
      Console.WriteLine("    " + b.GetFEN());
      Console.WriteLine();
    }
  }
}
