# Teststellungen

Bekannte Stellungen mit gesichertem Ergebnis, von leicht nach schwer sortiert. Die
Mattlänge zählt Züge der Seite am Zug (Matt in N = 2N − 1 Halbzüge). "Tabelle" heißt,
die Engine antwortet im UCI-Modus sofort aus den Endspieltabellen (bis vier Steine
immer, ab fünf nur mit Cache-Datei), "Suche" heißt, der gewählte Suchalgorithmus muss
ran. Der Beweisgraph ist die kleinste Stellungsmenge, die ein Beweis braucht (von
`matelist` gezählt), der Maßstab für jede Suche.

Die ersten zehn stehen in `chess/matedata.go` (`MatePositions`) und sind die
Regressionsstellungen der Suchen; die acht Endspiele bis fünf Steine wurden am
2026-10-05 gegen eine Tablebase geprüft (`docs/m4-mate-search-design.md`, Abschnitt 7).

| # | Name | Steine | FEN | Ergebnis | Antwort | Bestätigt durch | Beweisgraph | Hinweis |
|---|---|---|---|---|---|---|---|---|
| 1 | KQQ-KN | 5 | `7k/5n2/8/8/8/8/Q4Q2/K7 w - - 0 1` | Matt in 2 (`f2f6 h8g8 a2f7`) | Suche | `matepn` und `mateab`, von Hand nachprüfbar | | Tabellenstufe kennt nur das Remis `a2f7` (Patt), die Suche findet das Matt in 5 Knoten |
| 2 | KQQ-K | 4 | `8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1` | Matt in 3 | Tabelle | Tablebase, eigene Tabelle | | Rauchtest |
| 3 | KQR-K | 4 | `8/8/8/4k3/8/Q7/R7/K7 w - - 0 1` | Matt in 5 | Tabelle | Tablebase, eigene Tabelle | | |
| 4 | Bauern | 12 | `5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1` | Matt in 6 (`g3h4 f8g7 h7h8q ...`) | Suche | `matepn` (7.800 Knoten) und `mateab` (216.429) stimmen überein, keine externe Quelle | | Umwandlung, En passant, schwarze Dame; aus dem alten C#-Code |
| 5 | KRR-K | 4 | `8/8/8/4k3/8/R7/R7/K7 w - - 0 1` | Matt in 7 | Tabelle | Tablebase, eigene Tabelle | 907 | Treppenmatt, stille Züge |
| 6 | KRR-KN | 5 | `8/8/4k3/8/8/8/R7/K1R2n2 w - - 0 1` | Matt in 7 (`a2a6 e6d5 c1f1 ...`) | Tabelle (`5-KRRKN.bin`) | eigene Fünf-Steiner-Tabelle | | ohne Cache-Datei ein Suchfall |
| 7 | KQ-KN | 4 | `7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1` | Matt in 12 | Tabelle | Tablebase, eigene Tabelle | 2.687 | Zugzwang, Pattfallen; `matepn` 181.065 Besuche ohne Tabellen |
| 8 | KR-KR | 4 | `8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1` | Matt in 15 | Tabelle | Tablebase, eigene Tabelle | | Gegenspiel des Verteidigers; `matepn` 1,1 Mio. Besuche |
| 9 | KBB-K | 4 | `8/8/4k3/8/8/8/8/K2BB3 w - - 0 1` | Matt in 17 | Tabelle | Tablebase, eigene Tabelle | 9.825 | lange stille Manöver; `matepn` 109 Mio. Besuche mit 1 GB |
| 10 | KP-KP | 4 | `8/7k/1p6/1P6/7K/8/8/8 w - - 0 1` | Matt in 25 (nur `1.Kh5`), Schwarz am Zug Remis | Tabelle | eigene KPKP-Tabelle, `matelist` | 4.125 | Oppositionsstudie, `1.Kg5?` remis |
| 11 | KBN-K | 4 | `8/8/8/8/3k4/8/N7/KB6 w - - 0 1` | Matt in 31 | Tabelle | Tablebase, eigene Tabelle | 29.958 | braucht Transpositionen oder df-pn; `mateab` 1,4 Mrd. Knoten |
| 12 | KQ-KBN | 5 | `8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1` | Matt in 39 | Tabelle (`5-KBNKQ.bin`) | Tablebase, eigene Fünf-Steiner-Tabelle (Gewinn in 77 Halbzügen) | | ohne Datei scheitern alle Suchen bisher (`matepn` 13 Halbzüge in 3 s); Ziel des nächsten `matepn`-Schritts |

## Die Seite am Zug wird matt

Dieselben Stellungen einen Zug später, mit dem Verlierer am Zug: Die Suche muss das
drohende Matt als `score mate -N` melden, mit der längsten Verteidigung zuerst.

