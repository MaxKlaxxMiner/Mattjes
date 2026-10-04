# Messmaschinen

Mattjes wird auf zwei Rechnern entwickelt. Alle Tabellen in `m1-*.md` bis
`m3-*.md` stammen vom Arbeitsrechner. Zahlen aus verschiedenen Maschinen
(und auch aus verschiedenen Sitzungen derselben Maschine) sind nicht direkt
vergleichbar; jede neue Messreihe nennt deshalb ihre Maschine.

| | Arbeit | Zuhause |
|---|---|---|
| CPU | Intel Core i5, 6 Kerne / 12 Threads | Intel Core Ultra 9 285H |
| nutzbare Threads | 12 | **14** (Windows meldet 16, die 2 Low-Power-E-Cores bekommen bei Rechenlast nie etwas zugewiesen; Parallel-Tests immer mit 14 Workern aufrufen, nie mit 0 = NumCPU) |
| RAM | | 127 GB |
| Go | 1.26.3 | 1.26.3 |
| Rust | 1.95 | 1.96.0 (pacman, `x86_64-pc-windows-gnu`) |

## Zuhause, 2026-10-04: kompletter Regressions- und Perft-Lauf

Erster Lauf auf dem Heimrechner nach der Rust-Installation, nichts
Rechenaufwendiges im Hintergrund. Alle Zähler stimmen (Perft-Referenzen,
Zobrist inkrementell gegen Vollberechnung, OEIS A083276 auf Ebene 6,
Store-Roundtrip, Persistenz über Kreuz: Go schreibt, Rust liest und schreibt,
Go liest).

Perft-Suite (805 Mio. Knoten, Breitensuche ohne Pos 3 Tiefe 7 wegen 2-GB-Grenze):

| Variante | Go | Rust | Arbeit Go | Arbeit Rust |
|---|---|---|---|---|
| rekursiv (Make/Unmake) | 285,6 Mn/s | 407,6 Mn/s | 226,8 | 288,2 |
| iterativ (Copy-Make) | 349,3 Mn/s | 421,5 Mn/s | 254,2 | 310,1 |
| Breitensuche, Brett roh | 301,9 Mn/s | 310,3 Mn/s | 190,6 | 199,1 |
| Breitensuche, packed | 122,1 Mn/s | 203,5 Mn/s | 129,3 | 147,2 |
| Breitensuche, packed-fixed | 118,6 Mn/s | 194,1 Mn/s | 114,4 | 138,7 |
| parallel, 14 Worker (8,4 Mrd. Knoten) | 1838,7 Mn/s | 2498,3 Mn/s | 903,9 (12 W.) | 1263,4 (12 W.) |

Perft mit TT, 256 MB, 8,4 Mrd. Knoten:

| Variante | Go | Rust | Arbeit Go | Arbeit Rust |
|---|---|---|---|---|
| `Table` direkt, 24 Value-Bits | 6,95 s = 1212 Mn/s | 5,35 s = 1575 Mn/s | 11,90 s = 708 | 9,14 s = 922 |
| `Buckets`, 22 Value-Bits | 6,94 s = 1213 Mn/s | 5,20 s = 1620 Mn/s | 11,87 s = 710 | 9,03 s = 933 |

Trefferquote 52,5 %, Ersetzungen 68.346 (direkt) und 36 (Buckets), identisch
mit dem Arbeitsrechner, wie es sein muss.

Persistenz (Start Tiefe 7, 256 MB): kalter Lauf Go 2,25 s, Rust 1,61 s;
Speichern 181 / 159 ms; Laden 48 bis 98 ms; warmer Lauf 12 / 1 ms.

Eindeutige Stellungen Ebene 6 (20,2 Mio. Kinder, 9.417.681 eindeutig):

| Variante | Go | Rust | Arbeit Go | Arbeit Rust |
|---|---|---|---|---|
| sortieren + kompaktieren | 9,48 s | 3,36 s | 11,38 s | 4,76 s |
| `ttstore`-Set | 3,62 s | 3,26 s | 4,29 s | 3,91 s |

## Beobachtungen

- Einzelthread ist der Heimrechner im Perft 1,3- bis 1,5-mal schneller als der
  Arbeitsrechner, bei der Breitensuche mit rohen Brettern (speichergebunden)
  sogar 1,6-mal. Die packed-Varianten in Go legen dagegen nicht zu, die
  Nibble-Kodierung ist dort offenbar nicht speicher-, sondern rechengebunden
  und profitiert von der CPU nicht.
- Parallel skaliert mit 14 Workern auf das 6,4-fache (Go) bzw. 6,1-fache
  (Rust) des Einzelthreads, auf Arbeit waren es 4,0 bis 4,4 mit 12 Threads.
  Hybrid-Kerne: die P-Kerne tragen deutlich mehr als die E-Kerne, ein
  Root-Split mit statisch gleichen Anteilen wäre hier falsch, die
  Work-Stealing-Variante (atomarer Zugzähler) fängt das ab.
- Rust gegen Go bei identischem Code: 1,2- bis 1,7-mal, auf Arbeit 1,3 bis 1,5.
  Der größte Abstand liegt bei den packed-Streams (1,7), der kleinste bei der
  Breitensuche mit rohen Brettern (1,03), die in beiden Sprachen am
  Speicherdurchsatz hängt.
