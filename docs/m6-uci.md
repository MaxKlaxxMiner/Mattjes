# Milestone 6: UCI

Vorgezogen am 2026-10-08, damit Mattjes gegen echte Schachprogramme getestet werden
kann, bevor die Suchalgorithmen darunter liegen. Die Richtung des Autors (fünf Stufen
nach wachsender Gewissheit) steht in `CLAUDE.md`; hier geht es um das, was läuft.

## Stufe a: Antworten aus den Tabellen (2026-10-08)

Package `uci/` in Go, Modul `src/uci/` in Rust. **Die blanke Binary ist die Engine**,
wie jede GUI es erwartet: `runMattjesGo.exe` oder `runMattjesRs.exe` ohne Argument
spricht UCI auf stdin/stdout und meldet sich ungefragt mit einer Zeile ("Mattjes 0.6 by
Max Klaxx Miner, ..."), wie die meisten Engines. Argumente wählen Tests und Aktionen
(`test` für den Experiment-Block aus `main`, `egtb-list`, `egtb-measure`). Threads
werden nicht per Argument gesetzt, sondern über die UCI-Option `Threads` (Vorgabe: alle
CPUs, nur für das Erzeugen von Tabellen relevant). Das Arbeitsverzeichnis muss das
Repo-Root sein, weil der Cache-Ordner `mattjes-egtb-cache/` neben der Binary liegt;
fehlt die Basis, wird sie beim ersten `isready` erzeugt (17 s, Fortschritt als
`info string`).

**Protokoll:** `uci`, `isready`, `setoption name MultiPV|Threads value N`, `ucinewgame`,
`position startpos|fen ... [moves ...]`, `go` (in Stufe a alle Parameter ignoriert,
die Antwort kommt sofort; Zeit- und Tiefenparameter seit Stufe c), `stop`, `quit`,
dazu `d` zum Ausgeben der FEN.

**Antwort:** Jeder Wurzelzug wird gespielt und das Kind in der Tabelle nachgeschlagen.
Daraus hat jeder Zug einen exakten Wert aus Sicht der Seite am Zug: Gewinn in n,
Remis, Verlust in n, oder "keine Tabelle". Sortierung: kürzester Gewinn, dann Remis,
dann unbekannt, dann längster Verlust. Für jeden der ersten MultiPV Züge wird die
komplette Linie bis zum Matt ausgegeben (Gewinner nimmt ein Kind mit Verlust in n − 1,
Verlierer eins mit Gewinn in n − 1, also die längste Verteidigung), auch über Schläge
und Umwandlungen hinweg, die in kleinere Tabellen führen.

```
info depth 61 multipv 1 score mate 31 pv a1b2 d4c4 b2c2 ... d3a6 c6c7
info depth 61 multipv 2 score mate 31 pv a2b4 d4c5 ...
bestmove a1b2
```

`depth` ist die Länge der längsten Variante des Blocks in Halbzügen, für alle
MultiPV-Zeilen dieselbe (seit 2026-10-16; vorher je Zeile die eigene Länge, und GUIs
verwarfen die Remiszüge mit Tiefe 1 als veraltet, etwa Nc3, Be4 und Bd3 in KBN-K),
`score mate N` die Zugzahl aus DTM, negativ aus Sicht des Verlierers. Remis gibt
`score cp 0` ohne Variante.

**Was die Tabellen nicht abdecken:** Stellungen mit En-passant-Recht liegen nicht in
den Tabellen; `eval` bewertet sie einen Halbzug tiefer aus den Enkeln (Matt und Patt
terminal). Materialien ab fünf Steinen werden beim ersten Auftreten registriert und
aus `mattjes-egtb-cache/5-NAME.bin` geladen, wenn die Datei da ist (KQ-KBN liefert so
sofort Matt in 39 mit Variante); ohne Datei bleibt der Zug unbekannt. Hat kein
Wurzelzug einen Tabellenwert, meldet die Engine `info string no endgame table for this
material` und spielt den ersten legalen Zug. Das ist der Platz für die Suche (Stufe c).

**Verifikation:** Beide Binaries laufen gegen dasselbe Skript (`uci`, `isready`,
MultiPV 3, KBN-K, KP-KP mit Weiß und Schwarz am Zug, KQ-KBN, KBB-K nach `moves d1c2`,
Startstellung, Mattstellung, KQ-K) und liefern **zeilenidentische** Ausgaben:
KBN-K Matt in 31 (drei Erstzüge mit 31), KP-KP Matt in 25 für Weiß mit `b7b8q` in der
Variante und Remis für Schwarz, KQ-KBN Matt in 39 nach dem Laden der Fünf-Steiner-Datei,
KBB-K nach einem Nebenzug Matt in −17 für Schwarz, Mattstellung `bestmove 0000`,
KQ-K Matt in 5.

## Stufe b: Tabellen im Hintergrund erzeugen (2026-10-08)

Vier Optionen, alle mit Vorgabe aus:

| Option | Wirkung |
|---|---|
| `EgtbWriteCache` | erzeugte Tabellen in den Cache-Ordner schreiben; die Basis wird in jedem Fall erzeugt, vorhandene Dateien werden immer gelesen. Wird die Option später eingeschaltet, schreibt die Engine sofort alles, was nur im RAM liegt und noch keine Datei hat (Basis eingeschlossen) |
| `EgtbGenerate5` | fehlende Fünf-Steiner bei Bedarf erzeugen (Sekunden bis zwei Minuten, bis 6 GB RAM bei Bauern-Materialien) |
| `EgtbGenerate6` | fehlende Sechs-Steiner (Stunden, 23 bis 68 GB RAM), nur mit viel RAM sinnvoll |
| `EgtbPath` | Cache-Ordner, Vorgabe `mattjes-egtb-cache` neben der Binary |

Ablauf (seit 2026-10-16 **bei jedem `go`, neben der Suche**; bis dahin nur bei
`go infinite` und anstelle der Suche): Eine Erzeugung startet, wenn die Tabelle des
Wurzelmaterials fehlt und die Option es erlaubt. Die Engine antwortet zuerst wie bisher
aus den Tabellen, startet die Suche wie sonst auch in ihrem Thread und daneben die
Erzeugung in einem eigenen Thread auf einer Kopie der geladenen Tabellen (`Set.Fork`,
173 MB für die Basis), damit die Hauptschleife jederzeit `isready` beantwortet und
weitere Kommandos liest. Die Erzeugung bekommt **physische Kerne minus eins** Worker
(`egtb.PhysicalCores`, durch `Threads` begrenzt; beim Autor 5 neben dem Such-Thread auf
6 Kernen mit 12 Threads, damit sich die Suche keinen Kern mit einem SMT-Geschwister
teilt), die Hash-Tabelle der Suche bleibt stehen, solange die RAM-Schätzung neben sie
passt. `stop` beendet die Suche wie sonst und **pausiert** die Erzeugung: Die Worker
halten an der nächsten Chunk-Grenze an (Millisekunden), alles Gerechnete bleibt im RAM.
Das nächste `go` mit demselben Material setzt genau dort fort; ein anderes Material
verwirft den pausierten Job. Ist die Tabelle fertig, stoppt die Hauptschleife die
laufende Suche (ihr Teilergebnis wird verworfen, weil das Set sich ändert; der
Rust-Such-Thread besitzt es sogar), übernimmt die Tabelle (`Set.Adopt`), schreibt sie
bei `EgtbWriteCache` in den Cache und rechnet die Antwort neu: Tabellenstufe, und wo
die Tabelle nur Eimer liefert (geschrieben = quantisiert), die Suche noch einmal mit
der Restzeit des ursprünglichen `go` (`go infinite` läuft als `go infinite` weiter).
Benchmark-Stellung KRR-KN ohne `5-KRRKN.bin`, `go movetime 120000`, Arbeitsrechner:
Rust antwortet nach 26,8 s mit allen zwölf exakten Werten (Erzeugung 26,0 s mit fünf
Workern, die Suche kam nebenher bis Tiefe 15), Go nach 30,6 s; Suche allein 70 s,
Erzeugung allein mit zwölf Workern 20 s. Mit `EgtbWriteCache` kommt die Tabelle
quantisiert an und die neu gestartete Suche löst die Eimer in rund 0,3 s auf.

Zwei Bremsen, damit nicht gerechnet wird, was niemand braucht (Hinweis des Autors an
KQQ-KN, `7k/5n2/8/8/8/8/Q4Q2/K7 w`): Beantwortet die Tabellenstufe die Stellung
abschließend, startet keine Erzeugung. Und löst die Suche von selbst alles auf, was
gefragt war (alle Züge, oder bei MultiPV 1 das kürzeste Matt), wird eine laufende
Erzeugung **pausiert** (`searchEnd.answered`): `info string egtb: generation of KQQKN
paused, the search answered the position; the next go with this material resumes it`.
Bei KQQ-KN steht das nach 80 ms statt nach einer Minute Rechnen auf fünf Kernen; das
nächste `go` im selben Material setzt fort, ein anderes Material verwirft den Job.

**Erzeugung nach Bedarf (2026-10-16):** Welche Tabellen eine Suche bräuchte, sagt ein
Zähler: `Set.Lookup` zählt jede Anfrage an ein Material bis sechs Steine ohne Tabelle
(nicht registriert oder registriert und nicht erzeugt), und nach jeder Suche steht
`info string egtb: no table for KRRKBN (15,700,667 probes), KRRKB (4,659,497 probes),
KBNKR (2,996,298 probes)` mit den drei meistgefragten (`Set.MissingTables`; die Zähler
laufen bis `ucinewgame` weiter, ein gestartetes Material fällt heraus). Erreichbarkeit
wäre das falsche Kriterium gewesen: Vom Bauern-Test
(`5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w`, zwölf Steine) aus wären rund 70 der 110
Fünf-Steiner erreichbar, die Suche fragt aber in zehn Sekunden **keine einzige**
Stellung mit sechs oder weniger Steinen an, das Matt in 6 liegt elf Halbzüge tief und
sieben Schläge entfernt. Die Liste ist der Auftrag (`generationTarget`): Ist das
Wurzelmaterial nicht erzeugbar (mehr als sechs Steine, oder sechs ohne
`EgtbGenerate6`), wird das meistgefragte fehlende Material genommen, das die Optionen
erlauben. Entschieden wird bei jedem `go` (mit dem Bedarf der vorigen Suche auf
derselben Stellung), **während der Suche** und nach jeder fertigen Tabelle (dann das
nächste). Während der Suche geht das so: Kennt die Engine beim `go` noch keinen Bedarf,
legt sie vor dem Start eine Kopie des Sets zurück (`spare`, 173 MB plus exakte
RAM-Tabellen, denn der Such-Thread besitzt das Set danach, in Rust wörtlich), der
Reporter klopft mit jeder Fortschrittszeile alle fünf Sekunden bei der Hauptschleife an
(`demandTick`, Rust `Event::Tick`), die liest die Zähler (in Go unter einem Mutex, in
Rust über einen `Arc` auf die Zählertabelle, `demand_handle`) und startet den Job auf
der Kopie. Nach einem `stop` startet nichts mehr: Die Engine ist dann still, ein
laufender Job pausiert wie bisher, der Bedarf bleibt bis `ucinewgame` stehen und
startet beim nächsten `go` sofort. Nach einer Suche, die von der Zeit beendet wurde,
läuft ein laufender Job weiter, aber auch dann startet kein neuer. Beispiel KRRN-KN
(`8/8/4k3/8/8/8/R7/K1RN1n2 w`, sechs Steine, nur `EgtbGenerate5`, `go movetime
40000`, Rust): nach 5 s `the search asked 6,494,526 times for KRRNK, generating it`,
nach 30 s `KRRNK ready`, die Suche startet mit der Restzeit neu und sofort beginnt
KRNKN (9,6 Mio. Anfragen); Go dasselbe mit anderen Zahlen. `go infinite` mit `stop`
nach 12 s: KRRNK startet nach 5 s, `stop` pausiert ihn, danach kommt nichts mehr. Go
und Rust melden dieselben Materialien in derselben Reihenfolge, die Zahlen hängen bei
Zeitläufen vom Tempo ab; eine Tabelle, die inzwischen da ist, fällt aus der Liste.

```
info string egtb: generating KBBBK (231 MB table, about 346 MB RAM), stop pauses it
info string egtb: generation of KBBBK paused, the next go resumes it
bestmove a1a2
info string egtb: generation of KBBBK resumed
info string egtb: KBBBK  242221056 indices,   28017470 legal,    21487 mates
info string egtb: KBBBK wins   8089520, losses  11681659, draws   8246291, longest mate 31 plies = 16 moves, 6.4 s
info string egtb: KBBBK ready
info depth 23 multipv 1 score mate 12 pv a1b2 e5d6 b2c3 ...
```

**Wenige, kompakte Statuszeilen** (Vorgabe des Autors, 2026-10-08): eine Start- und eine
Abschlusszeile pro Tabelle, die bis zu 254 Ebenen-Zeilen werden auf eine alle fünf
Sekunden gedrosselt (`progressInterval`), die Erzeugung der Basis beim ersten `isready`
meldet jede der 35 Tabellen mit einer Zeile. **Lange Phasen melden Prozent** (2026-10-09,
nach dem ersten Sechs-Steiner-Versuch KRRKBN, der minutenlang stumm blieb): Der
Generator scannt vor der ersten Ebene alle Indizes, bei 15,5 Mrd. dauert das Minuten,
und einzelne Ebenen können ebenso lange laufen. Der wartende Thread gibt deshalb alle
fünf Sekunden `KRRKBN scanning 23% (62 s)` bzw. `level 12: 37% (41 s)` aus
(`tickInterval` im Generator, gilt auch für die Konsole). Die Spaltenausrichtung der Konsole wird für
die GUI auf einfache Leerzeichen zusammengezogen (`compact`), Arena zeigt keine
Festbreitenschrift.
Go und Rust verhalten sich in beiden Läufen (mit und ohne Schreiben) gleich, die
geschriebene Datei ist byteidentisch mit der aus `egtb-measure`.

**Speicher-Check und Log (2026-10-09):** Vor dem Start vergleicht die Engine den Bedarf
mit dem RAM der Maschine. Der Bedarf kommt aus `egtb.Requirements` (gemessene
Materialien: Spitzen-RAM, Zeit, Dateigröße, Maschine; von Hand aus `measure.log`
übernommen, wie die Prüfsummen) oder, wenn nicht gemessen, aus der Schätzung Tabelle ×
1,75 (Tabelle, vier Bitsets, `pending`-Listen) plus fehlende Abhängigkeiten. Der freie
und der gesamte physische Speicher kommen vom Betriebssystem (Windows
`GlobalMemoryStatusEx`, Linux `/proc/meminfo`):

```
info string egtb: generating KRRKBN (14784 MB table, about 26.8 GiB RAM measured, 110.2 GiB of 127.0 GiB free), stop pauses it
info string egtb: WARNING: more than the free memory, expect swapping        <- Bedarf über dem freien RAM
info string egtb: KRRKBN needs about 26.8 GiB (measured), the machine has 16.0 GiB: not started
```

Nach jeder Erzeugung hängt auch der UCI-Job eine Zeile an `measure.log`, mit denselben
Feldern wie `egtb-measure` plus `peak_commit_mb`/`peak_ws_mb` und der Quelle `uci`;
daraus werden die Konstanten in `Requirements` und `TableChecksums` nachgetragen.

Vorgemerkt: Während einer Erzeugung sollen Hash und reguläre Suche ihren RAM komplett
freigeben.

## Stufe c: Suche für Stellungen ohne Tabellenwert (2026-10-09)

Zwei neue Optionen:

| Option | Wirkung |
|---|---|
| `Search` | Combo `none`, `mateab`, `matepn` (Vorgabe), `matelist`: der Algorithmus aus Milestone 4 für Stellungen, die die Tabellen nicht entscheiden |
| `Hash` | MB für die Transposition Table der Suche (Vorgabe 256, Minimum 32, weil `mateab` 21 Value-Bits braucht); bei `matelist` das Stellungsbudget (128 Byte pro Stellung) |
| `MatePnMobility` | Combo `on` (Vorgabe), `off`, `lazy` (seit 2026-10-13): wie `matepn` frische Blätter initialisiert, `on` = Zuganzahl per Orakel und Generator pro Blatt, `off` = 1/1, `lazy` = 1/1 und beim Betreten die eigene Zuganzahl als erste Schätzung, die bei gerissener Schwelle ohne Enkel-Expansion zurückkehrt; Messexperiment aus `docs/handout-matepn-mobility.md` |

**Quantisierte Cache-Tabellen (2026-10-15):** Cache-Dateien ab fünf Steinen halten
Eimer von acht Zügen statt exakter Distanzen (`egtb.FileStep`, Format v2,
`docs/m5-endgame-tables.md`, Abschnitt "Quantisierte Werte"; keine Option, die
Basis bleibt exakt). Die Tabellenstufe meldet solche Werte nicht (`eval` nur bei
exakter Distanz), die Suche löst sie über das Intervall-Orakel auf (Schritt 6 in
`docs/m4-mate-search.md`): KRR-KN alle zwölf Werte nach rund 0,3 s statt sofort,
KQ-KBN Matt in 39 ebenso. Beim Laden steht `info string egtb: loaded ... (step 8,
the search resolves exact distances)`; die Variante endet an solchen Tabellen.
Eine Hintergrund-Erzeugung mit `EgtbWriteCache` quantisiert die Tabelle beim
Schreiben, die Antwort danach kommt deshalb aus der Suche; ohne Schreiben bleibt
die erzeugte Tabelle im RAM exakt. Seit 2026-10-16 bleiben geladene Tabellen ab
fünf Steinen gepackt in der Datei und werden Block für Block beim ersten Zugriff
entpackt (Format v3, `docs/m5-endgame-tables.md`, "Blockformat"); nach jeder
Suche steht `info string egtb: block cache N hits, M misses, B of T blocks
touched`, Go und Rust zählen gleich.

**Wann gesucht wird:** Zuerst antwortet die Tabellenstufe wie bisher. Sie gilt als
abschließend, wenn es keine Züge gibt oder alle Züge bekannt sind (dann ist das
Ergebnis exakt, auch Remis und Verlust). Bis 2026-10-16 reichte auch ein bekannter
Gewinn als bester Zug; das bewies aber nur den Gewinn, nicht, dass kein stiller Zug
schneller mattsetzt (in `8/8/4k3/8/8/8/R7/K1R2n2 w` wurde neben Rxf1 kein anderer Zug
angesehen). Sonst steckt hinter einem unbekannten Zug vielleicht ein kürzeres Matt,
und die Suche läuft über der Stellung: `mateab` und `matepn` mit den Tabellen als
Orakel (iterative Vertiefung über ungerade Halbzüge bis 127, also Matt in 64),
`matelist` mit Aufzählung bis zum Budget. Der Wurzeltreiber bekommt die exakt
bekannten Züge mit: Gewinne und Verluste starten als bewiesen mit Tabellenwert und
-variante, Remis als erledigt und werden nie gesucht; die Schleife endet, wenn alle
Züge erledigt sind. Bei MultiPV 1 wird bei einem bekannten Gewinn in n Halbzügen nur
bis n − 2 nach etwas Kürzerem gesucht (Rxf1 oben: Tiefen 1 bis 11, 8 Tsd. Besuche,
dann steht Matt in 7 exakt); mit MultiPV werden alle offenen Züge aufgelöst.
Vorher lädt die Hauptschleife alle Cache-Dateien, die die Suche durch Schläge und
Umwandlungen erreichen kann (`loadDependencies`, rekursiv über
`Material.Dependencies`), denn der Such-Thread lädt nichts.

**Thread, Stop, Zeit:** Die Suche läuft in einem eigenen Thread, die Hauptschleife
liest weiter Kommandos (`isready` wird sofort beantwortet). `stop` setzt ein atomares
Flag, das die Suche alle 4.096 Knoten abfragt; sie wickelt den Baum ohne weitere
Einträge ab (alles vorher Gespeicherte ist vollständig und bleibt gültig) und liefert
die letzte komplett gerechnete Tiefe. `go movetime N` setzt das Flag über einen Timer,
`go wtime/btime/winc/binc` nimmt ein Zwanzigstel der Restzeit plus halbes Inkrement
(mindestens 50 ms), `go mate N` begrenzt auf 2N − 1 Halbzüge, `go depth N` auf N
(aufgerundet auf ungerade), `go infinite` läuft bis `stop` und hält `bestmove` auch
dann zurück, wenn das Matt längst gefunden ist. `matelist` prüft das Flag nur an
Ebenengrenzen und löst dann auf, was es erreicht hat. Ein `go` während einer laufenden
Suche beendet sie zuerst (eine GUI schickt ohnehin `stop`), ebenso eine fertige
Hintergrund-Erzeugung, weil sie den Tabellensatz austauscht.

**Ausgabe:** je abgeschlossener Tiefe ohne Matt die ersten MultiPV Wurzelzüge, wie
andere Engines, damit die Liste in der GUI lückenlos bleibt:

```
info depth 9 multipv 1 score cp 0 nodes 7724 nps 234060 time 33 pv b5a6
info depth 9 multipv 2 score cp 0 nodes 7724 nps 234060 time 33 pv a1a2
info depth 9 multipv 3 score cp 0 nodes 7724 nps 234060 time 33 pv a1b2
```

**Die Wurzel treibt das UCI, nicht die Suche** (`runRoot` in `uci/search.go`, Rust
`run_root`): Jeder Wurzelzug wird einzeln gelöst, die Stellung nach dem Zug ist die
Wurzel der Suche, Tiefe für Tiefe mit d − 1 Halbzügen. Gerade Kindtiefe heißt "der
Gegner wird matt" (unser Gewinn), ungerade "der Gegner setzt matt" (unser Verlust). Weil
jeder Zug bei jeder Tiefe neu gefragt wird, bis er bewiesen ist, ist sein erster Beweis
auch sein kürzestes Matt, und die MultiPV-Liste ist exakt, so wie eine Alpha-Beta-Engine
für MultiPV jeden Wurzelzug komplett sucht. Eine Suche, die an der Wurzel beim ersten
Erfolg oder bei der ersten Flucht abbricht, lässt die anderen Züge mit Schranken zurück
(genau das war vorher der Fehler: `Kf7` in KRR-KN mit Schwarz am Zug stand bei `mateab`
auf `-M6`, bei `matepn` auf `-M5`, richtig ist `-M4`). Die Searcher bieten dafür den
Einzeltiefen-Aufruf (`mateab.SolveDepth`, `matepn.Solve`) und salzen ihre Keys nach
Angreiferfarbe statt nach Wurzelparität, damit die Kind-Suchen eine Tabelle teilen
(`blackAttackerSalt`; `matepn` codiert die Rolle über die Parität der Resttiefe und
salzt nur die Final-Einträge, `finalKey`). Ordnung der Liste: Gewinne nach kürzestem
Matt, offene Züge nach den Knoten ihrer letzten Tiefe (der am schwersten zu widerlegende
zuerst), Verluste nach längstem Matt. `cp 0` steht für "kein Matt bis zu dieser Tiefe",
eine Bewertung gibt es nicht; auch ein Zug, den die Tabellen als Verlust kennen, steht
im Suchblock als `cp 0` (der Tabellenblock davor hatte ihn richtig). Ende der Suche:
alle Züge bewiesen, bei MultiPV 1 der erste bewiesene Gewinn (kürzester), Tiefenschranke
oder `stop`. `matelist` braucht den Treiber nicht, sein Graph kennt jeden Wurzelzug
exakt. Die Experimente (`Solve`/`SolveShortest` mit Schalter `Mated`, Testbank-Buchstabe
`M`) bleiben die alte Wurzelsuche, damit die Knotenzahlen vergleichbar bleiben. Innerhalb einer langen Tiefe alle fünf Sekunden (`progressInterval`) `info depth
17 currmove e3h6 currmovenumber 10 nodes 2418594 nps 212941 time 11358` mit dem
Wurzelzug, der gerade untersucht wird, und seiner Nummer in der Zugliste (seit
2026-10-16, wie bei Stockfish; die Suche bietet die Zeile alle 65.536 Knoten an,
`ProgressEvery`;
die eigene Vorgabe der Searcher von 2^22 bzw. 2^26 wäre bei 200.000 Knoten pro Sekunde
minutenweit auseinander). Das gefundene Matt kommt als `info depth 11 score mate 6
nodes 7800 nps 236363 time 33 pv g3h4 f8g7 h7h8q ...`. `score mate N` gibt es erst mit dem Beweis:
`mateab` und `matelist` kennen die Distanz exakt, `matepn` liest sie aus dem
Beweisbaum in der Tabelle; fehlen dort Einträge (ersetzt), steht die Suchtiefe als
obere Schranke im Score und eine `info string` sagt es. Endet die Variante in einer
Tabellenstellung, hängt die Hauptschleife die optimale Linie aus den Tabellen an
(`extendPV`). Ohne Matt kommt eine Zeile `info string search: stopped, no mate within
13 plies, 505,026 nodes in 3.0 s` und `bestmove` ist der beste Tabellenzug (Remis vor
Verlust) oder der erste legale.

**Tabelle:** `matepn` bekommt die direkte Tabelle (Beweise dürfen nicht von größeren
Zahlen verdrängt werden), `mateab` Buckets ab 128 MB (der Typ in den hohen Bits hält
Matts über Widerlegungen), darunter ebenfalls die direkte. Die Tabelle lebt zwischen
den Suchen weiter (Final-Einträge von `matepn` beantworten die nächste Suche derselben
Stellung in Mikrosekunden), `ucinewgame` leert sie, eine Hintergrund-Erzeugung gibt
sie frei (der Generator braucht den Speicher), die nächste Suche legt sie neu an.

**Rust:** Der Such-Thread braucht den Tabellensatz als Orakel und bekommt ihn per
Move; `Engine.set` ist so lange `None`, das Ergebnis bringt Set und Tabelle zurück
(`SearchEnd`). Weil `std` keinen Select über zwei Kanäle hat, schickt der Thread das
Ergebnis in einen eigenen Kanal und meldet es mit `Event::Searched` im Hauptkanal;
`stop_search` wartet direkt auf dem Ergebniskanal. Ein nachträglich eingeschaltetes
`EgtbWriteCache` während einer Suche wird nach ihrer Rückkehr ausgeführt.

**Verifikation (Go und Rust zeilenidentisch bis auf `nps`/`time`, deterministisches
Skript ohne Zeitlimits):** Bauern-Test (12 Steine, `go mate 6`): `matepn` Matt in 6
nach 7.800 Knoten, `mateab` nach 216.429, `matelist` meldet das Budget (524.288
Stellungen bei 64 MB) an Ebene 7; KQQ-KN Matt in 2 mit `f2f6 h8g8 a2f7`, nachdem die
Tabellenstufe nur das bekannte Remis `a2f7` (Patt) hatte; KRRN-KN ohne Tabelle "no
mate within 5 plies"; KRR-KN aus der Cache-Datei sofort Matt in 7; KQ-KBN mit
`go movetime 3000` bricht nach 13 Halbzügen ab (496.834 Knoten bis Tiefe 13, Rust 1,4-mal
schneller). `go infinite` auf dem Bauern-Test: Matt gefunden, `isready` zwischendurch
beantwortet, `bestmove` erst bei `stop`.

**Das drohende Matt (2026-10-09, zweiter Nachtrag):** Die Suche beweist in den
ungeraden Tiefen "die Seite am Zug setzt matt" und in den geraden "die Seite am Zug
wird matt" (gerade Beweisziele: die Wurzel ist ein Verteidigerknoten, bei `mateab`
läuft `defend` an der Wurzel, bei `matepn` legt die Parität der Resttiefe die Rolle
fest). Ergebnis als negative Distanz (`MatePlies` < 0, `score mate -N`), die Liste
enthält dann die Verluste mit der längsten Verteidigung zuerst:

