# Milestone 1, Teil 2: Bitboard-Generator in Go und Rust

Stand: 2026-10-02, Go 1.26.3, Rust 1.95, Windows/amd64, 12 logische CPUs,
Maschine ohne Hintergrundlast. Alle Zahlen aus je einem Lauf, Vergleich zur
Mailbox aus `m1-mailbox-perft.md` (gleiche Sitzung, gleiche Suite).

## Design

Package `mattjesGo/bitboard` und Modul `mattjesRs/src/bitboard`, eins zu eins.

**Bit-Layout.** Bit i ist Feld `Pos(i)`, also a8 = Bit 0 und h1 = Bit 63, dieselbe
Nummerierung wie die Mailbox. Dadurch sind beide Bretter verlustfrei ineinander
umwandelbar und teilen `Move`, `Setup` und die UCI-Ausgabe. Preis: "Nord"
(Richtung Reihe 8) ist `>> 8`, in den meisten Engines ist es `<< 8`. Wer Code
aus anderen Engines übernimmt, muss die vertikale Spiegelung im Kopf behalten.

**Brett.** `Pieces[2][6]` (Farbe × Figurenart), `ByColor[2]` als Vereinigung,
dazu `Squares[64]` für die O(1)-Frage "was steht hier" (Schlagfigur im Zug).
184 Byte statt 74 bei der Mailbox. Der Kind-Index ist die Bitposition des
Typ-Flags in `chess.Piece` (King 0x01 → 0, Pawn 0x20 → 5), der Farb-Index ist
`p >> 7`. Beides ein einzelner Maschinenbefehl.

**Sliding-Angriffe: Magic Bitboards.** Für jedes Feld wird die relevante
Belegung (Blocker auf den Strahlen, Randfelder ausgenommen) mit einer "magischen"
Konstante multipliziert, die oberen Bits sind der Index in eine vorberechnete
Angriffstabelle. 107.648 Einträge (841 KB) für Türme und Läufer zusammen. Die
Magics werden beim Start per deterministischem xorshift64*-PRNG gesucht
(Carry-Rippler-Enumeration aller Belegungen, Epoch-Array statt Tabellen-Reset):
Go 241 ms in `init()`, Rust 227 ms in einem `LazyLock`. Später können die
gefundenen Konstanten als Quelltext ausgegeben und fest eingebaut werden.

**Legale Zuggenerierung ohne Make/Check/Unmake.** Pro Stellung werden drei
Bitmengen berechnet:

- `danger`: alle vom Gegner angegriffenen Felder, berechnet mit entferntem
  eigenem König, damit Gleiter "durch" den König hindurch wirken. Königszüge
  meiden diese Felder.
- `checkers`: gegnerische Figuren, die den König angreifen. Doppelschach: nur
  Königszüge. Einfachschach: alle anderen Figuren dürfen nur den Schachgeber
  schlagen oder die Linie dazwischen blockieren (`between`-Tabelle).
- `pinned`: eigene Figuren, die als einziger Blocker zwischen König und einem
  gegnerischen Gleiter stehen. Sie dürfen nur entlang dieser Linie ziehen
  (`line`-Tabelle). Gefesselte Springer ziehen nie.

Bauern werden farbparametrisiert als ganze Bitmenge verschoben (ein Shift für
alle Einzelschritte, einer für alle Doppelschritte, je einer pro
Schlagrichtung). Nur En passant braucht noch einen expliziten Test, weil zwei
Bauern gleichzeitig eine Reihe verlassen und einen Turmangriff aufdecken können.
Dafür wird die Belegung nach dem Schlag simuliert und der König auf Angreifer
geprüft. Das deckt auch den Fall ab, dass der doppelt gezogene Bauer selbst
der Schachgeber ist.

Folge in Rust: `gen_moves` braucht nur noch `&self`, die Mailbox brauchte
`&mut self`, weil sie Züge probeweise ausführt.

**Refactor nebenbei.** FEN-Parsing und Validierung, `Move`, `MoveBuffer` und
die Rochade-Flags liegen jetzt in `chess` (`chess.Setup` bzw. `Setup::parse`).
Beide Bretter konvertieren per `FromSetup`/`Setup()`. Es gibt genau eine Stelle,
die Eingaben misstraut.

## Korrektheit

Alle sieben Referenzstellungen stimmen in allen vier Varianten und beiden
Sprachen, geprüft bis 3,2 Mrd. Knoten (Startstellung Tiefe 7) und 3,0 Mrd.
(Position 3 Tiefe 8). Die Mailbox stimmt nach dem Refactor weiterhin.

## Messwerte Einzelthread (805 Mio. Knoten gesamt, Bulk-Counting am Blatt)

