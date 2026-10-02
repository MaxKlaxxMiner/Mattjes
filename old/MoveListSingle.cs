using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
// ReSharper disable UnusedMember.Global
// ReSharper disable MemberCanBePrivate.Global
// ReSharper disable UnusedType.Global

namespace Mattjes
{
  /// <summary>
  /// bildet eine Zugliste ab
  /// </summary>
  public unsafe class MoveListSingle : IList<Move>
  {
    /// <summary>
    /// merkt sich den Zeiger auf die Basis-Daten (0 = erster Zug, -1 = Anzahl der Züge)
    /// </summary>
    public readonly byte* data;

    /// <summary>
    /// gibt die Anzahl der Elemente zurück
    /// </summary>
    public int Count { get; set; }

    /// <summary>
    /// Konstruktor
    /// </summary>
    /// <param name="data">Zeiger auf die Daten</param>
    public MoveListSingle(byte* data)
    {
      this.data = data;
    }

    /// <summary>
    /// Konstruktor
    /// </summary>
    /// <param name="data">Zeiger auf die Daten</param>
    /// <param name="moves">Züge, welche initial hinzugefügt werden sollen</param>
    public MoveListSingle(byte* data, IEnumerable<Move> moves)
      : this(data)
    {
      int count = 0;
      foreach (var move in moves)
      {
        SetMove(count++, move);
      }
      Count = count;
    }

    /// <summary>
    /// fragt einen bestimmten Zug ab
    /// </summary>
    /// <param name="index">Index auf den Zug, welcher abgefragt werden soll</param>
    /// <returns>abgefragter Zug</returns>
    public Move GetMove(int index)
    {
      Debug.Assert(index >= 0 && index < Count);
      return *(Move*)&data[index * 4];
    }

    /// <summary>
    /// setzt einen bestimmten Zug
    /// </summary>
    /// <param name="index">Index auf den Zug, welcher geschrieben werden soll</param>
    /// <param name="move">Zug, welcher geschrieben werden soll</param>
    public void SetMove(int index, Move move)
    {
      Debug.Assert(index >= 0 && index < Count || Count == 0);
      *(Move*)&data[index * 4] = move;
    }

    /// <summary>
    /// Indexer zum Abfragen oder Setzen der Züge
    /// </summary>
    /// <param name="index">Index in der Liste</param>
    /// <returns>Zug</returns>
    public Move this[int index]
    {
      get { return GetMove(index); }
      set { SetMove(index, value); }
    }

    /// <summary>
    /// gibt den Enumerator der Zugliste zurück
    /// </summary>
    /// <returns>Zugliste als Enumerator</returns>
    public IEnumerator<Move> GetEnumerator()
    {
      int count = Count;
      for (int i = 0; i < count; i++)
      {
        yield return GetMove(i);
      }
    }

    /// <summary>
    /// gibt den Enumerator der Zugliste zurück
    /// </summary>
    /// <returns>Zugliste als Enumerator</returns>
    IEnumerator IEnumerable.GetEnumerator()
    {
      return GetEnumerator();
    }

    /// <summary>
    /// fügt einen Zug in der Liste hinzu
    /// </summary>
    /// <param name="item">Zug, welcher hinzugefügt werden soll</param>
    public void Add(Move item)
    {
      int count = Count;
      Count = count + 1;
      SetMove(count, item);
    }

    /// <summary>
    /// leert die Zugliste
    /// </summary>
    public void Clear()
    {
      Count = 0;
    }

    /// <summary>
    /// prüft, ob ein bestimmter Zug in der Liste enhalten ist
    /// </summary>
    /// <param name="item">Zug, welcher geprüft werden soll</param>
    /// <returns>true, wenn der Zug enthalten ist</returns>
    public bool Contains(Move item)
    {
      return IndexOf(item) >= 0;
    }

    /// <summary>
    /// durchsucht die gesamte Liste nach einem Zug und gibt deren Position zurück (oder -1 wenn nicht gefunden)
    /// </summary>
    /// <param name="item">Zug, welcher gesucht werden soll</param>
    /// <returns>gefundene Position oder -1, wenn nicht gefunden</returns>
    public int IndexOf(Move item)
    {
      int count = Count;
      for (int i = 0; i < count; i++)
      {
        var move = GetMove(i);
        if (move.fromPos == item.fromPos && move.toPos == item.toPos && move.capturePiece == item.capturePiece && move.promoPiece == item.promoPiece)
        {
          return i;
        }
      }
      return -1;
    }

    /// <summary>
    /// kopierte die gesamte Liste in ein Array
    /// </summary>
    /// <param name="array">Array, wohin die Liste kopiert werden soll</param>
    /// <param name="arrayIndex">Startposition innerhalb des Arrays</param>
    public void CopyTo(Move[] array, int arrayIndex)
    {
      if (array != null && array.Rank != 1) throw new ArgumentNullException("array");
      int count = Count;
      if ((uint)arrayIndex > array.Length || arrayIndex + count > array.Length) throw new IndexOutOfRangeException("arrayIndex");
      for (int i = 0; i < count; i++)
      {
        array[arrayIndex + i] = GetMove(i);
      }
    }

    /// <summary>
    /// gibt die Liste als Array zurück
    /// </summary>
    /// <returns>fertiges Array</returns>
    public Move[] ToArray()
    {
      var result = new Move[Count];
      fixed (Move* resultPtr = result)
      {
        Tools.CopyBytes(data, (byte*)resultPtr, result.Length * sizeof(Move));
      }
      return result;
    }

    /// <summary>
    /// gibt den aktuell benötigten Platzbedarf in Bytes zurück
    /// </summary>
    public uint ByteLength
    {
      get
      {
        return (uint)(Count * 4 + 1);
      }
    }

    public bool Remove(Move item) { throw new NotSupportedException(); }
    public void RemoveAt(int index) { throw new NotSupportedException(); }
    public void Insert(int index, Move item) { throw new NotSupportedException(); }
    public bool IsReadOnly { get { return false; } }
  }
}
