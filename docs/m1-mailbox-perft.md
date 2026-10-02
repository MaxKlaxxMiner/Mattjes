# Milestone 1: Mailbox-Generator in Go, Perft-Ergebnisse

Stand: 2026-10-02, Go 1.26.3, Windows/amd64, 12 logische CPUs, Maschine ohne
Hintergrundlast. (Eine erste Messreihe lief versehentlich parallel zu einer
Stockfish-Engine; die yacboard-Baseline war dadurch um ca. 40 % zu niedrig und
führte zunächst zur falschen Schlussfolgerung, der neue Generator sei schneller.
Diese Datei enthält nur die bereinigten Werte.)

## Design

Package `mattjesGo/mailbox`, minimalistischer Nachbau von yacboard:

- 8x8-Mailbox `[64]Piece`, `a8 = 0`, Rang 8 oben, weiße Bauern laufen Richtung kleinerer Indizes.
- Ein farbparametrisierter Generator statt getrennter Weiß/Schwarz-Funktionen. Statt ausgerollter Randprüfungen werden beim Package-Init Tabellen vorberechnet: `edgeDist[sq][dir]` (Schritte bis zum Rand je Richtung), `knightTargets[sq]`, `kingTargets[sq]`, `castleClear[sq]` (Rochaderechte, die ein Zug von/nach `sq` löscht).
- Legalität wie in yacboard: pseudo-legaler Zug wird auf dem Brett ausgeführt, eigener König auf Angriff geprüft, Zug zurückgenommen (`isLegal`). Rochade wird separat geprüft (König nicht im Schach, Durchgangs- und Zielfeld nicht angegriffen, Zwischenfelder leer).
- `Move` ist 4 Byte (`From, To, Promo, Capture`). Rochade = Königszug um zwei Felder, En passant = diagonaler Bauernzug mit `Capture == None`.
- `State` (uint32) ist der Undo-Snapshot: EP-Feld, Rochaderechte, Halfmove-Clock.
- EP-Feld wird nur gesetzt, wenn ein gegnerischer Bauer es tatsächlich nutzen kann (yacboard-Verhalten). Das hält gleiche Stellungen gleich, was ab Milestone 2 beim Hashing zählt.
- `Board` ist 74 Byte groß und ein reiner Wert, kann also frei kopiert werden.

## Korrektheit

Alle sieben Referenzstellungen von chessprogramming.org stimmen in allen vier
Perft-Varianten, geprüft bis 706 Mio. Knoten (Position 4, Tiefe 6).

## Vier Perft-Varianten

| Variante | Prinzip | Speicher |
|---|---|---|
| `PerftRecursive` | klassisch rekursiv, Make/Unmake auf einem Brett | ~1 KB Movelist pro Rekursionsebene (Stack) |
| `PerftIterative` | listenbasiert, expliziter Stack, **Copy-Make** statt Undo | 1120 Byte pro Ply (Brett + Movelist + Zähler), Heap |
| `PerftBreadth` | Breitensuche, jede Ebene ist eine vollständige Stellungsliste | 74 Byte × Knoten der vorletzten Ebene |
| `PerftParallel` | Wurzelzüge auf Goroutinen verteilt, darunter rekursiv | wie rekursiv pro Worker |

## Messwerte Einzelthread (Bulk-Counting am Blatt, 805 Mio. Knoten gesamt)

| Stellung | Tiefe | Knoten | rekursiv | iterativ | breadth | alloc breadth |
|---|---|---|---|---|---|---|
| Start | 6 | 119.060.324 | 27,9 Mn/s | 28,2 Mn/s | 27,1 Mn/s | 358 MB |
| Kiwipete | 5 | 193.690.690 | 29,3 Mn/s | 29,6 Mn/s | 28,8 Mn/s | 295 MB |
| Pos 3 | 7 | 178.633.661 | 22,1 Mn/s | 22,4 Mn/s | 20,8 Mn/s | 829 MB |
| Pos 4 | 5 | 15.833.292 | 28,6 Mn/s | 29,3 Mn/s | 27,9 Mn/s | 30,5 MB |
| Pos 4 gespiegelt | 5 | 15.833.292 | 29,2 Mn/s | 29,7 Mn/s | 28,3 Mn/s | 30,5 MB |
| Pos 5 | 5 | 89.941.194 | 26,3 Mn/s | 26,7 Mn/s | 25,5 Mn/s | 153 MB |
| Pos 6 | 5 | 164.075.551 | 31,5 Mn/s | 32,3 Mn/s | 31,3 Mn/s | 281 MB |
| **gesamt** | | 805 Mio. | **27,0 Mn/s** | **27,4 Mn/s** | **26,1 Mn/s** | |

