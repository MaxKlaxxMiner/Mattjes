using System.Diagnostics;

namespace Mattjes
{
  /// <summary>
  /// Struktur, welche einen Zug speichert
  /// </summary>
  public struct Move
  {
    /// <summary>
    /// merkt sich die zu promovierende Figur, sofern ein Bauer das Ziel erreicht und umgewandelt wird (default: <see cref="Piece.None"/>)
    /// </summary>
    public Piece promoPiece;
    /// <summary>
    /// merkt sich die geschlagene Figur <see cref="Piece.None"/> = es wurde keine Figur geschlagen
    /// </summary>
    public readonly Piece capturePiece;
    /// <summary>
    /// merkt sich die Startposition der Figur (0-63)
    /// </summary>
    public readonly byte fromPos;
    /// <summary>
    /// merkt sich die Endposition der Figur (0-63)
    /// </summary>
    public readonly byte toPos;

    /// <summary>
    /// Konstruktor
    /// </summary>
    /// <param name="fromPos">die Startposition der Figur (0-63)</param>
    /// <param name="toPos">die Endposition der Figur (0-63)</param>
    /// <param name="capturePiece">die zu schlagende Figur <see cref="Piece.None"/> = es wurde keine Figur geschlagen</param>
    /// <param name="promoPiece">die promovierende Figur, sofern ein Bauer das Ziel erreicht und umgewandelt wird (default: <see cref="Piece.None"/>)</param>
    public Move(int fromPos, int toPos, Piece capturePiece, Piece promoPiece)
    {
      Debug.Assert(fromPos >= 0 && fromPos <= 63);
      Debug.Assert(toPos >= 0 && toPos <= 63);
      Debug.Assert((capturePiece & Piece.King) == Piece.None); // Könige dürfen nicht geschlagen werden
      Debug.Assert(promoPiece == Piece.None || (promoPiece & Piece.Colors) != Piece.None); // Farbe des promoviernden Bauern vorhanden?

      this.fromPos = (byte)(uint)fromPos;
      this.toPos = (byte)(uint)toPos;
      this.capturePiece = capturePiece;
      this.promoPiece = promoPiece;
    }

    /// <summary>
    /// gibt den Inhalt als lesbare Zeichenkette zurück
    /// </summary>
    /// <returns>lesbare Zeichenkette</returns>
    public override string ToString()
    {
      return BoardTools.PosChars(fromPos) + "-" + BoardTools.PosChars(toPos) + (promoPiece != Piece.None ? BoardTools.PieceChar(promoPiece).ToString() : "");
    }

    /// <summary>
    /// sortiert die Züge in einem Array
    /// </summary>
    /// <param name="moves">Züge, welche sortiert werden sollen</param>
    /// <param name="ofs">Startposition innerhalb des Arrays</param>
    /// <param name="count">Anzahl der enthaltenen Züge</param>
    public static unsafe void Sort(Move[] moves, int ofs, int count)
    {
      if (count < 2) return;
      fixed (Move* ptr = &moves[ofs])
      {
        Sort((uint*)ptr, count);
      }
    }

    /// <summary>
    /// sortiert uint-Werte in einer Liste
    /// </summary>
    /// <param name="ptr">Pointer auf die Liste</param>
    /// <param name="count">Anzahl der Elemente</param>
    static unsafe void Sort(uint* ptr, int count)
    {
      for (int start = 1; start < count; start++)
      {
        int i = start;
        uint tmp = ptr[start];
        for (; i > 0 && tmp < ptr[i - 1]; i--)
        {
          ptr[i] = ptr[i - 1];
        }
        ptr[i] = tmp;
      }
    }

    /// <summary>
    /// sortiert die Züge in einem Array rückwärts
    /// </summary>
    /// <param name="moves">Züge, welche sortiert werden sollen</param>
    /// <param name="ofs">Startposition innerhalb des Arrays</param>
    /// <param name="count">Anzahl der enthaltenen Züge</param>
    public static unsafe void SortBackward(Move[] moves, int ofs, int count)
    {
      if (count < 2) return;
      fixed (Move* ptr = &moves[ofs])
      {
        SortBackward((uint*)ptr, count);
      }
    }

    /// <summary>
    /// sortiert uint-Werte in einer Liste rückwärts
    /// </summary>
    /// <param name="ptr">Pointer auf die Liste</param>
    /// <param name="count">Anzahl der Elemente</param>
    static unsafe void SortBackward(uint* ptr, int count)
    {
      for (int start = 1; start < count; start++)
      {
        int i = start;
        uint tmp = ptr[start];
        for (; i > 0 && tmp > ptr[i - 1]; i--)
        {
          ptr[i] = ptr[i - 1];
        }
        ptr[i] = tmp;
      }
    }
  }
}