| Name | Steine | FEN / Züge | Ergebnis | Bestätigt durch |
|---|---|---|---|---|
| Bauern, Schwarz am Zug | 11 | `5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1 moves g3h4` | gematt in 5 (`f8g7 h7h8q ...`), `f8e7` verliert in 4 | `matepn` (5.776 Knoten) und `mateab` |
| KQQ-KN, Schwarz am Zug | 5 | `7k/5n2/8/8/8/8/Q4Q2/K7 b - - 0 1` | gematt in 3 (`f7h6 f2f8 h8h7 a2b1 h6f5 b1f5`), Königszüge verlieren in 2 | `matepn` (182 Knoten) und `mateab` |

## Benchmark: KRR-KN, Schwarz am Zug

`8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1`, zwölf legale Züge, alle verlieren. Die perfekte
Lösung aus der Fünf-Steiner-Tabelle (Mattjes liefert sie sofort, wenn `5-KRRKN.bin`
im Cache liegt):

| Zug | Wert | | Zug | Wert |
|---|---|---|---|---|
| Ng3 | −M9 | | Kd7, Ke7, Kd6, Kf6, Kd5, Ke5, Kf5 | −M6 |
| Ne3, Nd2 | −M8 | | Kf7 | −M4 |
| Nh2 | −M7 | | | |

Andere Engines, sechs Kerne, MultiPV 12, bis alle zwölf Werte fehlerfrei standen
(Messung des Autors, 2026-10-09): AsmFish 10, Patricia und Slow Chess Blitz 1 s,
Reckless 0.9.0 6 s, Stockfish 19 7 s, Spike 1.4 13 s, Komodo Dragon 3.1 94 s.

Mattjes ohne Tabelle, ein Thread, Hash 256, bis alle zwölf Werte exakt standen
(2026-10-09, nach dem Umbau auf den Wurzeltreiber):

| Suche | Zeit | Knoten | Ergebnis |
|---|---|---|---|
| `matepn` | 93 s | 17,8 Mio. Besuche | alle zwölf exakt |
| `mateab` | 122 s | 414 Mio. Knoten | alle zwölf exakt |
| `matelist` | 6 min bis Halbzug 13 | | kein Ergebnis, siehe unten |

Nach 40 s (`matepn`) bzw. 20 s (`mateab`) stand Tiefe 15 mit neun exakten Werten
(`Kf7`, die Königszüge, `Nh2`); `Ne3` und `Nd2` brauchen Tiefe 16, `Ng3` Tiefe 18.
Messgröße für Mechanik-Änderungen: die Zeit bis Tiefe 15 bei gleichen 5.528.849
Besuchen (`matepn`, Hash 256): Rust 23,1 s vor und 16,5 s nach Prefetch und
Orakel-Abkürzung (2026-10-09), Go 31,3 s → 25,8 s.
`matelist` ist hier das falsche Werkzeug: Es antwortet erst nach der Rückwärtsphase,
die auf die komplette Aufzählung folgt, und der erreichbare Raum ist der ganze
Fünf-Steiner ohne Symmetrie (50 Mio. Stellungen bei Halbzug 10, 1,4 Mrd. insgesamt).
Mit `go mate 7` würde es bei Halbzug 13 anhalten und dann auflösen, braucht dafür aber
zweistellige Gigabyte. Das ist die Messlatte für die nächsten Schritte an den Suchen:
ein Thread gegen sechs Kerne, Faktor 10 bis 90 zu den schnellsten Engines.

## Ohne gesichertes Ergebnis

Stellungen, die bisher nur als Last- oder Verhaltenstests dienen; Ergebnis offen, bis
eine Sechs-Steiner-Tabelle oder eine fertige Suche es liefert.

| Name | Steine | FEN | Stand |
|---|---|---|---|
| KRRN-KN | 6 | `8/8/4k3/8/8/8/R7/K1RN1n2 w - - 0 1` | kein Matt in 5 Halbzügen (`matepn`, `mateab`) |
| KRR-KBN | 6 | `8/8/8/3kb3/3n4/8/R7/KR6 w - - 0 1` | kein Matt in 17 Halbzügen (`matepn` 11 Mio. Besuche, `mateab` 195 Mio. Knoten); Wert steht in `6-KRRKBN.bin` (längstes Matt des Materials 44 Züge) |

## Benutzung

In Arena die FEN laden und `go infinite` oder `go mate N` geben; `go mate N` begrenzt
die Suche auf 2N − 1 Halbzüge. Für die Konsole: `mateabSolveNamed("KBN-K", ...)`,
`matepnSolveNamed(...)` und `matelistSolve(...)` in `experiments()` nehmen die Namen
der ersten Tabelle.
