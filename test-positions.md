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
| 7 | KQ-KN | 4 | `7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1` | Matt in 12 | Tabelle | Tablebase, eigene Tabelle | 2.687 | Zugzwang, Pattfallen; `matepn` 214 Tsd. bis 848 Tsd. Besuche ohne Tabellen, je nach Tabellengröße (`docs/m4-mate-search.md`, Schritt 5) |
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
| `matepn` | 93 s, nach Prefetch und Orakel-Abkürzung 71 s | 17,8 Mio. Besuche | alle zwölf exakt |
| `mateab` | 122 s, danach 111 s | 414 Mio. Knoten | alle zwölf exakt |
| `matelist` | 6 min bis Halbzug 13 | | kein Ergebnis, siehe unten |

Nach 40 s (`matepn`) bzw. 20 s (`mateab`) stand Tiefe 15 mit neun exakten Werten
(`Kf7`, die Königszüge, `Nh2`); `Ne3` und `Nd2` brauchen Tiefe 16, `Ng3` Tiefe 18.

Zeitleiste pro Wert (Rust, ein Thread, Hash 16384, 2026-10-13, Stand d6cdbff plus
Option `MatePnMobility on`; `mateab` mit derselben Tabellengröße). Das ist die
Messgröße für Algorithmus-Änderungen: pro Variante die Zeit, bis jeder Wert exakt
steht, nicht nur die Summe. Die Varianten `off` und `lazy` stehen in
`docs/m4-mate-search.md`, Schritt 5 (beide langsamer, `lazy` um 19 %).

| Tiefe | Züge | `matepn` | Besuche | `mateab` | Knoten |
|---|---|---|---|---|---|
| 8 | Kf7 (−M4) | 0,3 s | 81 Tsd. | 0,06 s | 273 Tsd. |
| 12 | Kd7, Ke7, Kd6, Kf6, Kd5, Ke5, Kf5 (−M6) | 7,3 s | 1,7 Mio. | 2,8 s | 12,0 Mio. |
| 14 | Nh2 (−M7) | 22,4 s | 5,6 Mio. | 15,7 s | 61,5 Mio. |
| 16 | Nd2, Ne3 (−M8) | 70,3 s | 17,8 Mio. | 53,8 s | 198 Mio. |
| 18 | Ng3 (−M9) | 70,5 s | 17,8 Mio. | 115,7 s | 414 Mio. |

Zwei Drittel der Zeit gehen in Tiefe 16 (die drei Springerzüge bei 15 Halbzügen);
`Ng3` ist für `matepn` danach fast gratis, weil die Widerlegung bei Tiefe 16 die
Tabelle so gefüllt hat, dass der Beweis bei 18 sofort steht; `mateab` rechnet den
Raum für Tiefe 18 noch einmal (62 s). Bis Tiefe 16 wächst die Zeit etwa um Faktor 3
je zwei Halbzüge. Die erste Tiefenzeile des ersten Laufs einer Sitzung enthält die
Seitenfehler der frisch angelegten Tabelle (bei 16 GB rund 2,4 s). Die Stellung übt
nur gerade Tiefen (Wurzel = Verteidiger). Mit Weiß am Zug taugt sie nicht als
Angreifer-Benchmark: Rxf1 führt in den Vier-Steiner, die Tabellenstufe meldet den
bekannten Gewinn (Matt in 6) und die Suche läuft gar nicht.

Angreiferpfad: die Stellung nach `Nh2`, `8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2` (Matt in
7, 27 Züge, kein Schlag möglich, Werte aus derselben Tabelle). Dieselbe Messung,
Zeit und Besuche bis zu den Werten je Tiefe; nach Tiefe 21 von Hand gestoppt, die
16 übrigen Züge sind länger als Matt in 11:

| Tiefe | Züge | `matepn` | Besuche | `mateab` | Knoten |
|---|---|---|---|---|---|
| 13 | Rc5 (M7) | 13,9 s | 3,5 Mio. | 6,9 s | 3,9 Mio. |
| 15 | Ra5 (M8) | 48,7 s | 12,5 Mio. | 25,3 s | 94,7 Mio. |
| 19 | Ka3, Kb3, Kc3, Ra3, Rc3, Re1+, Rh1 (M10) | 368 s | 92,6 Mio. | 382 s | 1,31 Mrd. |
| 21 | Kc2, Ka1, Kb1 (M11) | 450 s | 107 Mio. | 858 s | 2,81 Mrd. |

`mateab` findet das erste Matt schneller (0,3 µs je Knoten gegen 4 µs je Besuch),
`matepn` holt mit jeder weiteren Tiefe auf und liegt ab Tiefe 19 vorn.

