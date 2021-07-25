using System;
using System.Collections.Generic;
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

    static int[] ScanMovePoints(IBoard b, Move[] moves, int depth)
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
                if (((Board)b).IsChecked(kingPos, Piece.Black))
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
                if (((Board)b).IsChecked(kingPos, Piece.White))
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
            if (b.WhiteMove) result[i] += ScanMovePoints(b, nextMoves, depth - 1).Max(); else result[i] += ScanMovePoints(b, nextMoves, depth - 1).Min();
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
          points = ScanMovePoints(b, moves, maxDepth);
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

    static void Main(string[] args)
    {
      Console.WriteLine();
      Console.WriteLine();

      IBoard b = new Board();

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

      LolGame(b);

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