```
info depth 10 multipv 1 score mate -5 nodes 5260 nps 228695 time 23 pv f8g7 h7h8q g7h8 f7f8q h8h7 e6e7 h7g6 e7e8q g6h7 e8f7
info depth 10 multipv 2 score mate -4 nodes 5260 nps 228695 time 23 pv f8e7 h7h8q e7d6 h8d8 d6e6 f7f8q e6e5 d8d6
```

Ohne Ergebnis nimmt `bestmove` den besten offenen Zug statt des ersten legalen (eine
bekannte Tabellenantwort bleibt). Beide Fragen kosten zusammen etwa das Doppelte einer
reinen Mattsuche. **Benchmark-Stellung des Autors** (`test-positions.md`): KRR-KN mit
Schwarz am Zug, `8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1`, zwölf Züge, alle verlieren (Tabelle:
`Ng3 -M9`, `Ne3 -M8`, `Nd2 -M8`, `Nh2 -M7`, sieben Königszüge `-M6`, `Kf7 -M4`); andere
Engines mit sechs Kernen und MultiPV 12: AsmFish, Patricia und Slow Chess 1 s, Reckless
6 s, Stockfish 7 s, Spike 13 s, Dragon 94 s. Mattjes ohne Fünf-Steiner-Datei, ein
Thread: `matepn` alle zwölf Werte exakt nach 93 s (17,8 Mio. Besuche), `mateab` nach
122 s (414 Mio. Knoten); nach 40 bzw. 20 s standen neun davon (Tiefe 15), `Ne3`/`Nd2`
brauchen Tiefe 16, `Ng3` Tiefe 18. `matelist` antwortet erst nach der Rückwärtsphase
und müsste dafür den ganzen Fünf-Steiner aufzählen. Die anderen Engines hatten die
Varianten zu dem Zeitpunkt gefunden, nicht bewiesen; unsere Werte sind durch die
erreichte Tiefe garantiert.

