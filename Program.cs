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
      b.SetFEN("8/8/8/8/3k4/8/N7/KB6 w - - 0 1 "); // Läufer + Springer Mattsuche

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