Mit `5-KRRKN.bin` im Cache (seit 2026-10-15 quantisiert auf Eimer von 8 Zügen,
Format v2) löst `matepn` über das Intervall-Orakel alle zwölf Werte mit 65.123
Besuchen in 0,28 s und alle 27 Werte nach Nh2 mit 71.697 Besuchen in 0,3 s auf,
exakt wie die exakte Tabelle (Go und Rust zeilenidentisch; mit Eimern von 4 Zügen
wären es 288 bzw. 1.174 Besuche in 1 ms, `docs/m5-endgame-tables.md`).
Messgröße für Mechanik-Änderungen: die Zeit bis Tiefe 15 bei gleichen 5.528.849
Besuchen (`matepn`, Hash 256): Rust 23,1 s vor und 16,5 s nach Prefetch und
Orakel-Abkürzung (2026-10-09), Go 31,3 s → 25,8 s. Bei Hash 16384 kostet ein
Besuch 4 µs statt 3 µs (TLB-Misses der großen Tabelle).
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

## WDL-Teststellungen (Milestone 7)

Stellungen für die WDL-Suche (`docs/m7-wdl-search-design.md`): gesucht wird nur
Gewinn / Remis / Verlust, keine Mattdistanz, Tabellen während der Suche aus.
Geprüft am 2026-10-21 mit Stockfish 19 und Syzygy (3 bis 6 Steine,
Fünfzig-Züge-Regel aus, `tests/sf-wdl.sh`, Liste `tests/wdl-positions.txt`):
`cp 20000` dort ist "Tabellen-Gewinn", `cp 0` "Tabellen-Remis". "Züge" zählt die
Wurzelzüge nach Ergebnis (G / R / V), das ist die MultiPV-Erwartung: Die Suche
muss jeden Wurzelzug einzeln beweisen.

### Gewinn oder Verlust der Seite am Zug

| Name | Steine | FEN | Wurzel | Züge G/R/V | Hinweis |
|---|---|---|---|---|---|
| KQ-KQ Dame hängt | 4 | `8/8/8/4k3/8/8/4q3/K2Q4 w - - 0 1` | Gewinn | 1/9/8 | Rauchtest, `Dxe2` in einem Halbzug, die übrigen Züge remis oder verloren |
| KQ-K | 4 | `8/8/8/4k3/8/8/8/K2Q4 w - - 0 1` | Gewinn | 20/3/0 | drei Damenzüge geben den Gewinn her (Patt-Nähe) |
| KR-K | 4 | `8/8/8/4k3/8/8/8/K2R4 w - - 0 1` | Gewinn | 13/3/0 | |
| KQ-KN | 4 | `7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1` | Gewinn | 24/1/0 | `Dxf7` ist Patt |
| KBB-K | 4 | `8/8/4k3/8/8/8/8/K2BB3 w - - 0 1` | Gewinn | 17/0/0 | |
| KBN-K | 4 | `8/8/8/8/3k4/8/N7/KB6 w - - 0 1` | Gewinn | 7/3/0 | langer Gewinnbeweis ohne Mattdistanz, Vergleich mit `matepn` (29.958 Beweisstellungen für das Matt) |
| KP-K König vor dem Bauern, Weiß | 3 | `4k3/8/4K3/4P3/8/8/8/8 w - - 0 1` | Gewinn | 2/2/0 | |
| KP-K König vor dem Bauern, Schwarz | 3 | `4k3/8/4K3/4P3/8/8/8/8 b - - 0 1` | Verlust | 0/0/2 | beide Züge verlieren |
| KBP-K richtiger Läufer | 4 | `k7/8/K7/8/8/8/P7/5B2 w - - 0 1` | Gewinn | 11/0/0 | Gegenstück zum falschen Läufer unten |
| KQ-KR | 4 | `8/8/3k4/8/8/8/3r4/K2Q4 w - - 0 1` | Gewinn | 12/0/3 | drei Züge hängen die Dame |
| KQ-KP Springerbauer | 4 | `7K/8/8/1Q6/8/8/1p6/k7 w - - 0 1` | Gewinn | 4/21/0 | nur vier von 25 Zügen gewinnen, der Rest remis |
| KQQ-KN | 5 | `7k/5n2/8/8/8/8/Q4Q2/K7 w - - 0 1` | Gewinn | 37/2/0 | Suchfall aus der ersten Tabelle |
| KRR-KN Benchmark, Schwarz | 5 | `8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1` | Verlust | 0/0/12 | alle zwölf Züge verlieren; Zeit gegen die 70 s mit Mattdistanz |
| KRRN-KN einsamer Springer, Weiß | 6 | `8/8/4k3/8/8/8/R7/K1RN1n2 w - - 0 1` | Gewinn | 27/0/0 | Stellung des Autors; alle Züge gewinnen, der Springer allein gewinnt nie, aber das muss die Suche beweisen (ersticktes Matt ist legal) |
| KRRN-KN einsamer Springer, Schwarz | 6 | `8/8/4k3/8/8/4N3/R7/K1R2n2 b - - 1 1` | Verlust | 0/0/10 | nach Ne3, Matt in höchstens 8; misst die Kosten des Beweises "der Springer gewinnt nicht" |
| KRRN-KNP sieben Steine | 7 | `8/5p2/4k3/8/8/8/R7/K1RN1n2 w - - 0 1` | Gewinn | 27/0/0 | die Springer-Stellung plus schwarzer Bauer f7: jenseits der Sechs-Steiner-Syzygy, jeder Schlag landet in ihnen; Stockfish bestätigt 27 Gewinne |
| KQ-KBN | 5 | `8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1` | Gewinn | 19/1/4 | Matt in 39, ohne Tabelle bisher unerreichbar |
| KQ-KRR | 5 | `k7/8/8/8/8/2r5/1r6/K5Q1 w - - 0 1` | Gewinn | 3/8/10 | nur `Kb2`, `Dg8`, `Dh1` gewinnen, zehn Züge verlieren |
| KQ-KRB | 5 | `k7/8/8/8/8/8/5rb1/K2Q4 w - - 0 1` | Gewinn | 2/12/7 | nur `Dd8` und `Da4` gewinnen |

