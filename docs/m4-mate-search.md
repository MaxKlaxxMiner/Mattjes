# Milestone 4: Matt- und Remissuche - Ergebnisse

Messwerte und Erkenntnisse zum Entwurf in `m4-mate-search-design.md`, in der
Reihenfolge des Umsetzungsplans. Maschine: Arbeitsrechner (`machines.md`),
sofern nicht anders angegeben.

## Schritt 1: Generator-Erweiterungen (2026-10-05)

Neu in `bitboard/mate.go` und `mate.rs`:

| Funktion | Was | Wie |
|---|---|---|
| `HasMoves` | gibt es einen legalen Zug? | `GenMoves`-Logik (danger, checkers, pins) mit Rückkehr beim ersten Treffer, keine Liste. Reihenfolge: König, Springer, Läufer/Dame, Turm/Dame, alle ungefesselten Bauern auf einmal per Masken, gefesselte Bauern einzeln, En passant. Rochade entfällt: ist sie legal, ist der einfache Königsschritt auf das überschrittene Feld es auch. |
| `IsMate`, `IsStalemate` | `InCheck && !HasMoves`, `!InCheck && !HasMoves` | |
| `checkersAfter(m)` | welche eigenen Steine greifen den gegnerischen König nach m an, ohne m zu spielen | Belegung nach dem Zug bauen (Start raus, Ziel rein, EP-Bauer raus, Rochadeturm versetzen), gezogenen Stein in die passende Figurenmenge legen (bei Umwandlung die neue Figur), dann Angriffe **vom Königsfeld aus** gegen diese Mengen schneiden. Liefert zusätzlich, ob der gezogene Stein selbst Schach gibt (direkt). |
| `GivesCheck(m)` | `checkersAfter(m) != 0` | |
| `GenChecks` | nur Schachgebote | `GenMoves` plus `GivesCheck`-Filter. Ein eigener Generator mit Zielmasken ist eine Optimierung, die gemessen werden muss, keine Voraussetzung. |
| `PerftDetailed` | Perft mit Klassifikation aller Blattzüge | Schlagzug, EP, Rochade, Umwandlung aus dem Zug; Schach, Abzug, Doppel aus `checkersAfter`; Matt nur für Schachgebote per `DoMove` + `HasMoves`. |

### Verifikation gegen chessprogramming.org

Die Perft-Tabellen dort haben für die Stellungen 1 bis 4 zusätzliche Spalten.
Alle 25 Zeilen bis 200 Mio. Knoten stimmen in **allen** Spalten, in Go und in
Rust (528,7 Mio. Blätter klassifiziert, Go 8,9 s, Rust 6,8 s):

| Stellung | Tiefen | Knoten | Schlagzüge | EP | Rochaden | Umwandlungen | Schachs | Abzug | Doppel | Matts |
|---|---|---|---|---|---|---|---|---|---|---|
| Grundstellung | 1 bis 6 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Kiwipete | 1 bis 5 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ (siehe unten) | ✓ |
| Stellung 3 | 1 bis 7 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Stellung 4 | 1 bis 5 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | leer | leer | ✓ |

Drei Dinge, die dabei herauskamen:

- **Definition von "Abzugschach".** Die Tabellen zählen als Discovery Check
  nur Schachs mit **genau einem** Schachgeber, der nicht der gezogene Stein ist.
  Ein Doppelschach (direkt plus Abzug, oder zwei Abzüge per En passant) zählt
  nur als Double Check. Mit "Abzug = mindestens ein fremder Schachgeber" lagen
  wir in jeder Zeile exakt um die Zahl der Doppelschachs daneben.
- **Kiwipete Tiefe 5, Doppelschachs.** Die Tabelle nennt 2637, wir zählen
  **2645**. Die Seite trägt dazu die Fußnote, dass es laut Talkchess 2645 sein
  könnten. Eine Nachrechnung, die jeden Doppelschach-Zug spielt und die
  Angreifer des Königs neu bestimmt, stimmt mit `checkersAfter` in allen 2645
  Fällen überein: 2638 normale Züge, 6 Umwandlungen, 1 En passant. Die
  Referenz in `chess/perftdata` steht deshalb auf 2645.
- **Leere Zellen.** Stellung 4 hat in den Tabellen keine Abzug- und
  Doppelschach-Werte (und bei Tiefe 4 keine Matts). Die Referenz markiert sie
  als `Unknown`, sie werden nicht verglichen. Eine erste automatische Abfrage
  der Seite hatte die Zeile für Tiefe 3 verschoben wiedergegeben (22 Abzüge,
  0 Matts statt 2 Abzüge, 22 Matts); die eigene Zählung hat das korrigiert.

### Verifikation gegeneinander

`bitboardMateVerify` läuft über alle Knoten der Referenzstellungen bis 5 Mio.
Blätter (17 Mio. Knoten) und prüft an jedem Knoten `HasMoves` gegen
`GenMoves() > 0`, `IsMate` und `IsStalemate` gegen `GenMoves` + `InCheck`, für
jeden Zug `GivesCheck` gegen `DoMove` + `InCheck`, und dass `GenChecks` genau
die Schachgebote in derselben Reihenfolge liefert: **0 Abweichungen**, Go und
Rust. Go und Rust liefern identische Zählerwerte in allen Spalten.