| Variante | Go Mailbox | Go Bitboard | Faktor | Rust Mailbox | Rust Bitboard | Faktor |
|---|---|---|---|---|---|---|
| rekursiv (Make/Unmake) | 27,0 Mn/s | 226,8 Mn/s | 8,4 | 41,3 Mn/s | 288,2 Mn/s | 7,0 |
| iterativ (Copy-Make) | 27,4 Mn/s | 254,2 Mn/s | 9,3 | 41,0 Mn/s | 310,1 Mn/s | 7,6 |
| Breitensuche | 26,1 Mn/s | 190,6 Mn/s | 7,3 | 39,4 Mn/s | 199,1 Mn/s | 5,1 |
| parallel, 12 Worker | 132,4 Mn/s | 903,9 Mn/s | 6,8 | 176,1 Mn/s | 1263,4 Mn/s | 7,2 |

Rust gegenüber Go beim Bitboard: 1,27 (rekursiv), 1,22 (iterativ), 1,04
(Breitensuche), 1,40 (parallel). Der Rust-Vorsprung ist kleiner als bei der
Mailbox (1,53), weil der Bitboard-Code aus wenigen, großen Operationen besteht,
die beide Compiler gut übersetzen, und weil Tabellenzugriffe und
Speicherbandbreite den Takt vorgeben.

Pro Stellung, rekursiv:

| Stellung | Tiefe | Knoten | Go Bitboard | Rust Bitboard |
|---|---|---|---|---|
| Start | 6 | 119.060.324 | 193,8 | 246,5 |
| Kiwipete | 5 | 193.690.690 | 298,0 | 384,3 |
| Pos 3 | 7 | 178.633.661 | 163,8 | 200,1 |
| Pos 4 | 5 | 15.833.292 | 239,9 | 306,1 |
| Pos 5 | 5 | 89.941.194 | 269,9 | 350,7 |
| Pos 6 | 5 | 164.075.551 | 281,9 | 371,6 |

Position 3 (Endspiel mit wenigen Figuren) ist die langsamste: Dort dominieren
die Fixkosten pro Stellung (Danger-Map, Checker, Pins), und es gibt wenig Züge,
über die sie sich verteilen. Kiwipete mit vielen Figuren und Zügen ist die
schnellste.

## Beobachtungen

1. **Faktor 7 bis 9 statt der erwarteten 2 bis 3.** Die Schätzung in
   `m1-mailbox-perft.md` bezog sich nur auf das Einsparen des
   Legalitätstests pro Zug. Dazu kommen hier: Bauern als Bitmenge statt einzeln,
   Gleiterangriffe als ein Tabellenzugriff statt Strahl-Schleife, und die
   Angriffsmenge pro Figur wird einmal berechnet statt pro Zielfeld geprüft.
2. **Copy-Make ist beim Bitboard schneller als Make/Unmake** (254 gegen 227
   Mn/s in Go, 310 gegen 288 in Rust). 184 Byte zu kopieren ist billiger als
   `UndoMove` mit seinen sechs Bitboard-Updates. Bei der Mailbox war es
   gleichauf. Das stärkt die listenbasierte Suche weiter.
3. **Breitensuche kostet jetzt sichtbar Zeit** (190 gegen 254 Mn/s), weil die
   Brettgröße von 74 auf 184 Byte gestiegen ist: 2,01 GB für Position 3 Tiefe 7
   statt 829 MB. Für Stellungslisten lohnt eine kompaktere Speicherform
   (z. B. nur `Pieces[2][6]` plus Flags = 104 Byte, `Squares` beim Laden
   rekonstruieren) oder gleich der Hash statt der Stellung.
4. **Parallel skaliert mit 4,0 bis 4,4** auf 12 logischen CPUs, etwas schlechter
   als die Mailbox (4,9), vermutlich weil die 841-KB-Magic-Tabelle von allen
   Kernen geteilt wird und der L2-Cache pro Kern knapp wird.
5. **Der Startaufwand von ca. 230 ms** für die Magic-Suche ist für
   Konsolenläufe egal, für eine UCI-Engine (Milestone 6) aber spürbar. Dann
   die Konstanten fest einbauen.

## Offene Punkte

- Magics als Konstanten generieren (ein Tool, das Go- und Rust-Quelltext ausgibt).
- Kompaktes Brettformat für Stellungslisten (Milestone 2/3 zusammen mit Hashing).
- `HasMoves()`/`IsMate()` mit Early-Exit; beim Bitboard fast gratis, weil
  `checkers != 0 && n == 0` schon in `GenMoves` sichtbar ist.
- Die Mailbox bleibt als Referenz und Lernstand im Repo, wird aber nicht weiter
  optimiert. Alle weiteren Milestones bauen auf dem Bitboard auf.
