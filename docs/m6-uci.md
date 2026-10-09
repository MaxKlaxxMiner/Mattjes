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
`position startpos|fen ... [moves ...]`, `go` (alle Parameter ignoriert, die Antwort
kommt sofort), `stop` (tut nichts, es läuft nichts im Hintergrund), `quit`, dazu `d`
zum Ausgeben der FEN.

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

`depth` ist die Länge der Variante in Halbzügen, `score mate N` die Zugzahl aus DTM,
negativ aus Sicht des Verlierers. Remis gibt `score cp 0` ohne Variante.

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

Ablauf: Eine Erzeugung startet **nur bei `go infinite`**, wenn die Tabelle des
Wurzelmaterials fehlt und die Option es erlaubt. Die Engine antwortet zuerst wie bisher
(ohne Tabelle: erster legaler Zug), hält `bestmove` wie bei `go infinite` üblich zurück
und meldet `info string egtb: generating KBBBK (231 MB table, about 346 MB RAM), stop
pauses it`. Die Erzeugung läuft in einem eigenen Thread auf einer Kopie der geladenen
Tabellen (`Set.Fork`, 173 MB für die Basis), damit die Hauptschleife jederzeit `isready`
beantwortet und weitere Kommandos liest. `stop` schickt das zurückgehaltene `bestmove`
und **pausiert** die Erzeugung: Die Worker halten an der nächsten Chunk-Grenze an
(Millisekunden), alles Gerechnete bleibt im RAM. Das nächste `go infinite` mit demselben
Material setzt genau dort fort; ein anderes Material verwirft den pausierten Job. Ist die
Tabelle fertig, übernimmt die Hauptschleife sie (`Set.Adopt`), schreibt sie bei
`EgtbWriteCache` in den Cache und rechnet bei laufendem `go infinite` die Antwort neu:

```
info string egtb: generating KBBBK (231 MB table, about 346 MB RAM), stop pauses it
info string egtb: generation of KBBBK paused, the next go infinite resumes it
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

Vorgemerkt: Während einer Erzeugung sollen Hash und reguläre Suche ihren RAM komplett
freigeben; dazu statische Anforderungsdaten je Material (RAM, Platte, Zeit) im Code, aus
denen die Engine vor dem Start warnt, wenn der Rechner das nicht schafft.

## Offen

- Stufe c: Suche für Stellungen ohne Tabellenwert als UCI-Option (`mateab`,
  `matelist`, `matepn`), Zeitkontrolle (`go movetime`, `wtime`), `stop` aus einem
  laufenden Thread heraus, `info nodes nps` mit Fortschritt.
- Ohne Tabelle wird der erste legale Zug gespielt, das ist keine Engine, nur ein
  Platzhalter. Schlagzüge in eine bekannte Tabelle hinein werden schon jetzt richtig
  bewertet, weil die Kinder einzeln nachgeschlagen werden.
- Fünfzig-Züge-Regel und Zugwiederholung kennt die Tabelle nicht; DTM-Linien über
  50 Züge (KBBKN, KNNKP) würden in einer Partie remis enden.
