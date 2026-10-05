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

## Schritt 3: `mateab` mit Transposition Table (2026-10-05)

Die Tabelle aus Milestone 3 (`tt.Table` oder `tt.Buckets`, 256 MB) hängt über
ein kleines Interface an der Suche. Was gespeichert wird, steht in
`mateab/ttvalue.go`:

| Knoten | Typ 2 (Matt) | Typ 1 (kein Matt) |
|---|---|---|
| Angreifer | Matt in d Halbzügen, exakt, gilt für jede Resttiefe ≥ d; Zug = Mattzug | kein Matt innerhalb d, gilt für jede Resttiefe ≤ d |
| Verteidiger | wird in d gemattet; Zug = längste Verteidigung | Flucht existiert innerhalb d; Zug = der Fluchtzug |

Value = Typ (2 Bit) | Tiefe (7 Bit) | Von (6) | Nach (6) = 21 Bit, passt in
beide Layouts bei 256 MB. Der Zug trägt keine Umwandlungsfigur (Dame wird
angenommen), er dient nur der Sortierung. **Kein Tiefensalz im Key**, anders
als bei Perft: Ein bewiesenes Matt ist tiefenunabhängig, eine Widerlegung
gilt bis zur gespeicherten Tiefe. Bei einem Treffer, der den Knoten beendet,
bricht die Hauptvariante der Dreieckstabelle ab; `extendPV` setzt sie nach der
Suche über die TT-Züge fort (Mattzug an Angreifer-, längste Verteidigung an
Verteidigerknoten), bis zum Matt oder bis ein Eintrag fehlt ("..." in der
Ausgabe).

Alle Teststellungen bis Matt in 17, direkte Tabelle 256 MB, Tabelle pro
Stellung geleert (Arbeitsrechner; Knoten und Zähler in Go und Rust identisch):

| Stellung | Matt in | Knoten ohne TT | Knoten mit TT | Go ohne | Go mit | Rust mit | Treffer, die den Knoten beenden | ersetzt | Füllgrad |
|---|---|---|---|---|---|---|---|---|---|
| KQQ-K | 3 | 1.966 | 1.810 | 0 ms | 0 ms | 0 ms | 14 | 0 | 0,0 % |
| KQR-K | 5 | 1.169.380 | 285.159 | 129 ms | 47 ms | 34 ms | 39.293 | 83 | 0,3 % |
| KRR-K | 7 | 106.775.538 | 2.440.949 | 17,9 s | 0,62 s | 0,40 s | 1.297.138 | 10.765 | 2,6 % |
| KQ-KN | 12 | | 82.841.381 | | 20,8 s | 13,5 s | 56.380.522 | 2.192.652 | 28,2 % |
| KR-KR | 15 | | 20.179.903 | | 5,0 s | 3,1 s | 16.096.707 | 92.443 | 4,7 % |
| KBB-K | 17 | | 446.543.152 | | 119,1 s | 81,5 s | 369.430.716 | 16.397.933 | 26,7 % |
| Bauern | 6 | 464.248 | 216.357 | 99 ms | 54 ms | 36 ms | 36.971 | 216 | 0,4 % |
| gesamt | | | 552.508.711 | | 145,7 s | 98,5 s | | | |

Alle Mattlängen exakt bei 2N-1 Halbzügen, Rust 1,5-mal schneller als Go. Lesart:

- **Faktor 30 bis 44** bei KRR-K (Knoten 44-fach, Zeit 29-fach). Der Gewinn
  kommt fast vollständig aus der Widerlegungsphase: Tiefe 11 fällt von 81,7
  Mio. auf 1,9 Mio. Knoten. Transpositionen in Figurenendspielen sind massiv,
  dieselbe Stellung entsteht über unzählige Zugfolgen.
- **Trefferquote 80 bis 85 %** der Probes beenden den Knoten. Die Suche
  besteht zum größten Teil aus Wiedersehen.
- **KQ-KN und KBB-K** füllen die Tabelle zu über einem Viertel und ersetzen
  Millionen Einträge, obwohl 16 Mio. Slots da sind. Die Hauptvariante von
  KQ-KN ist deshalb abgeschnitten (Eintrag unterwegs ersetzt). Beide sind
  Stellungen mit langen stillen Manövern, in denen fast jeder Zug legal und
  nicht widerlegbar ist, bis die Tiefe ausgeht. Hier wird Alterung oder eine
  klügere Ersetzung (Typ Matt behalten) nötig, und hier beginnt der Fall für
  df-pn.
