# Milestone 5: Endspieltabellen (Entwurf)

Stand 2026-10-05. Entscheidung des Autors siehe `CLAUDE.md`, Milestone 5. Dieses Dokument
erklärt, wie die eigenen Vier-Steiner-Tabellen aufgebaut werden und warum so. Package-Name
`egtb` (endgame tablebase) ist ein Arbeitstitel.

## 1. Was eine Tabelle ist

Eine Endspieltabelle ist ein Array mit einem Byte pro Stellung. Der Array-Index **ist** die
Stellung: aus den Feldern der Steine und der Seite am Zug wird eine Zahl berechnet, und aus
der Zahl lassen sich die Felder zurückrechnen. Es gibt keinen Hash, keine Kollision, keine
Suche. Das Byte ist das Ergebnis aus Sicht der Seite am Zug:

| Wert     | Bedeutung                                   |
|----------|---------------------------------------------|
| 0        | Remis (oder während der Berechnung: unbekannt) |
| 1..127   | Seite am Zug setzt in n Halbzügen matt      |
| 128      | ungültige Stellung (Felder doppelt belegt, Gegner steht im Schach) |
| 129..255 | Seite am Zug wird in n = Wert − 129 Halbzügen mattgesetzt, 129 = steht matt |

DTM (distance to mate) in Halbzügen, damit ein Treffer in der Suche sofort den Beweis mit
Distanz liefert. Das längste Vier-Steiner-Matt (KQKR, 35 Züge = 69 Halbzüge) passt locker.

## 2. Indizierung

Eine Stellung mit vier Steinen wären naiv 64⁴ · 2 = 33 Mio. Indizes pro Material. Zwei
Dinge drücken das:

**Königs-Symmetrie.** Ohne Bauern ist das Brett achtfach symmetrisch (spiegeln an beiden
Achsen und an der Diagonale). Der weiße König wird in das Dreieck a1-d1-d4 (10 Felder)
gedreht, alle anderen Steine machen dieselbe Drehung mit. Steht er auf der Diagonale, bleibt
die Transposition offen; dann entscheidet der schwarze König (unterhalb oder auf der
Diagonale). Von den 10 · 64 Königspaaren bleiben nach Abzug benachbarter Könige **462**
(Standardzahl, der Generator prüft sie beim Start). Stehen beide Könige auf der Diagonale,
legen die Könige die Transposition nicht fest; dann ist der kleinere der beiden möglichen
Indizes der kanonische, der andere ein toter Index (wie bei zwei gleichen Steinen, siehe
unten). Die erste Fassung hatte beide Zwillinge als "Redundanz, kein Fehler" behalten; für
die Kandidaten-Methode in Abschnitt 3 war es doch ein Fehler, siehe `m5-endgame-tables.md`.

**Gleiche Steine** (KQQK, KBBK, KPPK): Beide Reihenfolgen der Steine ergeben dieselbe
Stellung. Der Index sortiert gleiche Steine nach Feld, die Indizes mit der anderen Reihenfolge
sind tot und werden als ungültig markiert (Index(Decode(i)) ≠ i).

**Mit Bauern** gibt es nur die Links-rechts-Spiegelung (Bauern laufen in eine Richtung).
Der weiße König steht auf den Linien a-d (32 Felder), **1806** legale Königspaare. Bauern
stehen nur auf den Reihen 2 bis 7: 48 Felder statt 64.

Index = ((Seite · KK + Königspaar) · 64 + Stein 1) · 64 + Stein 2, bei Bauern 48 statt 64.
Doppelt belegte Felder sind einfach ungültige Indizes (Wert 128).

| Material           | Indizes    | Anzahl Materialien | Summe   |
|--------------------|------------|--------------------|---------|
| K+X vs K           | 59.136     | 4                  | 0,2 MB  |
| K+X+Y vs K, KX vs KY | 3.784.704 | 20                 | 75,7 MB |
| KPK                | 173.376    | 1                  | 0,2 MB  |
| ein Bauer, 4 Steine | 11.096.064 | 8                 | 88,8 MB |
| KPPK, KPKP         | 8.322.048  | 2                  | 16,6 MB |
| **gesamt**         |            | **35**             | **≈ 182 MB** |

