using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Mattjes
{
  class Program
  {
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

      for (int p = 0; p < 64; p++)
      {
        b.ScanMove(p, movePos =>
        {
          Console.WriteLine("    " + Board.PosChars(p) + "-" + Board.PosChars(movePos) + " - " + b.fields[p]);
        });
      }

      Console.WriteLine();
      Console.WriteLine(b);
      Console.WriteLine();
      Console.WriteLine("    " + b.GetFEN());
      Console.WriteLine();
    }
  }
}