- **KR-KR** (Matt in 15) ist mit 5 s billiger als KQ-KN (Matt in 12): Das
  Gegenspiel des Verteidigers (eigener Turm) erzwingt Abtausch oder
  Turmverlust, der Baum ist schmal.

Beide Layouts (direkt und 4-Wege-Buckets) liefern dieselben Mattlängen.
Buckets bis Matt in 7: 2 statt 10.765 Ersetzungen bei KRR-K, Knotenzahlen
innerhalb von 1 % gleich, Zeit identisch. Ob die Buckets bei den großen
Stellungen (28 % Füllgrad) den Unterschied machen, ist noch nicht gemessen.

### KBN-K, Matt in 31, mit 1-GB-Bucket-Tabelle

Vom Autor auf dem Arbeitsrechner gestartet (`mateabSolveNamed("KBN-K", 1024,
true)`), Go und Rust knotengenau gleich:

| | Go | Rust |
|---|---|---|
| Knoten gesamt (31 Iterationen) | 1.411.582.185 | 1.411.582.185 |
| Zeit | 363,9 s | 300,2 s |
| TT-Treffer, die den Knoten beenden | 1.205.568.301 (85 %) | gleich |
| Stores (= echte Expansionen) | 192.200.797 | gleich |
| Ersetzungen | 164.863 | gleich |
| Füllgrad | 14,1 % = 9,5 Mio. Einträge | gleich |

Der Füllgrad passt zur Schätzung von rund 13 Mio. erreichbaren Stellungen
(wK 64 × sK 64 × L 32 Felder einer Farbe × S 64 × 2 Seiten, minus illegale):
Die Tabelle hält das ganze Endspiel, 67 Mio. Plätze reichen mit Reserve. Der
Aufwand pro Iteration wächst nicht mehr exponentiell, sondern bleibt ab Tiefe
37 flach bei 70 bis 120 Mio. Knoten, weil jede Iteration denselben
Stellungsraum noch einmal durchläuft.

**Die Zerlegung zeigt die Schwäche des Verfahrens:** 192 Mio. echte
Expansionen in 31 Iterationen sind rund 6 Mio. pro Iteration, weniger als die
Hälfte der erreichbaren Stellungen. Verschwendet wird der Faktor 31 der
iterativen Vertiefung. Ursache ist die Semantik der Widerlegungen: "kein Matt
innerhalb d" verfällt, sobald mit d+2 gesucht wird, und in einem Endspiel ohne
Matt in Reichweite sind fast alle Knoten Widerlegungen. Beweise dagegen
bleiben stehen; sie machen die 85 % Sofort-Treffer aus.

Konsequenzen:

- Für kleines Material ist die **Retrograde-Analyse** (Milestone 5) das
  richtige Werkzeug: jede der 13 Mio. Stellungen genau einmal, und DTM für
  alle Stellungen des Materials fällt mit ab. Zwei Größenordnungen weniger
  Arbeit als diese Suche.
- Für großes Material braucht die Vorwärtssuche ein Verfahren, dessen Wissen
  nicht verfällt: **df-pn** iteriert über Beweiszahlen statt über Tiefen.
  Dafür liefert es kein kürzestes Matt, deshalb die Zweistufigkeit.
- Die **Hauptvariante bricht ab**, obwohl nur 1,7 % der Einträge ersetzt
  wurden: Bei 61 Gliedern fehlt mit über 50 % Wahrscheinlichkeit eines. Die
  Ersetzung muss Beweise schonen, oder die Variante kommt aus der Suche selbst.

### Offen nach Schritt 3

- Ersetzung nach Typ (Beweise nie durch Widerlegungen verdrängen) oder
  Alterung; danach sollte die Hauptvariante bei KBN-K vollständig sein.
- Buckets gegen direkt bei KQ-KN und KBB-K mit 256 MB: Die direkte Tabelle
  ersetzte dort bei 27 % Füllgrad jeden vierten neuen Eintrag, Buckets sollten
  das fast vollständig vermeiden.