(Alle Materialien bis vier Steine sind 35, nicht 30 wie zuerst geschätzt: 5 Dreisteiner,
15 mit zwei Steinen auf einer Seite, 15 mit je einem Stein pro Seite.)

**Farbwechsel.** Jede Tabelle wird für eine feste Zuordnung gespeichert, Weiß ist die
Seite mit mehr Material (KQKR, nicht KRKQ). Steht das Material auf dem Brett andersherum,
wird vor dem Nachschlagen das Brett vertikal gespiegelt, die Farben getauscht und die Seite
am Zug gewechselt. Der Wert gilt dann unverändert, denn er ist aus Sicht der Seite am Zug.

## 3. Berechnung: Retrograde als Ebenen-Iteration

Das Standardverfahren (Thompson 1986) ist die Rückwärtsanalyse: von den Mattstellungen aus
werden über **Rückwärtszüge** die Vorgänger gefunden. Das braucht einen Rückwärtszug-
Generator, der nirgends sonst verwendet und nur durch den Vorwärts-Generator geprüft wird.

Wir fangen mit der einfacheren Variante an, die **nur den geprüften Vorwärts-Generator**
benutzt und dieselben exakten Distanzen liefert, die Ebenen-Iteration:

1. **Init:** jede Stellung dekodieren, Legalität prüfen (Gegner nicht im Schach), Züge
   zählen. Keine Züge und im Schach = matt = Verlust in 0 (Wert 129). Keine Züge ohne
   Schach = Patt, bleibt 0 und wird nie mehr angefasst.
2. **Ebene n = 1, 2, 3, ...:** alle noch unbekannten Stellungen durchgehen.
   - n ungerade: hat ein Kind den Status "Verlust" (egal welche Distanz), gewinnt die
     Stellung in n. Die Distanz muss genau n − 1 sein, sonst hätte eine frühere Ebene die
     Stellung schon gefunden; der Generator prüft das.
   - n gerade: sind **alle** Kinder "Gewinn" für den Gegner, verliert die Stellung in n.
     Ein unbekanntes oder Remis-Kind (beides 0) blockiert für immer, denn die Seite am Zug
     kann dorthin ausweichen.
3. Ändert eine Ebene nichts mehr, ist alles Übrige Remis (bleibt 0).

Korrektheit hängt an einer Invariante: wenn Ebene n läuft, sind alle Werte < n endgültig.
Deshalb reicht in Schritt 2 der Status des Kindes, die Distanz ergibt sich. Dieselbe
Invariante macht die Iteration **trivial parallel**: ungerade Ebenen schreiben nur Gewinne
und lesen nur Verluste, gerade Ebenen umgekehrt. Kein Worker liest, was ein anderer in
derselben Ebene schreibt.

Preis: jede Ebene läuft über alle unbekannten Stellungen, auch die späteren Remis. Für
KQKR sind das rund 70 Ebenen über ein paar Millionen Stellungen; Remis-lastige Tabellen wie
KRKR scannen in jeder Ebene fast alles (gemessen: über 100 s). Deshalb die Standard-
Beschleunigung: **Kandidaten**. Eine Stellung kann sich in Ebene n nur ändern, wenn eines
ihrer Kinder in Ebene n − 1 entschieden wurde. Wer in n − 1 entschieden wird, markiert per
**Rückwärtszug** seine Eltern (ein Stein der Seite, die gerade gezogen hat, zurück auf ein
leeres Feld, von dem er gekommen sein kann; Schlag- und Umwandlungs-Rückzüge gibt es
innerhalb einer Tabelle nicht). Ebene n bewertet nur die Markierten, und zwar unverändert
vorwärts. Der Rückwärtszug ist damit nur ein Filter: Fehlt einer, wird eine Stellung
übersehen, aber keine falsch bewertet. Übersehene findet die Vorwärts-Verifikation (`Verify`,
jede Stellung aus ihren Kindern nachgerechnet, eine volle Ebene ohne Kandidaten).

