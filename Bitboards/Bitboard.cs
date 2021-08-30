// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable UnusedMember.Global
// ReSharper disable UnusedType.Global

namespace Mattjes.Bitboards
{
  public static class Bitboard
  {
    public const ulong AllSquares = ~0UL;
    public const ulong DarkSquares = 0xAA55AA55AA55AA55UL;

    public const ulong FileA = 0x0101010101010101UL;
    public const ulong FileB = FileA << 1;
    public const ulong FileC = FileA << 2;
    public const ulong FileD = FileA << 3;
    public const ulong FileE = FileA << 4;
    public const ulong FileF = FileA << 5;
    public const ulong FileG = FileA << 6;
    public const ulong FileH = FileA << 7;

    public const ulong Rank1 = 0xFF;
    public const ulong Rank2 = Rank1 << (8 * 1);
    public const ulong Rank3 = Rank1 << (8 * 2);
    public const ulong Rank4 = Rank1 << (8 * 3);
    public const ulong Rank5 = Rank1 << (8 * 4);
    public const ulong Rank6 = Rank1 << (8 * 5);
    public const ulong Rank7 = Rank1 << (8 * 6);
    public const ulong Rank8 = Rank1 << (8 * 7);

    public const ulong QueenSide = FileA | FileB | FileC | FileD;
    public const ulong CenterFiles = FileC | FileD | FileE | FileF;
    public const ulong KingSide = FileE | FileF | FileG | FileH;
    public const ulong Center = (FileD | FileE) & (Rank4 | Rank5);

    public static readonly ulong[] KingFlank =
    {
      QueenSide ^ FileD, QueenSide, QueenSide,
      CenterFiles, CenterFiles,
      KingSide, KingSide, KingSide ^ FileE
    };
  }
}
