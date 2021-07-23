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

    static void Main(string[] args)
    {
      Console.WriteLine();
      Console.WriteLine();

      var b = new Board();

      //b.SetFEN("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"); // Startaufstellung
      //b.SetFEN("8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1"); // Dame + Dame Mattsuche (Matt in 3)
      //b.SetFEN("8/8/8/4k3/8/Q7/R7/K7 w - - 0 1"); // Dame + Turm Mattsuche (Matt in 5)
      //b.SetFEN("8/8/8/4k3/8/R7/R7/K7 w - - 0 1"); // Turm + Turm Mattsuche (Matt in 7)
      //b.SetFEN("7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1"); // Dame Mattsuche (Matt in 12)
      //b.SetFEN("8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1"); // Turm Mattsuche (Matt in 15)
      //b.SetFEN("8/8/4k3/8/8/8/8/K2BB3 w - - 0 1"); // Läufer + Läufer Mattsuche (Matt in 17)
      //b.SetFEN("8/8/8/8/3k4/8/N7/KB6 w - - 0 1"); // Läufer + Springer Mattsuche (Matt in 31)
      //b.SetFEN("8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1"); // Dame gegen Läufer + Springer Mattsuche (Matt in 39)
      b.SetFEN("5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1"); // Bauern-Test (Matt in 6)

      Console.WriteLine();
      foreach (var move in b.GetMoves())
      {
        PrintMarkedBoard(b, new[] { (int)move.fromPos, move.toPos });
        if (move.promoPiece != Piece.None) Console.WriteLine("    promotion: " + move.promoPiece);
        Console.WriteLine();
        Console.WriteLine();
      }

      Console.WriteLine();
      Console.WriteLine("    " + b.GetFEN());
      Console.WriteLine();
    }
  }
}