Zwei Sonderfälle: Stellungen, deren Entscheidung nur von Kindern in **kleineren Tabellen**
abhängt (Schlag ins KQK mit Verlust in 19), werden von keinem Rückwärtszug angestoßen; sie
werden bei der Initialisierung erkannt und für ihre Ebene vorgemerkt (`pending`). Und beim
En-passant-Doppelschritt liegt das Kind außerhalb der Tabelle; die Elternstellung wird
Kandidat, sobald die Stellung nach dem Doppelschritt (ohne EP-Recht) Kandidat ist, denn
beide haben dieselben Kinder.

**Kinder in anderen Tabellen.** Schlagzüge führen in Dreisteiner-Tabellen (oder zu K
gegen K = Remis), Umwandlungen in andere Viersteiner. Dafür müssen die kleineren Tabellen
vorher fertig sein: Reihenfolge nach (Bauern, Steine): erst bauernlos 3, bauernlos 4, dann
KPK, dann ein Bauer, dann zwei Bauern. Jede Abhängigkeit hat weniger Bauern oder weniger
Steine. Kinder innerhalb der Tabelle brauchen kein `DoMove`: Feld des gezogenen Steins
ersetzen, Index rechnen.

**En passant.** Unter den 35 Materialien hat nur KPKP Bauern beider Farben, also nur dort
kann es En-passant geben. Eine Stellung mit EP-Recht steht nicht in der Tabelle (Entscheid:
die Suche spielt einen Halbzug weiter). Im Generator ist der Doppelschritt, der ein EP-Recht
erzeugt, deshalb ein Zug zu einem Kind außerhalb der Tabelle; sein Wert wird aus **seinen**
Kindern bestimmt (Tabellenstellungen ohne EP oder, nach dem EP-Schlag, KPK). Dank der
Invariante ist dieser Wert endgültig, sobald er sich aus bekannten Enkeln ergibt.

## 4. Verifikation

Nur gegen Bekanntes oder Eigenes:

- Königspaare 462 und 1806, Index-Roundtrip über alle Indizes (dekodieren, kodieren).
- Längste Matts in Zügen gegen die Literatur: KQK 10, KRK 16, KBBK 19, KBNK 33, KQKR 35,
  KQKQ 13, KRKR 19, KQKN 21, KQKB 17, KRKB 29, KRKN 27.
- Die Teststellungen aus `chess/matedata.go` bis vier Steine müssen genau 2N − 1 Halbzüge
  liefern (Matt in 3, 5, 7, 12, 15, 17, 31), dasselbe Ergebnis wie `mateab` und `matelist`.
- Prüfsumme über jede Tabelle als Konstante im Code: Regressionstest des Generators und
  zugleich Gültigkeitsprüfung der Cache-Datei.

## 5. Cache-Datei

Eine Datei neben der Binary, die Tabellen in fester Material-Reihenfolge hintereinander,
ohne Header. Laden: Größe prüfen, Prüfsumme gegen die Konstante, fertig. Passt etwas nicht,
wird gerechnet, geprüft und geschrieben.

## 6. Umsetzungsschritte

Alle sieben am 2026-10-05 umgesetzt, Ergebnisse in `m5-endgame-tables.md`.

1. `bitboard`: `FromPieces` (Brett aus Einzelsteinen ohne Rochade/EP), `OpponentInCheck`,
   `PieceAttacks` (für die Rückwärtszüge). ✅
2. `egtb`: Materialien, Königspaar-Tabellen, Index kodieren/dekodieren, Roundtrip-Test. ✅
3. Generator (Ebenen-Iteration mit Kandidaten, parallel), Maxima prüfen. ✅
4. Bauern: Umwandlungen, KPKP mit En passant. ✅
5. Cache-Datei mit Prüfsummen-Konstanten. ✅
6. Orakel-Adapter für `mateab` (`Oracle`-Interface), Distanz beendet den Knoten. ✅
7. Rust-Port, Tabellen byteidentisch (gleiche Prüfsummen). ✅
