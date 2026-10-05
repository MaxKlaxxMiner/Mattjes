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