Rekursiv, iterativ und breadth liegen innerhalb der Messtoleranz gleichauf.

## Parallel (12 Worker, Root-Split, 2,22 Mrd. Knoten gesamt)

| Stellung | Tiefe | Knoten | Zeit | Mn/s |
|---|---|---|---|---|
| Start | 6 | 119.060.324 | 0,70 s | 169,6 |
| Kiwipete | 5 | 193.690.690 | 1,06 s | 182,1 |
| Pos 3 | 7 | 178.633.661 | 1,44 s | 123,8 |
| Pos 4 | 6 | 706.045.033 | 5,86 s | 120,5 |
| Pos 5 | 5 | 89.941.194 | 0,55 s | 164,1 |
| Pos 6 | 5 | 164.075.551 | 0,84 s | 194,5 |
| **gesamt** | | 2,22 Mrd. | 16,8 s | **132,4** |

Faktor 4,9 gegenüber Einzelthread. Bei 12 logischen CPUs (vermutlich 6 Kerne mit
SMT) ist das plausibel. Pos 3 und 4 skalieren schlechter, weil sie nur 14 bzw. 6
Wurzelzüge haben und die Arbeit ungleich verteilt ist. Ein Split auf Tiefe 2
würde das beheben.

## Vergleich mit yacboard (gleiche Maschine, gleiche Stellungen, rekursiv)

| Stellung | Tiefe | Knoten | yacboard | mailbox (neu) | Verhältnis |
|---|---|---|---|---|---|
| Start | 5 | 4.865.609 | 36,3 Mn/s | 27,1 Mn/s | 0,75 |
| Start | 6 | 119.060.324 | 38,2 Mn/s | 27,9 Mn/s | 0,73 |
| Kiwipete | 4 | 4.085.603 | 35,0 Mn/s | 28,9 Mn/s | 0,83 |
| Kiwipete | 5 | 193.690.690 | 36,6 Mn/s | 29,3 Mn/s | 0,80 |
| Pos 3 | 6 | 11.030.083 | 24,2 Mn/s | 20,1 Mn/s | 0,83 |
| Pos 3 | 7 | 178.633.661 | 26,7 Mn/s | 22,1 Mn/s | 0,83 |
| Pos 4 | 5 | 15.833.292 | 37,1 Mn/s | 28,6 Mn/s | 0,77 |
| Pos 5 | 5 | 89.941.194 | 31,9 Mn/s | 26,3 Mn/s | 0,82 |
| Pos 6 | 5 | 164.075.551 | 42,7 Mn/s | 31,5 Mn/s | 0,74 |
| **gesamt** | | 781 Mio. | **34,1 Mn/s** | **27,0 Mn/s** | **0,79** |

yacboard ist durchgehend etwa 20 bis 25 % schneller. Beide verwenden denselben
Legalitätstest, der Unterschied steckt also im Generator selbst: yacboards
ausgerollte Weiß/Schwarz-Varianten mit konstanten Offsets sparen die
Tabellenzugriffe (`edgeDist`, `dirDelta`, Slices für Springer/König) und die
Farbverzweigungen, die der kompakte Generator pro Zug ausführt. Der Preis ist
Codegröße: ca. 1.200 Zeilen gegenüber ca. 350. Der yacboard-Lauf liegt als
Wegwerf-Programm im Scratchpad und ist nicht Teil des Repos.

## Rust-Port (`mattjesRs/src/mailbox`)

Eins-zu-eins-Port desselben Algorithmus: gleiche Module, gleiche Tabellen,
gleiche Zugreihenfolge, gleiche Datengrößen (Board 74 Byte, Frame 1120 Byte).
Alle sieben Stellungen stimmen in allen vier Varianten. Rust 1.95, Release-Profil
mit `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`.