- KQ-KBN (fünf Steine, Matt in 39, über 500 Mio. Stellungen) ist mit dieser
  Suche nicht sinnvoll; das ist der Fall für df-pn.

## `matelist`: listenbasierte Suche auf dem endlichen Graphen (2026-10-05)

Der KBN-K-Lauf hatte das Grundproblem der Tiefensuche gezeigt: 192 Mio.
Expansionen für 13 Mio. Stellungen, weil die iterative Vertiefung denselben
Raum pro Tiefe neu durchläuft. Der Autor wollte deshalb zuerst den Algorithmus,
der bei endlichem Stellungsraum jede Stellung nur einmal anfasst, und erst
danach die Tabellen aus Milestone 5. Das ist der dritte Ansatz aus dem Entwurf
(Abschnitt 3.3), vorgezogen vor df-pn. Package `matelist`, Go und Rust.

Ablauf in drei Phasen:

1. **Vorwärts, Breitensuche.** Alle vom Start erreichbaren Stellungen werden
   Ebene für Ebene aufgezählt und über einen `ttstore` (Key → Index)
   dedupliziert: Jede Stellung wird genau einmal expandiert, egal über wie
   viele Zugfolgen sie erreichbar ist. Dabei werden pro Stellung der 32-Byte-
   Satz, die Zahl der legalen Züge und die Kind-Indizes (4 Byte pro Kante)
   gespeichert. Stellungen ohne Züge im Schach sind die Mattstellungen, die
   Startmenge der Rückwärtsphase. Ein Horizont in Halbzügen und eine
   Obergrenze an Stellungen begrenzen die Aufzählung; die Horizont-Ebene wird
   nicht expandiert, Beweise, die sie bräuchten, bleiben offen (sicher).
2. **Elternlisten** per Zähl-Sortierung aus den Kind-Kanten (CSR-Layout),
   ohne Generator und ohne Hash: eine Sekunde für 160 Mio. Kanten. Eine erste
   Fassung hatte die Züge dafür ein zweites Mal erzeugt und die Kinder
   nachgeschlagen, das kostete so viel wie die ganze Aufzählung.
3. **Rückwärts, Retrograde-Analyse.** Status pro Stellung: unbekannt, "Seite
   am Zug gewinnt in n" oder "verliert in n". Ebene 0 sind die Mattstellungen
   (verliert in 0). Ebene n → n+1: Jeder noch unbekannte Elternknoten eines
   "verliert in n" gewinnt in n+1. Jeder Elternknoten eines "gewinnt in n"
   bekommt seinen Zähler offener Kinder um eins verringert; erreicht er null,
   verliert er in n+1 (alle Kinder gewonnen, das längste zuletzt, deshalb ist
   n+1 automatisch das Maximum). Jede Kante wird genau einmal angefasst, nichts
   verfällt, nichts wird wiederholt. Patt und Stellungen mit nicht
   aufgelösten Kindern bleiben unbekannt, also "kein erzwungenes Matt".

Das Ergebnis ist die **exakte Mattdistanz für jede erreichbare Stellung**, die
Hauptvariante folgt den Distanzen von der Wurzel (Gewinner wählt ein Kind mit
"verliert in n-1", Verlierer eines mit "gewinnt in n-1") und ist immer
vollständig.

Alle Vier-Steiner der Teststellungen, Horizont 200, Grenze 30 Mio. Stellungen
(Arbeitsrechner, Go, vom Autor in der Konsole gestartet):

| Stellung | Matt in | Stellungen | Kanten | Vorwärts (Go) | Eltern | Retrograde | gesamt Go | gesamt Rust | `mateab` mit TT (Go) | aufgelöst | längstes Matt im Graphen |
|---|---|---|---|---|---|---|---|---|---|---|---|
| KQQ-K | 3 | 9.194.248 | 154 Mio. | 16,7 s | (1 s) | 1,3 s | ≈ 19 s | | 0 ms | 98 % | 20 Halbzüge |
| KQR-K | 5 | 20.391.772 | 320 Mio. | 38,2 s | (2 s) | 3,0 s | ≈ 43 s | | 47 ms | 99 % | 32 |
| KRR-K | 7 | 11.079.632 | 159 Mio. | 18,8 s | 1,0 s | 1,4 s | 21,1 s | 21,5 s | 0,62 s | 99,6 % | 32 |
| KBN-K | 31 | 12.814.320 | 142 Mio. | 18,0 s | 0,6 s | 1,5 s | 20,1 s | 22,0 s | 364 s | 85 % | 66 |
| KBB-K | 17 | 6.352.868 | 76 Mio. | 8,8 s | 0,3 s | 0,7 s | 9,8 s | 10,5 s | 119 s | 83 % | 38 |
| KQ-KN | 12 | 22.292.508 | 323 Mio. | 36,0 s | 1,4 s | 4,3 s | 41,6 s | 42,3 s | 20,8 s | 87 % | 42 |
| KR-KR | 15 | 22.341.512 | 331 Mio. | 43,4 s | 2,2 s | 1,5 s | 47,1 s | 51,6 s | 5,0 s | 32 % | 38 |

