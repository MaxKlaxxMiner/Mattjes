// ReSharper disable BuiltInTypeReferenceStyle
// ReSharper disable UnusedMember.Global
// ReSharper disable RedundantCast
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable UnusedType.Global
// ReSharper disable NotAccessedField.Global
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
    public byte fromPos;
    /// <summary>
    /// merkt sich die Endposition der Figur (0-63)
    /// </summary>
    public byte toPos;
    /// <summary>
    /// merkt sich die geschlagene Figur <see cref="Piece.None"/> = es wurde keine Figur geschlagen
    /// </summary>
    public Piece capturePiece;
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

      this.fromPos = (byte)(uint)fromPos;
      this.toPos = (byte)(uint)toPos;
      this.capturePiece = capturePiece;
      this.promoPiece = promoPiece;
    }
  }
}
