﻿using System;
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
          case Piece.WhitePawn: r += 100; r += (6 - pos / IBoard.Width) * (6 - pos / IBoard.Width); break;
          case Piece.BlackPawn: r -= 100; r -= (pos / IBoard.Width - 1) * (pos / IBoard.Width - 1); break;
        }
      }
      return r;
    }

    static long nodeCounter;

    static int[] ScanMovePointsSlowFen(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      string backupFen = b.GetFEN();

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

        nodeCounter++;

        var nextMoves = b.GetMoves().ToArray();

        if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
        {
          for (int kingPos = 0; kingPos < IBoard.Width * IBoard.Height; kingPos++)
          {
            if (b.WhiteMove)
            {
              if (b.GetField(kingPos) == Piece.WhiteKing)
              {
                if (b.IsChecked(kingPos, Piece.Black))
                {
                  result[i] -= 25000 + depth * 100; // Matt
                }
                else
                {
                  result[i] += 1000; // Patt
                }
                break;
              }
            }
            else
            {
              if (b.GetField(kingPos) == Piece.BlackKing)
              {
                if (b.IsChecked(kingPos, Piece.White))
                {
                  result[i] += 25000 + depth * 100; // Matt
                }
                else
                {
                  result[i] -= 1000; // Patt
                }
                break;
              }
            }
          }
        }
        else
        {
          if (depth > 0)
          {
            if (b.WhiteMove) result[i] += ScanMovePointsSlowFen(b, nextMoves, depth - 1).Max(); else result[i] += ScanMovePointsSlowFen(b, nextMoves, depth - 1).Min();
          }
          else
          {
            result[i] += PiecePoints(b);
            if (b.WhiteMove) result[i] += nextMoves.Length; else result[i] -= nextMoves.Length;
          }
        }

        b.SetFEN(backupFen);
      }

      return result;
    }

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

    static int[] ScanMovePointsFastFen(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      byte[] backupFen = new byte[64];
      b.GetFastFen(backupFen, 0);

      if (depth == 0)
      {
        for (int i = 0; i < moves.Length; i++)
        {
          if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

          nodeCounter++;

          int moveCount = b.GetMoves().Count();
          if (moveCount == 0)
          {
            result[i] += EndCheck(b, depth);
          }
          else
          {
            result[i] += PiecePoints(b);
            if (b.WhiteMove) result[i] += moveCount; else result[i] -= moveCount;
          }

          b.SetFastFen(backupFen, 0);
        }

        return result;
      }

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

        nodeCounter++;

        var nextMoves = b.GetMoves().ToArray();

        if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
        {
          result[i] += EndCheck(b, depth);
        }
        else
        {
          if (b.WhiteMove) result[i] += ScanMovePointsFastFen(b, nextMoves, depth - 1).Max(); else result[i] += ScanMovePointsFastFen(b, nextMoves, depth - 1).Min();
        }

        b.SetFastFen(backupFen, 0);
      }

      return result;
    }

    static readonly Dictionary<ulong, int[]> HashTable = new Dictionary<ulong, int[]>();

    static int[] ScanMovePointsHashed(IBoard b, Move[] moves, int depth)
    {
      var result = new int[moves.Length];

      byte[] backupFen = new byte[64];
      b.GetFastFen(backupFen, 0);

      if (depth == 0)
      {
        for (int i = 0; i < moves.Length; i++)
        {
          if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

          nodeCounter++;

          int moveCount = b.GetMoves().Count();
          if (moveCount == 0)
          {
            result[i] += EndCheck(b, depth);
          }
          else
          {
            result[i] += PiecePoints(b);
            if (b.WhiteMove) result[i] += moveCount; else result[i] -= moveCount;
          }

          b.SetFastFen(backupFen, 0);
        }

        return result;
      }

      for (int i = 0; i < moves.Length; i++)
      {
        if (!b.DoMove(moves[i])) throw new Exception("invalid move?");

        nodeCounter++;

        ulong checkSum = b.GetFullChecksum();

        var nextMoves = b.GetMoves().ToArray();

        if (nextMoves.Length == 0) // keine weiteren Zugmöglichkeit?
        {
          result[i] += EndCheck(b, depth);
        }
        else
        {
          if (b.WhiteMove) result[i] += ScanMovePointsFastFen(b, nextMoves, depth - 1).Max(); else result[i] += ScanMovePointsFastFen(b, nextMoves, depth - 1).Min();
        }

        b.SetFastFen(backupFen, 0);
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
          points = ScanMovePointsFastFen(b, moves, maxDepth);
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

    // --- Reference ---
    // [0]      0,2 ms   -20, -20, -20, -20, -19, -19, -19, -19, -19, -19, -19, -19, -16, -16, -16, -16, -16, -16, -16, -16 (119.109 nps)
    // [1]      2,1 ms    16,  16,  16,  16,  16,  18,  18,  18,  18,  18,  19,  20,  20,  20,  20,  21,  23,  26,  27,  29 (195.570 nps)
    // [2]     40,9 ms   -29, -29, -29, -28, -28, -28, -28, -28, -28, -28, -28, -27, -24, -24, -24, -24, -23, -22, -22, -22 (227.922 nps)
    // [3]  1.114,5 ms    13,  15,  15,  15,  15,  20,  20,  20,  20,  21,  21,  24,  24,  24,  25,  26,  27,  28,  29,  33 (185.381 nps)
    // [4] 22.711,4 ms   -35, -33, -32, -32, -30, -29, -28, -27, -25, -25, -23, -22,  -9,  -9,  54,  57,  66,  69,  71,  78 (223.333 nps)

    // --- Reference + FastFen ---
    // [0]      0,1 ms   -20, -20, -20, -20, -19, -19, -19, -19, -19, -19, -19, -19, -16, -16, -16, -16, -16, -16, -16, -16 (223.466 nps)
    // [1]      1,7 ms    16,  16,  16,  16,  16,  18,  18,  18,  18,  18,  19,  20,  20,  20,  20,  21,  23,  26,  27,  29 (254.105 nps)
    // [2]     28,7 ms   -29, -29, -29, -28, -28, -28, -28, -28, -28, -28, -28, -27, -24, -24, -24, -24, -23, -22, -22, -22 (324.682 nps)
    // [3]    836,6 ms    13,  15,  15,  15,  15,  20,  20,  20,  20,  21,  21,  24,  24,  24,  25,  26,  27,  28,  29,  33 (246.967 nps)
    // [4] 15.967,3 ms   -35, -33, -32, -32, -30, -29, -28, -27, -25, -25, -23, -22,  -9,  -9,  54,  57,  66,  69,  71,  78 (317.663 nps)


    static void SpeedCheck(IBoard b)
    {
      var moves = b.GetMoves().ToArray();
      Console.WriteLine("    moves: " + moves.Length);

      for (int maxDepth = 0; maxDepth < 100; maxDepth++)
      {
        for (int r = 0; r < 5; r++)
        {
          var time = Stopwatch.StartNew();
          nodeCounter = 0;
          var points = ScanMovePointsFastFen(b, moves, maxDepth);
          time.Stop();
          double durationMs = time.ElapsedTicks * 1000.0 / Stopwatch.Frequency;
          Console.WriteLine("    [{0}]{1,9:N1} ms  {2} ({3:N0} nps)", maxDepth, durationMs, string.Join(",", points.OrderBy(x => x).Select(x => x.ToString("N0").PadLeft(4))), nodeCounter / durationMs * 1000.0);
        }
      }

    }

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
