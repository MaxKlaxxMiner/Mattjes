using System;
// ReSharper disable UnusedType.Global
// ReSharper disable UnusedMember.Global

namespace Mattjes.Bitboards
{
  [Flags]
  public enum Castling : byte
  {
    NoCastling = 0,
    WhiteOO = 1,
    WhiteOOO = 2,
    BlackOO = 4,
    BlackOOO = 8,
    KingSide = WhiteOO | BlackOO,
    QueenSide = WhiteOOO | BlackOOO,
    WhiteCastling = WhiteOO | WhiteOOO,
    BlackCastling = BlackOO | BlackOOO,
    AnyCastling = WhiteCastling | BlackCastling
  }
}
