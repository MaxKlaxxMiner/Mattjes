# Milestone 2: Hash-Keys

Stand: 2026-10-02, Go 1.26.3, Rust 1.95, Windows/amd64, 12 logische CPUs,
Maschine ohne Hintergrundlast. Alle Experimente auf dem Bitboard-Generator.

## Was verglichen wird

| Key | Breite | Berechnung | Zweck |
|---|---|---|---|
| **Zobrist** | 64 oder 128 Bit | inkrementell in `DoMove`/`UndoMove` über die `put`/`remove`-Hooks und `xorState` | Standard für Transposition Tables |
| **CRC64** (FNV-1a) | 64 Bit | Vollberechnung über 64 Felder + Seite + Rochade + EP, wie yacboards `Checksum` | Vergleich zur alten Lösung |
| **Exakter Key** | 32 Byte | `PackedFixed`-Satz ohne Zugzähler | kollisionsfrei, Referenz für "wirklich verschieden" |

Die Key-Breite ist eine Compile-Zeit-Konstante: Go über Build-Tags
(`-tags keywords0` / `keywords1`, Default 2), Rust über Cargo-Features
(`--features keywords0` / `keywords1`, Default 2). Mit Breite 0 ist `Board.Key`
ein Array der Länge 0, alle Updates fallen weg.

Identität einer Stellung = Figuren + Seite am Zug + Rochaderechte + EP-Feld.
Halfmove-Clock und Zugnummer gehören nicht dazu.

## Korrektheit

- Inkrementeller Zobrist-Key gegen Vollberechnung an jedem Knoten (17 Mio.
  Knoten über alle Referenzstellungen, Hin- und Rückweg): **0 Abweichungen**, in
  beiden Sprachen.
- Roundtrip-Test der Kodierungen schließt jetzt den Key mit ein (ein Fehler im
  Decoder, der die Flag-Anteile des Keys vergaß, wurde dadurch gefunden, siehe unten).

## Kanonisches En passant