(KQQ-K und KQR-K noch mit der alten Elternlisten-Fassung gemessen, dort
geschätzt.) Alle Mattlängen exakt, Go und Rust mit identischen Stellungs-,
Kanten- und Auflösungszahlen. Der Bauerntest (12 Steine) sprengt die Grenze
bei Ebene 9 mit über 22 Mio. Stellungen: Für viele Steine ist der erreichbare
Raum nicht endlich genug, dafür bleibt die Vorwärtssuche.

**Rust ist hier nicht schneller als Go**, sondern 2 bis 10 % langsamer. Das ist
dasselbe Bild wie bei der Breitensuche mit rohen Brettern in Milestone 1
(Faktor 1,03): Die Aufzählung ist speichergebunden, pro Kante ein zufälliger
Zugriff in einen 1-GB-Store plus Sätze und Kantenlisten. Rusts Vorteil bei
reiner Rechenarbeit (Perft 1,3 bis 1,5, Mattsuche 1,5 bis 1,8) spielt keine
Rolle, wenn die CPU auf den Speicher wartet. Für eine Parallelisierung heißt
das: Der Gewinn kommt aus mehr gleichzeitig offenen Speicherzugriffen, nicht
aus mehr Rechenleistung.

**Die Spalte "längstes Matt im Graphen" ist eine Verifikation ohne externe
Tabelle.** Der Graph enthält auch das Untermaterial nach Schlagfällen, und die
längsten Distanzen treffen genau die bekannten Maxima: KBN-K 66 Halbzüge =
Matt in 33, KBB-K 19, KQ-KN 21, KR-KR 19, KR-K (in KRR-K und KQR-K) 16, KQ-K
(in KQQ-K) 10. Die Retrograde-Phase rechnet also für diese Materialien bereits
korrekte Tabellenwerte für alle erreichbaren Stellungen.

Lesart:

- **KBN-K in 20 s statt 5 bis 6 Minuten**, Faktor 15 bis 18, mit vollständiger
  Variante über 61 Halbzüge. Je länger das Matt, desto deutlicher gewinnt die
  Liste. Bei kurzen Matts (KRR-K, 0,6 s mit TT) bleibt die Tiefensuche billiger,
  weil sie nur den Beweis sucht, nicht den ganzen Raum.
- Die Zeit steckt zu über 90 % in der Vorwärts-Aufzählung, rund 130 ns pro
  Kante (Zug ausführen, Key, Hash-Zugriff). Elternlisten und Retrograde sind
  Sekunden. Die Aufzählung lässt sich pro Ebene parallelisieren, der Store
  braucht dafür nebenläufiges Einfügen.
- Speicher für 22 Mio. Stellungen: 700 MB Sätze, 1 GB Store (2^26 Slots),
  1,3 GB Kanten (Kinder, danach Eltern), rund 3 GB in der Spitze. Die
  Kind-Liste wird nach dem Bau der Elternliste freigegeben.
- KR-KR löst nur 32 % der Stellungen auf: Der Rest ist Remis oder Gewinn für
  Schwarz, der eigene Turm gibt dem Verteidiger viel Gegenspiel.
- Für vier Steine ist das schon die Tabelle aus Milestone 5, beschränkt auf die
  erreichbare Teilmenge und per Hash statt Index. Der eigentliche Generator
  zählt stattdessen alle Stellungen des Materials auf (mit Symmetrie), nutzt
  Rückwärtszüge statt Elternlisten und schreibt DTM in ein Byte-Array; die
  Retrograde-Logik ist dieselbe.
