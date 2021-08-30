// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes.Bitboard
{
  public enum Direction
  {
    North = 8,
    East = 1,
    South = -North,
    West = -East,

    NorthEast = North + East,
    SouthEast = South + East,
    SouthWest = South + West,
    NorthWest = North + West
  }
}