**Erstes Profil (2026-10-09, `MATTJES_CPUPROFILE=<Datei>` beim Start der Go-Binary im
UCI-Modus, dann `go tool pprof -top runMattjesGo.exe <Datei>`):** 36 % der Suchzeit
waren die zwei zufälligen Tabellenzugriffe pro Kind (Final-Eintrag und gesalzener
Eintrag, jeder ein Cache-Miss hintereinander), 15 % die Orakel-Abfrage, davon fast
alles die Index-Rechnung für die registrierte, aber nicht erzeugte KRRKN-Tabelle (das
Ergebnis wurde weggeworfen), 7 % Allokationen, weil ein Brettzeiger, der an die
Orakel-Schnittstelle geht, in Go auf den Heap entkommt. Drei Mechanik-Korrekturen ohne
Änderung der Suche (Knotenzahlen und Ausgaben identisch): alle Kind-Keys zuerst
berechnen und ihre Slots per Prefetch anfordern, erst dann sondieren (`tt.Prefetch`,
Rust `_mm_prefetch`, Go ein Load in eine globale Senke), die Tabellensuche bricht vor
der Index-Rechnung ab, wenn die Tabelle keine Werte hat (`findTable`/`indexOf`), und
in Go bekommen Orakel und Rekursion feste Bretter im Searcher (`scratch`, `stack`).
Zeit bis Tiefe 15 (5.528.849 Besuche) auf dieser Stellung: Go 31,3 s → 25,8 s, Rust
23,1 s → 16,5 s (335.000 Besuche pro Sekunde); die komplette Lösung (Rust) `matepn`
93 s → 71 s, `mateab` 122 s → 111 s. Beim Test kamen zwei Altlasten hoch: Wechselt die Option `Search` zwischen
Algorithmen, muss die Transposition Table geleert werden (unter 128 MB nutzen `mateab`
und `matepn` dieselbe direkte Tabelle, und `matepn` las `mateab`-Einträge als
Beweiszahlen); und `boardSignature` lief bei vier gleichen Steinen über (K + 4 Bauern
gegen K sah aus wie KNK und ließ den Index-Code abstürzen), jetzt melden vier gleiche
Steine "keine Tabelle".

## Offen

- `matepn` beweist nur "Matt in ≤ N"; der vorgemerkte Schritt (Beweisziel "Gewinn":
  gewonnene Tabellenstellung = bewiesen, Ergebnis "Matt in höchstens k + DTM") macht
  KQ-KBN ohne Fünf-Steiner-Datei erst lösbar.
- MultiPV gilt nur für Tabellenantworten, die Suche liefert eine Variante.
- Fünfzig-Züge-Regel und Zugwiederholung kennt die Tabelle nicht; DTM-Linien über
  50 Züge (KBBKN, KNNKP) würden in einer Partie remis enden.