| Stellung | Tiefe | Knoten | Go rekursiv | Rust rekursiv | Rust iterativ | Rust breadth | Faktor Rust/Go |
|---|---|---|---|---|---|---|---|
| Start | 6 | 119.060.324 | 27,9 Mn/s | 40,6 Mn/s | 40,6 Mn/s | 39,0 Mn/s | 1,46 |
| Kiwipete | 5 | 193.690.690 | 29,3 Mn/s | 46,1 Mn/s | 45,1 Mn/s | 43,9 Mn/s | 1,57 |
| Pos 3 | 7 | 178.633.661 | 22,1 Mn/s | 32,1 Mn/s | 32,9 Mn/s | 31,2 Mn/s | 1,45 |
| Pos 4 | 5 | 15.833.292 | 28,6 Mn/s | 46,7 Mn/s | 44,9 Mn/s | 43,4 Mn/s | 1,63 |
| Pos 4 gespiegelt | 5 | 15.833.292 | 29,2 Mn/s | 44,7 Mn/s | 44,2 Mn/s | 43,4 Mn/s | 1,53 |
| Pos 5 | 5 | 89.941.194 | 26,3 Mn/s | 39,7 Mn/s | 38,7 Mn/s | 37,7 Mn/s | 1,51 |
| Pos 6 | 5 | 164.075.551 | 31,5 Mn/s | 53,3 Mn/s | 51,6 Mn/s | 49,8 Mn/s | 1,69 |
| **gesamt** | | 805 Mio. | **27,0 Mn/s** | **41,3 Mn/s** | **41,0 Mn/s** | **39,4 Mn/s** | **1,53** |

Parallel, 12 Worker: **176,1 Mn/s** (Go: 132,4 Mn/s), Faktor 4,3 gegenüber dem
Rust-Einzelthread. Die Allokationen sind in beiden Sprachen identisch (z. B. 829 MB
für Pos 3 Tiefe 7 in der Breitensuche), was bestätigt, dass dieselben Datenstrukturen
entstehen.

Rust ist bei identischem Algorithmus durchgehend um Faktor 1,45 bis 1,69 schneller
und schlägt damit auch yacboards ausgerollte Go-Version (34,1 Mn/s). Die
wahrscheinlichsten Gründe: LLVM mit Fat-LTO inlined `is_legal`, `is_attacked` und
die Tabellenzugriffe aggressiver als der Go-Compiler, Bounds-Checks werden häufiger
wegoptimiert, und es gibt keinen Write-Barrier- oder GC-Overhead (auch wenn Go hier
kaum allokiert). Der Rust-Code ist dabei nicht "unsafe" und enthält keine manuellen
Optimierungen.

Sprachliche Besonderheiten des Ports, die für Rust-Einsteiger interessant sind:

- `Piece` und `Pos` sind **Newtypes** (`struct Piece(u8)`), nicht Aliase. Dadurch
  lassen sich `|`, `&`, `+` und `Display` gezielt definieren, und ein `Piece` kann
  nicht versehentlich als Zahl verwendet werden. Go erlaubt das über benannte Typen
  ebenfalls, aber ohne Operator-Überladung.
- Die Tabellen entstehen per **`const fn`** zur Compile-Zeit und liegen als Daten im
  Binary. Go baut sie in `init()` beim Programmstart.
- `Board` ist `Copy`, Copy-Make ist also `let mut child = parent;`.
- `gen_moves` braucht `&mut self`, obwohl das Brett danach unverändert ist, weil
  `is_legal` den Zug kurz ausführt. Der Compiler erzwingt diese Ehrlichkeit.
- Im iterativen Perft dürfen Eltern- und Kind-Frame nicht gleichzeitig als `&mut`
  aus demselben `Vec` geholt werden; **`split_at_mut`** teilt den Vec in zwei
  disjunkte Hälften. Zwei Felder desselben Structs gleichzeitig mutabel zu
  borgen ist dagegen erlaubt (`f.board.gen_moves(&mut f.moves)`).
- `perft_parallel` nutzt **`thread::scope`**: Die Threads dürfen `root` und die
  Zugliste per Referenz borgen, weil der Scope garantiert, dass alle Threads vor
  dem Rücksprung beendet sind. Kein `Arc`, kein `clone`.
- Ein eigener **`#[global_allocator]`** zählt die Allokationen, weil Rust kein
  Äquivalent zu `runtime.MemStats` mitbringt.

## A/B-Test: Bauern-Randreihen-Check im Generator oder in SetFEN