### Remis der Seite am Zug

| Name | Steine | FEN | Wurzel | Züge G/R/V | Hinweis |
|---|---|---|---|---|---|
| KP-K Randbauer | 3 | `k7/8/K7/8/8/8/P7/8 w - - 0 1` | Remis | 0/5/0 | Patt in der Ecke |
| KP-K Opposition | 3 | `8/8/8/8/8/4k3/4P3/4K3 w - - 0 1` | Remis | 0/2/0 | der schwarze König pendelt vor dem Bauern |
| KNN-K | 4 | `8/8/8/4k3/8/8/8/K1NN4 w - - 0 1` | Remis | 0/11/0 | Härtetest: die Widerlegung des Gewinns braucht den ganzen erreichbaren Graphen |
| KBP-K falscher Läufer | 4 | `k7/8/K7/8/8/8/P7/2B5 w - - 0 1` | Remis | 0/12/0 | Festung in der Ecke, der König pendelt a8/b8 |
| KQ-KP c-Bauer | 4 | `7K/8/8/Q7/8/8/2p5/1k6 w - - 0 1` | Remis | 0/22/2 | Patt-Trick `Ka1`; `Da1+`/`Da2+` verlieren die Dame |
| KQ-KP a-Bauer | 4 | `7K/8/8/Q7/8/8/p7/1k6 w - - 0 1` | Remis | 0/23/0 | |
| KR-KB sichere Ecke, Schwarz | 4 | `k7/8/K7/8/8/8/5b2/1R6 b - - 0 1` | Remis | 0/6/3 | drei Läuferzüge lassen `Tb8` matt zu |
| KR-KB Läufer deckt b6, Weiß | 4 | `k7/8/K7/8/8/4b3/8/1R6 w - - 0 1` | Remis | 0/16/0 | |
| KBP-KB ungleiche Läufer | 5 | `8/8/8/3k4/8/2K5/2PB4/5b2 w - - 0 1` | Remis | 0/9/0 | Blockade, Kandidat für die Remis-Politik |
| KQ-KRP Festung | 5 | `8/8/5kp1/5r2/8/8/8/K2Q4 w - - 0 1` | Remis | 0/18/5 | Turm vom Bauern gedeckt; fünf Damenzüge verlieren |
| KQ-KRR Dauerschach | 5 | `3k4/8/8/8/8/8/5rr1/K6Q w - - 0 1` | Remis | 0/4/11 | nur `Dh4+`, `Dh8+`, `Db1`, `Dd1` halten, der Rest verliert: der Dauerschach-Fall |
| KRB-KR Philidor-Typ, Schwarz | 5 | `8/8/8/8/8/1k6/4r3/K1BR4 b - - 0 1` | Remis | 0/15/4 | |
| KNN-KP | 5 | `8/8/8/8/2k5/8/2p5/K1NN4 w - - 0 1` | Remis | 0/4/6 | sechs von zehn Zügen verlieren gegen den Bauern |

Alle Stellungen bis fünf Steine kennen auch unsere eigenen Tabellen (Basis und
Fünf-Steiner-Cache), sie dienen als zweite Verifikation, wenn die Suche mit
abgeschalteten Tabellen läuft.
