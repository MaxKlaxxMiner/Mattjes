namespace Mattjes
{
  /// <summary>
  /// Klasse mit Hilfsmethoden
  /// </summary>
  public static class Tools
  {
    /// <summary>
    /// kopiert Bytes von einer Speicheradresse auf eine andere Speicheradresse
    /// </summary>
    /// <param name="srcPtr">Adresse auf die Quelldaten</param>
    /// <param name="dstPtr">Adresse auf die Zieldaten</param>
    /// <param name="byteCount">Anzahl der Bytes, welche kopiert werden sollen</param>
    public static unsafe void CopyBytes(byte* srcPtr, byte* dstPtr, int byteCount)
    {
      // --- 64-Bit Modus (als longs kopieren) ---
      int end = byteCount >> 3;
      var pSrc = (long*)srcPtr;
      var pDst = (long*)dstPtr;
      for (int i = 0; i < end; i++)
      {
        pDst[i] = pSrc[i];
      }
      int pos = end << 3;

      // --- die restlichen Bytes kopieren ---
      for (; pos < byteCount; pos++)
      {
        dstPtr[pos] = srcPtr[pos];
      }
    }
  }
}
