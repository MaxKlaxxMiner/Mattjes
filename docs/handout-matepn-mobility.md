# Handout: Mobilität in `matepn` messen und entscheiden

Stand 2026-10-09, Arbeitsrechner. Nächster Schritt an der df-pn-Suche, als
Arbeitsanleitung für eine Sitzung an einem anderen Rechner. Alles Nötige steht hier
oder in den verlinkten Dateien; nichts davon ist angefangen.

## Ausgangslage

- Benchmark-Stellung (`test-positions.md`, Abschnitt "Benchmark"): KRR-KN mit Schwarz
  am Zug, `8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1`, zwölf Züge, alle verlieren, Werte aus
  der Fünf-Steiner-Tabelle bekannt. Messung immer **ohne** `5-KRRKN.bin` im Cache
  (sonst antwortet die Tabellenstufe sofort), ein Thread, `Hash 256`, `MultiPV 12`.
- Zwei Messgrößen:
  1. **Mechanik:** Zeit bis Tiefe 15 bei festen 5.528.849 Besuchen (`matepn`). Rust
     16,5 s, Go 25,8 s. Ändert sich die Besuchszahl, war es keine Mechanik.
  2. **Algorithmus:** Zeit und Besuche bis alle zwölf Werte exakt stehen. Rust
     `matepn` 71 s mit 17,8 Mio. Besuchen, `mateab` 111 s mit 414 Mio. Knoten.
