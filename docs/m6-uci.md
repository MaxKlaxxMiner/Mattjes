# Milestone 6: UCI

Vorgezogen am 2026-10-08, damit Mattjes gegen echte Schachprogramme getestet werden
kann, bevor die Suchalgorithmen darunter liegen. Die Richtung des Autors (fünf Stufen
nach wachsender Gewissheit) steht in `CLAUDE.md`; hier geht es um das, was läuft.

## Stufe a: Antworten aus den Tabellen (2026-10-08)

Package `uci/` in Go, Modul `src/uci/` in Rust. **Die blanke Binary ist die Engine**,
wie jede GUI es erwartet: `runMattjesGo.exe` oder `runMattjesRs.exe` ohne Argument
spricht UCI auf stdin/stdout. Argumente wählen Tests und Aktionen (`test` für den
Experiment-Block aus `main`, `egtb-list`, `egtb-measure`, `egtb-compress`). Threads
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

## Offen

- Stufe b: Fünf-Steiner-Tabelle im Hintergrund erzeugen, wenn die Datei fehlt
  (`info string` mit Fortschritt, bis dahin ohne Tabelle antworten). Eine Minute und
  bis zu 6 GB für ein Bauern-Material, also nur im Analyse-Modus sinnvoll.
- Stufe c: Suche für Stellungen ohne Tabellenwert als UCI-Option (`mateab`,
  `matelist`, `matepn`), Zeitkontrolle (`go movetime`, `wtime`), `stop` aus einem
  laufenden Thread heraus, `info nodes nps` mit Fortschritt.
- Ohne Tabelle wird der erste legale Zug gespielt, das ist keine Engine, nur ein
  Platzhalter. Schlagzüge in eine bekannte Tabelle hinein werden schon jetzt richtig
  bewertet, weil die Kinder einzeln nachgeschlagen werden.
- Fünfzig-Züge-Regel und Zugwiederholung kennt die Tabelle nicht; DTM-Linien über
  50 Züge (KBBKN, KNNKP) würden in einer Partie remis enden.