### Offen

- Geschwindigkeit von `HasMoves` gegen `GenMoves` an Blättern, die kein Matt
  sind (Messgröße aus dem Entwurf), und ob ein eigener `GenChecks`-Generator
  mit Zielmasken den Filter schlägt. Beides erst messen, wenn `mateab` steht
  und zeigt, wie oft die Funktionen wirklich aufgerufen werden.

## Schritt 2: `mateab` ohne Transposition Table (2026-10-05)

Package `mateab` (Arbeitstitel), in Go und Rust. Zwei Knotenfunktionen bilden
den AND/OR-Baum direkt ab:

- `attack` (OR, Angreifer am Zug, Resttiefe ungerade): Orakel fragen, bei
  Resttiefe 1 nur `GenChecks` und `HasMoves` am Kind, sonst alle Züge in der
  Reihenfolge Killer, Schachs, Schlagzüge, Rest. Der erste Zug, dessen
  `defend` ein Matt liefert, beendet den Knoten.
- `defend` (AND, Verteidiger am Zug, Resttiefe gerade): keine Züge und Schach
  ist "schon matt" (Distanz 0), keine Züge ohne Schach ist Patt, Orakel-Remis
  ist Widerlegung. Reihenfolge Killer, Schlagzüge, Rest. Der erste Zug, dessen
  `attack` kein Matt findet, beendet den Knoten; sonst ist die Distanz das
  längste Kind plus eins.
- **Killer** pro Ebene: der Zug, der zuletzt auf dieser Ebene gemattet oder
  widerlegt hat, wird zuerst probiert. Billig und bei iterativer Vertiefung
  sehr wirksam, weil die nächste Tiefe die alten Widerlegungen wiederfindet.
- **Hauptvariante** über eine Dreieckstabelle (`pv[ply][...]`), keine
  Allokation im Baum; copy-make mit einem Brett pro Rekursionsebene.
- **Orakel**: `Material` meldet Remis für tote Stellungen nach FIDE 5.2.2
  (K-K, K+Leichtfigur-K, K+L-K+L gleichfarbig). Zwei Springer gegen König sind
  bewusst nicht tot, weil mit Hilfe des Verteidigers Matt möglich ist.

Teststellungen bis Matt in 7, Iteration über 1, 3, 5 ... Halbzüge
(Arbeitsrechner):

| Stellung | Matt in | Halbzüge | Knoten gesamt | Go | Rust | Hauptvariante |
|---|---|---|---|---|---|---|
| KQQ-K | 3 | 5 | 1.966 | 0 ms | 0 ms | Qe7+ Kf5 Qg2 Kf4 Qg5# |
| KQR-K | 5 | 9 | 1.169.380 | 124 ms | 78 ms | Qe7+ Kd5 Ra5+ Kc6 Rc5+ Kb6 Qc7+ Ka6 Ra5# |
| KRR-K | 7 | 13 | 106.775.538 | 16,35 s | 9,15 s | Ra5+ Kd6 Rd2+ Kc7 Rc5+ Kb6 Rc8 Kb7 Rc3 Ka6 Rb2 Ka7 Ra3# |
| Bauern | 6 | 11 | 464.248 | 108 ms | 51 ms | g3xh4 Ke7 h8=Q Ke6 Qe8+ Kd6 f8=Q+ Kc7 Qef7+ Kb6 Qfd6# |

Alle vier Mattlängen exakt bei 2N-1 Halbzügen, eine Tiefe weniger findet
nichts. Der Bauerntest (Matt in 6) ist damit zum ersten Mal unabhängig
bestätigt, beide Sprachen finden dieselbe Variante. Go und Rust zählen
knotengenau gleich (108.411.132 Knoten über alle vier), Rust ist 1,8-mal
schneller als Go.

Die Verteilung bei KRR-K zeigt, wo die Zeit hingeht: Tiefe 11 ("es gibt kein
Matt in 6") kostet 81,7 Mio. Knoten, Tiefe 13 (das Matt in 7 finden) nur 22,3
Mio. **Widerlegen ist teurer als Beweisen**, weil an jedem Verteidigerknoten
nur ein Fluchtzug nötig ist, aber die Angreiferknoten darüber jeden ihrer Züge
widerlegt sehen müssen. Transpositionen sind in solchen Endspielen massiv
(dieselbe Stellung über verschiedene Zugfolgen), das ist der Ansatzpunkt für
Schritt 3.

Die Mattzüge der Hauptvarianten weichen von den Tablebase-Erstzügen ab (KQQ-K:
Qe7+ statt Qf7). Das ist korrekt: Es gibt mehrere Matts in 3, die Suche liefert
das erste in Zugreihenfolge, die Tablebase ein beliebiges optimales.