- Profil nach den Mechanik-Korrekturen (Go, `MATTJES_CPUPROFILE`, Abschnitt "Erstes
  Profil" in `docs/m6-uci.md`): Tabelle rund 40 %, **Mobilität neuer Blätter 37 %**
  (`GenMoves` 17 %, `GenChecks` 12 %, Orakel 10 %), `mid` selbst 6 %.

## Die Frage

`leafNumbers` (Go `matepn/search.go`, Rust `src/matepn/mod.rs`) initialisiert jedes
Kind ohne Tabelleneintrag mit seiner Zuganzahl statt mit 1/1: Verteidiger pn = Züge,
Angreifer dn = Züge. Dafür ruft es `terminal`, also Orakel und kompletten
Zuggenerator, für jedes frische Blatt, und davon gibt es sechsmal so viele wie
Besuche (Bauern-Test: 47.644 Blätter auf 7.933 Besuche). Beim Einbau (M4, Schritt 4,
`docs/m4-mate-search.md`) hat die Mobilität die Besuche auf den Vier-Steinern
deutlich gesenkt, gemessen wurde aber nie, ob sie die Zeit senkt, und nie auf einer
Stellung, auf der die Tabellen nicht sofort greifen.

Drei Varianten sind zu vergleichen:

| Variante | Blatt-Initialisierung | Kosten pro Blatt |
|---|---|---|
| A: Mobilität (heute) | pn/dn = Zuganzahl, Terminalprüfung inklusive | Orakel + Generator |
| B: ohne | 1/1, keine Terminalprüfung am Blatt | nichts |
| C: faul | 1/1 beim Expandieren; die Zuganzahl erst, wenn das Kind zum besten Kind wird und `mid` es ohnehin betritt (dort wird der Generator sowieso gerufen) | nichts beim Expandieren |

Variante C ist die interessante: `mid` erzeugt die Züge des Knotens, den es betritt,
und könnte die Zahl an den Elternknoten zurückgeben, ohne dass ein zweiter Aufruf
nötig ist. Das ändert die Reihenfolge, in der Geschwister ausgewählt werden (alle
frischen Kinder sehen zunächst gleich aus), also die Besuchszahl.

## Umsetzung

1. **UCI-Option** `MatePnMobility` (Combo `on`, `off`, `lazy`, Vorgabe `on`) in
   `uci/uci.go` (`setOption`, `uci`-Antwort) und `src/uci/mod.rs`; in
   `uci/search.go` `startSearch` an `matepn.New(oracle, table, codec, mobility)`
   durchreichen, für `lazy` ein neues Feld `Searcher.LazyMobility`. Go und Rust
   zeilenidentisch halten (gleiche Option, gleiche Werte).
2. **Testbank:** Buchstabe `m` schaltet Mobilität heute ein; einen Buchstaben `l`
   für faul ergänzen (`tests_pn.go` `runMatepn`, `tests_pn.rs` `run`), Kopfzeile
   ausgeben.
3. **Variante C im Searcher:** `mid` liefert neben pn/dn die Zuganzahl des
   betretenen Knotens zurück (oder schreibt sie in ein Feld), der Elternknoten
   setzt beim ersten Rückweg `c.pn`/`c.dn` daraus, falls das Kind noch frisch war
   (Markierung im `child`-Eintrag). Terminale Kinder (Matt, Patt, Orakel) erkennt
   `mid` beim Betreten selbst, dafür braucht das Blatt keine Vorprüfung.
4. **Nichts anderes anfassen**, damit die Messung sauber bleibt: kein Prefetch
   ändern, keine Schwellen (`Epsilon`), keine Final-Einträge.

## Messprotokoll

- Beide Binaries bauen (`./build.sh`), Clippy, `go vet`, `gofmt` sauber.
- Testbank: `matepnSolve(15, 256, "sat", "mEf")` und mit `l` statt `m` sowie ohne
  beides, Go und Rust müssen je Variante **besuchsgenau** gleich zählen (die
  Regressionszahlen für `mEf` stehen in `docs/m4-mate-search.md`: KQ-KN 181.065
  Besuche, KR-KR 1.125.947; `mEfi` KQ-KN 7,9 Mio.).
- Benchmark in Arena oder per Treiber-Skript (Muster in der Memory-Notiz
  "UCI test driver": `coproc`, auf `bestmove` warten): je Variante `go movetime
  120000`, notieren: Zeit und Besuche bis Tiefe 15, Zeit und Besuche bis alle zwölf
  Werte exakt sind, oder welche nach 120 s noch offen sind. Rust zählt, Go nur zur
  Kontrolle der Gleichheit.
- Zweitstellung als Gegenprobe, damit nicht auf eine Stellung optimiert wird: der
  Bauern-Test (`5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1`, Matt in 6, 7.800
  Besuche mit Mobilität) und KQ-KBN ohne Fünf-Steiner-Datei mit `go movetime 60000`
  (Tiefe und Besuche nach 60 s vergleichen; die Stellung ist in 60 s nicht lösbar,
  Fortschritt ist die Tiefe).
- Ergebnis in `docs/m4-mate-search.md` als neuen Abschnitt eintragen (Tabelle mit
  Variante, Besuche, Zeit je Stellung), Vorgabe der Option auf die beste Variante
  setzen, `CLAUDE.md` nachziehen.

## Erwartung und Fallen

- Variante B spart die 37 % Blattkosten komplett, wird aber mehr Besuche brauchen;
  bei 30 Kindern pro Knoten und sechs Blättern pro Besuch könnte der Nettoeffekt
  trotzdem positiv sein, weil ein Besuch mit allen Sondierungen rund 3 µs kostet und
  ein Blatt 1,5 µs.
- Variante C sollte die Blattkosten sparen und die Besuche nahe an A halten; die
  Gefahr sind Mehrfachbesuche, weil frische Geschwister ununterscheidbar sind und
  df-pn dann in Generierungsreihenfolge absteigt.
- Die Besuchszahlen in `docs/m4-mate-search.md` gelten nur für `mEf` mit Mobilität;
  neue Varianten bekommen eigene Zeilen, alte Zahlen bleiben stehen.
- Ohne Mobilität entfällt in `leafNumbers` auch die Terminalprüfung des Blattes;
  `terminal` wird dann erst beim Betreten gerufen. Das ist korrekt (ein Mattkind wird
  beim Betreten als bewiesen erkannt), kostet aber einen Besuch pro terminalem Kind.

## Danach (nicht Teil dieses Schritts)

- `mateab` hat noch keinen Prefetch: ein Zugriff pro Knoten direkt nach `DoMove`;
  hilft nur, wenn der Kind-Key vor der Zugsortierung des Elternknotens angefordert
  wird (`orderAttack` mit `GivesCheck` für jeden Zug ist dort selbst teuer, siehe
  Antwort vom 2026-10-09 zu Spike).
- Final-Eintrag und gesalzener Eintrag eines Kindes in eine Cache-Line legen (ein
  Miss statt zwei), zum Beispiel Buckets mit beiden Einträgen unter dem unsalzenen
  Key.
- Signatur des Orakels (6 %) inkrementell im Brett führen statt zehn Popcounts pro
  Abfrage.
- Beweisziel "Gewinn" (gewonnene Tabellenstellung = bewiesen), der eigentliche
  vorgemerkte `matepn`-Schritt aus `CLAUDE.md`, davon unabhängig.
