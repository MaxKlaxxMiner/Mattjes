using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
// ReSharper disable UnusedMember.Local

namespace Mattjes
{
  class Program
  {
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
          case Piece.WhiteQueen: r += 900; break;
          case Piece.BlackQueen: r -= 900; break;
          case Piece.WhiteRook: r += 500; break;
          case Piece.BlackRook: r -= 500; break;
          case Piece.WhiteBishop: r += 300; break;
          case Piece.BlackBishop: r -= 300; break;
          case Piece.WhiteKnight: r += 300; break;
          case Piece.BlackKnight: r -= 300; break;
          //case Piece.WhitePawn: r += 100; break;
          //case Piece.BlackPawn: r -= 100; break;
          case Piece.WhitePawn: r += 100; r += (6 - pos / IBoard.Width) * (6 - pos / IBoard.Width); break;
          case Piece.BlackPawn: r -= 100; r -= (pos / IBoard.Width - 1) * (pos / IBoard.Width - 1); break;
        }
      }
      return r;
    }

    static long nodeCounter;

    static int EndCheck(IBoard b, int depth)
    {
      for (int kingPos = 0; kingPos < IBoard.Width * IBoard.Height; kingPos++)
      {
        if (b.WhiteMove)
        {
          if (b.GetField(kingPos) == Piece.WhiteKing)
          {
            return b.IsChecked(kingPos, Piece.Black) ? -25000 - depth * 100 : 1000;
          }
        }
        else
        {
          if (b.GetField(kingPos) == Piece.BlackKing)
          {
            return b.IsChecked(kingPos, Piece.White) ? 25000 + depth * 100 : -1000;
          }
        }
      }
      throw new IndexOutOfRangeException();
    }

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

    static int[] ScanMovePointsAlphaBeta(IBoard b, Move[] moves, int depth)
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

          //ulong checkSum = b.GetFullChecksum();
          int points;
          //if (!HashTable.TryGetValue(checkSum, out points))
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
            //HashTable.Add(checkSum, points);
          }
          result[i] = points;

          b.DoMoveBackward(moves[i], lastBoardInfos);
          //b.SetFastFen(backupFen, 0);
        }

        return result;
      }

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

        nodeCounter++;

        //ulong checkSum = b.GetFullChecksum();
        int points;
        //if (!HashTable.TryGetValue(checkSum, out points))
        {
          var nextMoves = b.GetMoves().ToArray();

          if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
          {
            points = EndCheck(b, depth);
          }
          else
          {
            var pointArray = ScanMovePointsAlphaBeta(b, nextMoves, depth - 1);
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
          //HashTable.Add(checkSum, points);
        }
        result[i] = points;

        b.DoMoveBackward(moves[i], lastBoardInfos);
        //b.SetFastFen(backupFen, 0);
      }

      return result;
    }

    static void LolGame(IBoard b)
    {
      var rnd = new Random(12345);
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

        int[] points = null;
        int time = Environment.TickCount;
        int maxDepth;
        nodeCounter = 0;
        for (maxDepth = 0; maxDepth < 100; maxDepth++)
        {
          HashTable.Clear();
          points = ScanMovePointsHashed(b, moves, maxDepth);
          int duration = Environment.TickCount - time;
          if (duration > 2000) break;
        }
        time = Environment.TickCount - time;

        var nextList = new List<int>();
        int bestNext = b.WhiteMove ? int.MinValue : int.MaxValue;
        for (int i = 0; i < points.Length; i++)
        {
          if (b.WhiteMove)
          {
            if (points[i] >= bestNext)
            {
              if (points[i] > bestNext) nextList.Clear();
              bestNext = points[i];
              nextList.Add(i);
            }
          }
          else
          {
            if (points[i] <= bestNext)
            {
              if (points[i] < bestNext) nextList.Clear();
              bestNext = points[i];
              nextList.Add(i);
            }
          }
        }

        int next = nextList[rnd.Next(nextList.Count)];

        Console.WriteLine("    selected: " + moves[next] + " (" + (bestNext / 100.0).ToString("N2") + ")");
        Console.WriteLine("    nodes: " + nodeCounter.ToString("N0") + " (" + (nodeCounter * 1000 / time).ToString("N0") + " / s), depth: " + maxDepth);
        if (!b.DoMove(moves[next])) throw new Exception("invalid move?");
      loop:
        Console.WriteLine();
        PrintMarkedBoard(b, new[] { (int)moves[next].fromPos, moves[next].toPos });
        string fixMove = Console.ReadLine();
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
      }
    }

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
          var points = ScanMovePointsAlphaBeta(b, moves, maxDepth);
          //HashTable.Clear();
          //var points = ScanMovePointsHashed(b, moves, maxDepth);
          time.Stop();
          double durationMs = time.ElapsedTicks * 1000.0 / Stopwatch.Frequency;
          Console.WriteLine("    [{0}]{1,9:N1} ms  {2} ({3:N0} nps)", maxDepth, durationMs, string.Join(",", points.OrderBy(x => x).Select(x => x.ToString("N0").PadLeft(4))), nodeCounter / durationMs * 1000.0);
        }
      }

    }

    static int[] ScanMoveEndgame(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      var backupFen = new byte[64];
      b.GetFastFen(backupFen, 0);

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
            points = b.GetMoves().Any() ? 0 : EndCheck(b, depth);
            HashTable.Add(checkSum, points);
          }
          result[i] = points;

          b.SetFastFen(backupFen, 0);
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
            var pointArray = ScanMoveEndgame(b, nextMoves, depth - 1);
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
        result[i] = points;

        b.SetFastFen(backupFen, 0);
      }

      return result;
    }

    static void MateScan(IBoard b)
    {
      var moves = b.GetMoves().ToArray();
      Console.WriteLine("    moves: " + moves.Length);

      for (int maxDepth = 0; maxDepth < 100; maxDepth++)
      {
        HashTable.Clear();
        Console.WriteLine();
        var time = Stopwatch.StartNew();
        nodeCounter = 0;
        var points = ScanMoveEndgame(b, moves, maxDepth);
        time.Stop();
        double durationMs = time.ElapsedTicks * 1000.0 / Stopwatch.Frequency;
        int max = points.Max();
        int min = points.Min();
        Console.WriteLine("    [{0}]{1,9:N1} ms  {2} ({3:N0} nps)", maxDepth, durationMs, max > 25000 ? "+M" + ((maxDepth - (max - 25000) / 100 + 2) / 2) : "?", nodeCounter / durationMs * 1000.0);
      }
    }

    // --- Reference ---
    // [0]      0,2 ms   -20, -20, -20, -20, -19, -19, -19, -19, -19, -19, -19, -19, -16, -16, -16, -16, -16, -16, -16, -16 (119.109 nps)
    // [1]      2,1 ms    16,  16,  16,  16,  16,  18,  18,  18,  18,  18,  19,  20,  20,  20,  20,  21,  23,  26,  27,  29 (195.570 nps)
    // [2]     40,9 ms   -29, -29, -29, -28, -28, -28, -28, -28, -28, -28, -28, -27, -24, -24, -24, -24, -23, -22, -22, -22 (227.922 nps)
    // [3]  1.114,5 ms    13,  15,  15,  15,  15,  20,  20,  20,  20,  21,  21,  24,  24,  24,  25,  26,  27,  28,  29,  33 (185.381 nps)
    // [4] 22.711,4 ms   -35, -33, -32, -32, -30, -29, -28, -27, -25, -25, -23, -22,  -9,  -9,  54,  57,  66,  69,  71,  78 (223.333 nps)

    // --- Reference + BackMove ---
    // [0]      0,1 ms   -20, -20, -20, -20, -19, -19, -19, -19, -19, -19, -19, -19, -16, -16, -16, -16, -16, -16, -16, -16 (251.111 nps)
    // [1]      1,6 ms    16,  16,  16,  16,  16,  18,  18,  18,  18,  18,  19,  20,  20,  20,  20,  21,  23,  26,  27,  29 (267.458 nps)
    // [2]     27,3 ms   -29, -29, -29, -28, -28, -28, -28, -28, -28, -28, -28, -27, -24, -24, -24, -24, -23, -22, -22, -22 (341.865 nps)
    // [3]    796,4 ms    13,  15,  15,  15,  15,  20,  20,  20,  20,  21,  21,  24,  24,  24,  25,  26,  27,  28,  29,  33 (259.431 nps)
    // [4] 15.545,0 ms   -35, -33, -32, -32, -30, -29, -28, -27, -25, -25, -23, -22,  -9,  -9,  54,  57,  66,  69,  71,  78 (326.293 nps)

    // --- Reference + BackMove + Hashtable ---
    // [0]      0,1 ms   -20, -20, -20, -20, -19, -19, -19, -19, -19, -19, -19, -19, -16, -16, -16, -16, -16, -16, -16, -16 (256.398 nps)
    // [1]      1,6 ms    16,  16,  16,  16,  16,  18,  18,  18,  18,  18,  19,  20,  20,  20,  20,  21,  23,  26,  27,  29 (262.248 nps)
    // [2]     18,8 ms   -29, -29, -29, -28, -28, -28, -28, -28, -28, -28, -28, -27, -24, -24, -24, -24, -23, -22, -22, -22 (496.205 nps)
    // [3]    337,4 ms    13,  15,  15,  15,  15,  20,  20,  20,  20,  21,  21,  24,  24,  24,  25,  26,  27,  28,  29,  33 (378.900 nps)
    // [4]  3.262,7 ms   -35, -33, -32, -32, -30, -29, -28, -27, -25, -25, -23, -22,  -9,  -9,  54,  57,  66,  69,  71,  78 (590.070 nps)

    static void Main(string[] args)
    {
      Console.WriteLine();
      Console.WriteLine();

      IBoard b = new BoardReference();

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
      //MateScan(b);

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
