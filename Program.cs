using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Mattjes
{
  class Program
  {
    static void PrintMarkedBoard(Board b, IEnumerable<int> marker)
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

    static void RandomGame(Board b, bool prioCatch = false, bool prioCastling = false)
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
        if (prioCatch) // 2x Wahrscheinlichkeit bei Zügen, welche eine Figur schlagen
        {
          moves = moves.Where(x => x.capturePiece != Piece.None)
            .Concat(moves.Where(x => x.capturePiece != Piece.None))
            .Concat(moves.Where(x => x.capturePiece == Piece.None)).ToArray();
        }
        if (prioCastling && (b.blackCanCastleKingside || b.blackCanCastleQueenside || b.whiteCanCastleKingside || b.whiteCanCastleQueenside)) // 2x Wahrscheinlichkeit bei Zügen, welche eine Rochade begünstigen
        {
          var tmpGood = moves
            .Where(x => x.fromPos != 0 && x.fromPos != 7 && x.fromPos != 56 && x.fromPos != 63)
            .Where(x => x.fromPos == 1 || x.fromPos == 2 || x.fromPos == 3 || x.fromPos == 4 && (x.toPos == 2 || x.toPos == 6) || x.fromPos == 5 || x.fromPos == 6 || x.fromPos == 7
                          || x.fromPos == 57 || x.fromPos == 58 || x.fromPos == 59 || x.fromPos == 60 && (x.toPos == 58 || x.toPos == 62) || x.fromPos == 61 || x.fromPos == 62).ToArray();
          if (tmpGood.Length > 0)
          {
            moves = tmpGood;
          }
          else
          {
            tmpGood = moves.Where(x => x.fromPos != 0 && x.fromPos != 4 && x.fromPos != 7 && x.fromPos != 56 && x.fromPos != 60 && x.fromPos != 63).ToArray();
            if (tmpGood.Length > 0) moves = tmpGood;
          }
        }
        else if (prioCastling && prioCatch) // Figuren schlagen nochmals verdoppeln...
        {
          moves = moves.Where(x => x.capturePiece != Piece.None)
            .Concat(moves.Where(x => x.capturePiece != Piece.None))
            .Concat(moves.Where(x => x.capturePiece == Piece.None)).ToArray();
        }
        int next = rnd.Next(moves.Length);
        Console.WriteLine("    selected: " + moves[next]);
        if (!b.DoMove(moves[next])) throw new Exception("invalid move?");
        Console.WriteLine();
        PrintMarkedBoard(b, new[] { (int)moves[next].fromPos, moves[next].toPos });
        Console.ReadLine();
      }
    }

    static void LolGame(Board b)
    {
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

        Console.WriteLine("    selected: " + moves[next]);
        if (!b.DoMove(moves[next])) throw new Exception("invalid move?");
        Console.WriteLine();
        PrintMarkedBoard(b, new[] { (int)moves[next].fromPos, moves[next].toPos });
        Console.ReadLine();
      }
    }

    static void Main(string[] args)
    {
      Console.WriteLine();
      Console.WriteLine();

      var b = new Board();

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

      //RandomGame(b, true, true);
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
