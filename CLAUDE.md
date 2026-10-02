# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projekt

**Mattjes** ist eine Schachengine, die sich auf **Mattsuche und Remissuche** spezialisiert (nicht auf allgemeine Spielstärke). Lizenz: GPLv3. Ideen aus anderen GPL-Engines (Stockfish, Reckless u. a.) dürfen übernommen werden, aber bewusst: für reine Mattsuche sind andere Ansätze als klassisches Alpha/Beta oft besser (Proof-Number-Search / df-pn, listen-/task-basierte Suche statt Rekursion, Transposition-Tables mit längeren, kollisionsarmen Keys, Retrograde-Analyse).

Neustart nach ca. 5 Jahren Pause (Oktober 2026). Der Autor (20+ Jahre Entwickler, C# → Go, Rust-Anfänger) will **mit dem Projekt lernen**: Engine-Konzepte erklären, nicht nur Code liefern. Kommunikation auf Deutsch, Code-Bezeichner und Kommentare auf Englisch.

## Entscheidungen

- **Zwei Sprachen parallel:** `mattjesGo/` (Go, Heimat des Autors) und `mattjesRs/` (Rust, Lernziel). Algorithmen werden zuerst in Go gebaut, dann nach Rust portiert und gegeneinander verifiziert (Perft-Zahlen, Benchmarks). IDEs: GoLand und RustRover (JetBrains).
- **Kein UCI, keine Unit-Tests am Anfang.** Alles sind Konsolenprogramme; Tests werden in `main` ein- und auskommentiert (wie im alten C#-Code). UCI kommt erst in Milestone 6.
- **Ein Package/Modul pro Ansatz.** Jeder Generator-, Hash- oder Such-Ansatz bekommt sein eigenes Package, damit er später leicht komplett verworfen werden kann.
- **Performance nur mit Zahlen.** Jede Optimierungsbehauptung wird per Benchmark belegt (Nodes/s, Speicherverbrauch), so wie in der alten Commit-Historie.
- **Korrektheit und Benchmark trennen.** Korrektheitsläufe (Perft-Zahlen stimmen) kann Claude jederzeit selbst starten. Benchmarks dagegen nur auf Ansage des Autors, weil auf der Maschine oft andere Engines (z. B. Stockfish) nebenher laufen und die Werte verzerren. Zwischenmessungen immer mit diesem Vorbehalt kennzeichnen.
- **Mikro-Änderungen per A/B-Wechsellauf messen.** Beide Varianten als getrennte Binaries bauen (ins Scratchpad kopieren), dann abwechselnd A B A B A B laufen lassen und nur die Differenz innerhalb eines Laufs werten. Absolute Mn/s schwanken zwischen Sitzungen um einige Prozent (thermisch), Tabellen aus verschiedenen Sitzungen sind nicht direkt vergleichbar. Code-Layout-Effekte können auch "weniger Code" langsamer machen, daher nie ohne Messung annehmen.
- **Alle Dev-Tools laufen unter MSYS2/ucrt64**, ohne Abhängigkeit zum nativen Windows. Go 1.26 (cgo via ucrt64-gcc möglich), Rust 1.95 aus pacman (`mingw-w64-ucrt-x86_64-rust`, Target `x86_64-pc-windows-gnu`, kein rustup, kein MSVC). JetBrains-IDEs (GoLand, RustRover) sind optional und bringen ggf. eigene Toolchains mit. C# wird nicht verwendet.

## Milestones

1. **Movegen** ✅ (Oktober 2026): Mailbox nach yacboard-Vorbild in Go und Rust, dann Magic-Bitboards mit Pin/Check-Masken (Faktor 7 bis 9 schneller, Mailbox danach entfernt). Rekursiv, iterativ (copy-make), Breitensuche mit kompakten Stellungsströmen, parallel. Ergebnis: Rust 1,3 bis 1,5-mal schneller als Go bei identischem Code.
2. **Hash-Keys** ✅: Zobrist 64/128 Bit inkrementell, CRC64 und 32-Byte-Vollkey verglichen. Entscheidung: 128-Bit-Zobrist fest. CRC64-Low-Bits als Index unbrauchbar. Kanonisches EP nötig (OEIS-Treffer).
3. **Transposition Table:** mehrere Ansätze, Fokus auf Keygröße und Persistenz.
4. **Matt-/Remissuche:** ggf. zweistufig, erst schnelle Vorab-Suche nach vermeintlich sicheren Matts/Remis, dann gezielt mit optimaler Zugtiefe.
5. **Endgame-Tables** (Syzygy): eher kompletter Nachbau als Lib, um zu den eigenen Generatoren zu passen und gegen fertige Libs antreten zu können.
6. **UCI**, damit die Mattsuche in Arena & Co. läuft.
7. Optional: **neuronale Netze** zum schnelleren Finden von Gewinnstellungen.

## Build / Run

Immer **bauen, dann die Binary im Repo-Root starten**. Kein `go run`, kein `cargo run`.

```
./build.sh        # beide Binaries -> runMattjesGo.exe, runMattjesRs.exe
./build.sh go     # nur Go
./build.sh rs     # nur Rust (Release-Profil)
./runMattjesGo.exe
./runMattjesRs.exe
```

Welcher Test läuft, wird in `main` ein- und auskommentiert. Benchmarks nur mit diesen Release-Binaries messen.

## Git

Claude committet nicht selbst. Bei sinnvollen Abständen einen **Commit-Vorschlag als kurzen Einzeiler** machen (Format wie die bisherige Historie, z. B. `mattjes: faster search for mate positions`). Längere Erklärungen gehören in Markdown-Docs im Repo, nicht in die Commit-Message. Der Autor committet per TortoiseGit und bestätigt explizit ("commit ist drin").

## Code-Struktur (mattjesGo)

- `chess/` – generator-unabhängiges Vokabular: `Piece`, `Pos`, `Move`/`MoveBuffer`, `Castling`-Flags, `Setup` (validiertes FEN-Parsing, einzige Stelle, die Eingaben misstraut) und Perft-Referenzstellungen. Jedes Brett konvertiert per `FromSetup`/`Setup()`.
- `bitboard/` – **der** Generator (Milestone 1+2): `Pieces[2][6]` + `ByColor[2]` + `Squares[64]` + `Key`, 200 Byte, Bit i = `Pos(i)` (a8 = Bit 0, Nord ist `>> 8`!). Magic Bitboards, beim Start gesucht (ca. 230 ms, `InitDuration`). Legale Generierung über `danger`/`checkers`/`pinned` ohne Make/Unmake, nur En passant mit explizitem Test. `zobrist.go`: inkrementeller **128-Bit-Zobrist-Key** (`Board.Key`, fest, über `put`/`remove` und `xorState`; jeder Weg, der ein Brett erzeugt, muss `finishKey` aufrufen). `encode.go`: `PackedCodec` (Belegung + Nibbles, 19 bis 30 Byte, variabel) und `PackedFixedCodec` (32 Byte fix, für Index-Zugriff). `perft.go`: `PerftRecursive`, `PerftIterative` (copy-make), `PerftBreadth`, `PerftBreadthEncoded`, `PerftParallel`, `PerftDivide`.
- Stellungsidentität = Figuren + Seite + Rochade + EP. EP wird **kanonisch** nur bei legalem Schlag gesetzt (`hasLegalEnPassant`), Referenz für eindeutige Stellungen OEIS A083276 (9.417.681 nach 6 Halbzügen).
- `perft/` – Runner: prüft eine `Func(fen, depth)` gegen die Referenzdaten und druckt Zeit, Mn/s und Allokationen.
- `tests_perft.go`, `tests_hash.go` + `main.go` – die ein-/auskommentierbaren Testläufe. Regressionstests: `bitboardPerftRecursive`, `bitboardEncodeRoundtrip`, `bitboardHashVerify`, `bitboardUniquePositions` (OEIS-Werte).
- `docs/` – Messwerte und Erkenntnisse pro Milestone. Neue Benchmarks dort eintragen, nicht in Commit-Messages.
- **Go-Falle:** Pointer, die an Funktionswerte oder Interface-Methoden übergeben werden, entkommen auf den Heap (Escape-Analyse). In heißen Pfaden Bretter per Wert übergeben oder direkt aufrufen; Allokationen im Perft-Runner (`alloc`) verraten das sofort.

**Entfernt (nur noch in Git-Historie und `docs/`):** das Mailbox-Package (Faktor 7 bis 9 langsamer), der FastFen-Codec, CRC64/ExactKey-Vergleiche und die Key-Breiten-Schalter. Wer die Messungen nachvollziehen will: `docs/m1-mailbox-perft.md`, `docs/m1-bitboard-perft.md`, `docs/m2-hash-keys.md`.

## Code-Struktur (mattjesRs)

Spiegelt mattjesGo eins zu eins: `src/chess/`, `src/bitboard/`, `src/perft/`, `src/tests_perft.rs`, `src/tests_hash.rs`, `src/main.rs`. Gleiche Modulnamen, gleiche Funktionsnamen in snake_case (`gen_moves`, `do_move`, `perft_recursive`). Wer eine Änderung in einer Sprache macht, zieht sie in der anderen nach, damit die Gegenprüfung erhalten bleibt. Besonderheiten: `Piece`/`Pos` sind Newtypes mit Operator-Impls, alle Tabellen inklusive Zobrist liegen in einem `LazyLock<Tables>` (Magic-Suche ist zu langsam für const-eval), heiße Funktionen holen sich `&TABLES` einmal pro Aufruf und reichen `t: &Tables` an `put`/`remove` weiter. Codecs sind ein Trait mit Typparameter (monomorphisiert, kein Escape-Problem wie in Go). `perft/alloc_stats.rs` ersetzt den globalen Allokator zum Zählen der Allokationen. `#![allow(dead_code)]` im Crate-Root ist Absicht, weil immer nur ein Experiment aktiv ist. Clippy soll warnungsfrei bleiben (`cargo clippy --release`).

## Referenzcode

### `old/` – C#-Vorgänger (nur Referenz, nicht baubar, wird nicht weiterentwickelt)

Die Mattsuche darin war teils noch buggy. Nützlich sind Teststellungen und Ideen:

- `old/Program.cs`, `Main`: auskommentierte FEN-Teststellungen für Matt in N (Q+Q=3, Q+R=5, R+R=7, Q vs N=12, R=15, B+B=17, B+N=31, Q vs B+N=39, Bauern-Test=6) sowie Rochade-Tests.
- `old/Program.cs`: Suchvarianten Alpha/Beta "short" (ein Zug pro Knoten), "dynamic" (iterative Vertiefung), MoveCache-Varianten mit Hashtable auf CRC64-Keys.
- `old/Tests/Tests.MateReverse.cs`: **Retrograde-Idee**, alle Mattstellungen für eine Figurenkombination aufzählen (Königspaare iterieren, Figuren platzieren, `IsMate` prüfen, per CRC64 deduplizieren). Für Milestone 4/5 wieder aufgreifen.
- `old/IBoard.cs` und `BoardReference` → `BoardIndexed` → `BoardKingOptimized`/`2`/`3`: mehrstufig gebenchmarkte Board-Implementierungen.
- `old/Bitboards/`: unfertiger Stockfish-Port, nicht funktionsfähig.

### `E:\prog\spiele\chess\huschiBoard\yac\yacboard` – Go-Movegenerator des Autors (externes Repo)

Vom Autor komplett selbst entworfen (erst C#, dann Go), bewusst **ohne Bitboards** (Mailbox `[64]piece.Piece`), naiv begonnen und dann optimiert. Hält in Go mit Bitboard-Generatoren anderer Engines mit und ist oft schneller. Mit vielen komplexen Perft-Stellungen verifiziert, gilt als sehr zuverlässig. Vorlage für Milestone 1:

- `GetMoves()` / `GetMovesFast(*[256]Move)` (allokationsfrei), `DoMove` / `DoMoveBackward(m, BoardInfo)` mit `BoardInfo` als `uint32`-Snapshot (EP, Rochade, Halfmove-Clock).
- `IsChecked()`, `HasMoves()` (Early-Exit ohne Movelist, `IsMate = !HasMoves && IsChecked`).
- `SetFEN/GetFEN`, `Move.Uci()`, `Move.San(board)`, `perftTest.go` mit Referenzwerten von chessprogramming.org.
- Bekannter Nachteil: `Checksum()` ist eine CRC64/FNV-artige Vollberechnung, **nicht inkrementell**. Das ist das Thema von Milestone 2.

Piece-Encoding (yacboard und `old/` identisch, in Mattjes beibehalten): `White=0x40, Black=0x80`, `King=0x01, Queen=0x02, Rook=0x04, Bishop=0x08, Knight=0x10, Pawn=0x20`. Pos ist `x + y*8`, Rang 8 oben (`a8 = 0`).

## Dateien schreiben

Dateien **nur** mit `Write`/`Edit` anlegen oder ändern, nie per Bash-Heredoc/`echo`/`sed -i` (Git-Bash auf diesem Rechner halbiert Backslashes). Bash nur zum Lesen, Suchen, Bauen, Ausführen.
