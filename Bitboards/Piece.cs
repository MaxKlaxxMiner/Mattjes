// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes.Bitboards
{
  public enum Piece
  {
    None,
    W_Pawn = PieceType.Pawn, W_Knight, W_Bishop, W_Rook, W_Queen, W_King,
    B_Pawn = PieceType.Pawn + 8, B_Knight, B_Bishop, B_Rook, B_Queen, B_King,
    NB = 16
  }
}
