using System.Diagnostics;

namespace Mattjes
{
  /// <summary>
  /// Struktur, welche einen Zug speichert
  /// </summary>
  public struct Move
  {
    /// <summary>
    /// merkt sich die Startposition der Figur (0-63)
    /// </summary>
    public readonly byte fromPos;
    /// <summary>
    /// merkt sich die Endposition der Figur (0-63)
    /// </summary>
    public readonly byte toPos;
    /// <summary>
    /// merkt sich die geschlagene Figur <see cref="Piece.None"/> = es wurde keine Figur geschlagen
    /// </summary>
    public readonly Piece capturePiece;
    /// <summary>
    /// merkt sich die zu promovierende Figur, sofern ein Bauer das Ziel erreicht und umgewandelt wird (default: <see cref="Piece.None"/>)
    /// </summary>
    public Piece promoPiece;

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
      return IBoard.PosChars(fromPos) + "-" + IBoard.PosChars(toPos) + (promoPiece != Piece.None ? IBoard.PieceChar(promoPiece).ToString() : "");
    }
  }
}