Frage: Kostet ein `if y == 0 || y == 7 { continue }` pro Bauer im Generator
etwas, oder kann das Entfernen durch verändertes Code-Layout sogar schaden?
Methode: beide Varianten als getrennte Binaries, abwechselnd A B A B A B über
die volle 805-Mio.-Knoten-Suite, Maschine ohne Hintergrundlast.

| Runde | Go mit Check (A) | Go ohne (B) | Rust mit Check (A) | Rust ohne (B) |
|---|---|---|---|---|
| 1 | 25,2 | 25,4 | 39,6 | 40,1 |
| 2 | 25,3 | 25,4 | 40,0 | 40,3 |
| 3 | 25,3 | 25,4 | 39,9 | 40,1 |

Ohne Check ist in jeder Runde und in beiden Sprachen minimal schneller
(Go +0,5 %, Rust +0,8 %). Kein negativer Layout-Effekt. Die Prüfung liegt
jetzt in `SetFEN`, der Generator setzt die Invariante voraus. Das Muster gilt
allgemein: `SetFEN` ist der einzige Ort, der Eingaben misstraut; `DoMove`
erhält alle Invarianten automatisch.

Nebenbefund: Das absolute Niveau lag in diesem Lauf etwas unter dem der
Haupttabellen (25,4 statt 27,0 bzw. 40,1 statt 41,3 Mn/s) bei identischem
B-Code. Die Maschine schwankt also auch ohne Last um einige Prozent zwischen
Sitzungen, vermutlich thermisch. Vergleiche sind nur innerhalb eines
Wechsellaufs belastbar, nicht zwischen Tabellen aus verschiedenen Sitzungen.

## Erkenntnisse

1. **Copy-Make kostet nichts.** Ein 74-Byte-Brett zu kopieren ist so schnell wie
   Make/Unmake. Das ist die wichtigste Erkenntnis für die geplante listenbasierte
   Suche: Stellungen speichern und laden ist auf dieser Brettgröße praktisch gratis.
   Der Flaschenhals ist ausschließlich `GenMoves` samt Legalitätsprüfung.
2. **Breitensuche ist machbar, aber speicherhungrig.** Pos 3 Tiefe 7 braucht
   829 MB für 11 Mio. Stellungen der vorletzten Ebene. Startstellung Tiefe 7 würde
   119 Mio. × 74 Byte = 8,8 GB benötigen. Für die Mattsuche heißt das: Listen pro
   Ebene nur mit Pruning oder Hash-Deduplizierung (Milestone 2/3), nicht roh.
3. **Der Legalitätstest pro Zug ist der größte Posten.** Jeder pseudo-legale Zug
   ruft `IsAttacked` auf, das bis zu 8 Strahlen plus Springer- und Bauernfelder
   abklappert. Gefesselte Figuren und Schachgebote vorab zu bestimmen (Pin-Masken,
   Checker-Erkennung) ist der bekannte Weg, das zu umgehen, bringt typischerweise
   Faktor 2 bis 3. Das lohnt mehr als das Ausrollen des Generators.
4. **Kompakt kostet 20 bis 25 %.** Die Tabellenvariante ist ein guter Lern- und
   Referenzstand, aber yacboards ausgerollter Code bleibt die schnellere
   Mailbox-Implementierung. Ob sich das Ausrollen lohnt, entscheidet sich erst,
   wenn klar ist, ob die Mailbox überhaupt gegen Bitboards bestehen kann.
5. **Benchmarks nur auf leerer Maschine.** Die verfälschte erste yacboard-Messung
   hätte ohne Wiederholung eine falsche Design-Entscheidung gestützt.

6. **Rust ist bei gleichem Code 1,5-mal schneller als Go.** Das ist der erste harte
   Datenpunkt für die Sprachentscheidung. Er betrifft reinen Rechen-Code mit
   kleinen Arrays; wie sich beide bei großen Hashtabellen (Milestone 2/3) und
   Multithreading mit geteiltem Zustand verhalten, ist eine eigene Messung.

## Offene Punkte für spätere Schritte

- `HasMoves()` mit Early-Exit (für `IsMate`) fehlt noch, kommt mit Milestone 4.
- Bitboard-Generator als zweites Package in beiden Sprachen, dann Vergleich.
- Pin-Masken und Checker-Erkennung statt Make/Check/Unmake pro Zug.