Beim Zählen der eindeutigen Stellungen ergab Ebene 6 zunächst 9.417.683 statt
der Referenz 9.417.681 ([OEIS A083276](https://oeis.org/A083276)). Ursache: Das
EP-Feld wurde gesetzt, sobald ein gegnerischer Bauer daneben stand, auch wenn der
Schlag wegen Fesselung illegal war. Zwei Stellungen auf Ebene 6 unterscheiden sich
nur darin. Jetzt setzt `DoMove` das EP-Feld nur, wenn mindestens ein **legaler**
EP-Schlag existiert (`hasLegalEnPassant`, nutzt den vorhandenen Belegungstest).
`FromSetup` wendet dieselbe Regel auf eingelesene FENs an. Danach: **9.417.681**,
exakt die Referenz, und damit auch ein unabhängiger Beweis, dass Generator und
Deduplizierung stimmen.

| Ebene | Kinder | eindeutig | Referenz |
|---|---|---|---|
| 1 | 20 | 20 | 20 |
| 2 | 400 | 400 | 400 |
| 3 | 8.902 | 5.362 | 5.362 |
| 4 | 118.522 | 72.078 | 72.078 |
| 5 | 1.797.389 | 822.518 | 822.518 |
| 6 | 20.197.183 | 9.417.681 | 9.417.681 |

"Kinder" sind die Züge aller eindeutigen Eltern der Vorebene, nicht perft(n):
schon ab Ebene 3 spart die Deduplizierung spürbar (20,2 Mio. statt 119 Mio. auf
Ebene 6). Das ist der Kern dessen, was eine Transposition Table später leistet.

Die Mailbox (eingefroren) behält die einfachere "Bauer daneben"-Regel; ihre FEN-
Ausgabe kann in seltenen Fesselungsfällen vom Bitboard abweichen, Perft-Zahlen nicht.

## Kollisionen (Startstellung, eindeutige Stellungen pro Ebene)

| Ebene | eindeutig | zob64 | zob128 | crc64 | zob32 | crc32 low | crc32 high | erwartet 32 Bit |
|---|---|---|---|---|---|---|---|---|
| 4 | 72.078 | 0 | 0 | 0 | 0 | 9 | 1 | 0,6 |
| 5 | 822.518 | 0 | 0 | 0 | 45 | 766 | 72 | 78,8 |
| 6 | 9.417.681 | 0 | 0 | 0 | 9.037 | 63.706 | 10.261 | 10.325 |

Erwartete Kollisionen nach Geburtstagsparadoxon: n(n-1)/2 / 2^bits. Für 64 Bit
bei 9,4 Mio. Stellungen 2,4·10⁻⁶, für 32 Bit 10.325.

- **Zobrist 64 und CRC64: null Kollisionen**, wie erwartet. Zobrist 128 bringt bei
  dieser Größenordnung nichts Messbares; es wird erst relevant, wenn eine Tabelle
  über Milliarden Stellungen hinweg *nie* falsch liegen darf (Mattbeweis).
- **Zobrist 32 Bit: 9.037** gegen 10.325 erwartet, also wie ein Zufallsgenerator.
- **CRC64 obere 32 Bit: 10.261**, ebenfalls zufällig. **CRC64 untere 32 Bit:
  63.706**, sechsmal mehr als Zufall. Die unteren Bits von FNV-1a hängen nur von
  den unteren Bits der Eingabe ab (Multiplikation trägt Information nur nach oben),
  und die Eingabebytes (Figurencodes) sind sehr ähnlich.

## Verteilung als Tabellenindex (9,4 Mio. Stellungen in 2²² Slots, Last 2,25)

| Key → Index | leere Slots | erwartet | max. Last | z-Score |
|---|---|---|---|---|
| Zobrist64 untere Bits | 446.045 | 444.137 | 12 | 2,4 |
| Zobrist64 obere Bits | 443.462 | 444.137 | 13 | -4,5 |
| **CRC64 untere Bits** | **2.053.167** | 444.137 | **63** | **17.033** |
| CRC64 obere Bits | 443.922 | 444.137 | 13 | -0,8 |
| mix64(CRC64) untere Bits | 444.123 | 444.137 | 13 | -0,3 |
| Belegungs-Bitboard untere Bits (naiv) | 4.186.552 | 444.137 | 85.963 | 18 Mio. |
| exakter Key, gefaltet | 444.593 | 444.137 | 14 | 0,2 |

z-Score = (χ² − df) / √(2·df); nahe 0 heißt nicht von Zufall unterscheidbar.

**CRC64 ist als Index unbrauchbar, wenn man die unteren Bits nimmt:** die Hälfte
der Tabelle bleibt leer, Ketten bis 63. Mit den oberen Bits oder nach einem
Finalizer (`Mix64`, splitmix64) ist es einwandfrei. Zobrist ist in allen Bits
gleichmäßig. Für Milestone 3 heißt das: Index aus Zobrist direkt, oder aus jedem
anderen Key nur nach Mixen.

## Kosten

### Inkrementelle Pflege (A/B/C-Wechsellauf, volle Perft-Suite rekursiv, 805 Mio. Knoten)

| Key-Breite | Go | Rust |
|---|---|---|
| 0 (aus) | 209,4 / 208,8 Mn/s | 301,0 / 302,1 Mn/s |
| 64 Bit | 198,5 / 197,2 Mn/s (−5,5 %) | 286,6 / 269,0 Mn/s (−8 %) |
| 128 Bit | 193,7 / 193,1 Mn/s (−7,6 %) | 280,3 / 279,1 Mn/s (−7 %) |

Inkrementelles Zobrist kostet 5 bis 8 % der reinen Zuggenerierung. Der Schritt
von 64 auf 128 Bit ist in Rust kostenlos (zwei XOR-Wörter werden vektorisiert),
in Go ca. 2 %.

### Vollberechnung pro Knoten (59,7 Mio. Knoten, gegen inkrementell lesen)

| pro Knoten berechnet | Go | relativ | Rust | relativ |
|---|---|---|---|---|
| inkrementellen Key lesen (Baseline) | 176,8 Mn/s | 100 % | 256,1 Mn/s | 100 % |
| CRC64 über 64 Felder | 128,8 Mn/s | 73 % | 165,4 Mn/s | 65 % |
| Zobrist Vollberechnung | 109,4 Mn/s | 62 % | 159,7 Mn/s | 62 % |
| exakter 32-Byte-Key + Faltung | 132,4 Mn/s | 75 % | 132,6 Mn/s | 52 % |

Die Vollberechnung kostet das Vier- bis Sechsfache der inkrementellen Pflege.
In Go ist der exakte 32-Byte-Key billiger als CRC64, weil er nur die besetzten
Felder anfasst (`popLSB`) statt alle 64; in Rust lag die erste Fassung wegen
einer Vec-Allokation pro Aufruf zurück (im Runner an `alloc 6.3 MB` erkennbar),
inzwischen auf ein Stack-Array umgestellt.

### Deduplizierung (Ebene 6, 20,2 Mio. Kinder sortieren und eindeutig zählen)

Go 15,0 s, Rust 7,0 s. Der Unterschied ist das Sortieren von 32-Byte-Schlüsseln:
Rusts `sort_unstable` auf `[u8; 32]` vergleicht inline, Go ruft pro Vergleich
`bytes.Compare` über einen Funktionswert auf. Alle Zählungen (eindeutig,
Kollisionen, Verteilung) sind in beiden Sprachen byteidentisch.

## Lektionen

1. **Decoder müssen den Key vervollständigen.** `DecodePacked` setzte die Figuren
   per `put` (Key wird mitgeführt), die Flags aber direkt, ohne Zobrist-Anteile.
   Ergebnis: 157.073 scheinbare Zobrist-"Kollisionen", alle zwischen Stellungen,
   die sich nur in Seite, Rochade oder EP unterschieden. Der Roundtrip-Test prüft
   jetzt den Key mit, und `readFlags` ruft `finishKey` auf.
2. **Ein Key ist nur so gut wie seine Definition.** Dass Zobrist-64 und -128
   identische Kollisionszahlen hatten, war das Signal: Zufall hätte 128 Bit auf
   null gebracht. Systematische Fehler sehen anders aus als zufällige.
3. **Kanonische Stellungen sind Voraussetzung fürs Hashing.** Erst die strikte
   EP-Regel macht gleiche Stellungen gleich. Die Übereinstimmung mit OEIS ist der
   Beleg.
4. **Yacboards CRC64 ist als Key brauchbar, als Index nicht**, es sei denn, man
   nimmt die oberen Bits oder mischt nach.

## Entscheidung für Milestone 3

- **Zobrist 64 Bit inkrementell** als Arbeitsschlüssel und Index-Quelle.
- **128 Bit** als Option behalten (Compile-Zeit-Schalter), um in der TT die
  Falsch-Positiv-Rate gegen 64 Bit zu messen. Praktisch misst man dort den
  Unterschied zwischen "sehr selten" und "nie".
- **Exakter 32-Byte-Key** als Verifikation in der TT: Wo Korrektheit zählt
  (Mattbeweis), wird der Eintrag mit dem vollen Key verglichen, nicht nur mit
  dem Hash. Kostet 32 statt 8 Byte pro Eintrag, dafür keine falschen Treffer.
