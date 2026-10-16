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

**Fünf Steine sprengen die Liste ebenfalls** (2026-10-06, Zuhause, Rust, Grenze
400 Mio. = Store 2^30 Slots = 16 GB): KQ-KBN erreicht bei Ebene 16 schon 387 Mio.
Stellungen und 4,8 Mrd. Kanten, mit 86 bis 96 Mio. neuen Stellungen pro Ebene,
Abbruch bei Ebene 17 nach 960 s, Spitzenverbrauch 42 GB (16 GB Store, 12 GB
Sätze, 19 GB Kind-Kanten). Die Abschätzung vorab: 3.612 Königspaare × 62 × 61 ×
60 × 2 = 1,64 Mrd. Rohstellungen, der Läufer ist farbgebunden (÷ 2), etwa 85 %
legal, plus Untermaterial, also 700 bis 750 Mio. erreichbare Stellungen. Ohne
Symmetrie ist das für die Liste nicht machbar (Kanten allein über 100 GB).
Lehre: Die Rohzahl vor dem Start rechnen. Für KBB-K stimmt sie ebenfalls:
13,7 Mio. roh, 6,9 Mio. mit ungleichfarbigen Läufern, 86 % davon legal und
erreichbar = 5,9 Mio. plus 0,45 Mio. Untermaterial = die gemessenen 6,35 Mio.

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

### Wie groß ist der Beweis? Der Maßstab für jede Suche

Der Autor hielt dagegen, dass `matelist` Brute Force ist und kein besserer
Algorithmus. Um zu wissen, wie viel ein besserer Algorithmus überhaupt sparen
kann, zählt `matelist` nach dem Lösen **einen Beweisgraphen** des Wurzelmatts:
von der Wurzel aus nimmt der Angreifer pro Stellung einen kürzesten Zug, der
Verteidiger alle Züge; gezählt werden die verschiedenen Stellungen. Keine
Suche kann das Matt mit weniger Stellungen beweisen als der kleinste solche
Graph (der hier gezählte ist eine Obergrenze dafür).

| Stellung | Matt in | erreichbare Stellungen | Beweisgraph | Anteil | `mateab`-Expansionen (Stores) | Faktor `mateab` | Faktor `matelist` |
|---|---|---|---|---|---|---|---|
| KRR-K | 7 | 11.079.632 | 907 (1.194 Kanten) | 0,008 % | 578.963 | 640 | 12.200 |
| KQ-KN | 12 | 22.292.508 | 2.687 (3.560) | 0,012 % | 12.883.793 | 4.800 | 8.300 |
| KBN-K | 31 | 12.814.320 | 29.958 (45.788) | 0,23 % | 192.200.797 | 6.400 | 430 |
| KBB-K | 17 | 6.352.868 | 9.825 (13.047) | 0,15 % | 369.430.716 | 37.600 | 650 |
| KP-KP | 25 | 2.878.165 | 4.125 (7.504) | 0,14 % | | | 700 |

Die Vermutung, ein Matt in 31 sei inhärent ein großer Beweis, war falsch: Die
drei bis acht Königszüge des Verteidigers laufen über Transpositionen
zusammen, der Beweis bleibt bei 30 Tsd. Stellungen. Gegen diesen Maßstab
rechnen beide bisherigen Verfahren um Größenordnungen zu viel. Ein Verfahren,
das den **Beweis sucht statt den Raum**, hat hier echtes Potenzial, und der
Beweisgraph ist sein Maßstab: Knoten der Suche geteilt durch Beweisgröße ist
der Overhead des Algorithmus, unabhängig vom Material. Das ist der Auftrag an
df-pn (Schritt 4).

Einschränkung: Der Beweisgraph beweist "Matt in 31 existiert". Der Nachweis,
dass es **kein kürzeres** gibt, ist eine Widerlegung über alle Angreiferzüge
und kann viel größer sein; daran hat sich `mateab` verausgabt. Deshalb die
Zweistufigkeit aus dem Milestone-Text: Stufe 1 findet beweisgeleitet irgendein
Matt (Obergrenze), Stufe 2 beweist die Kürze gezielt mit dieser Schranke.

## Schritt 4: `matepn`, df-pn (2026-10-06)

Arbeitsrechner, direkte Tabelle 256 MB (16 Mi Slots, 24 Value-Bits), Go und
Rust besuchsgenau gleich, Rust 1,5-mal schneller. Algorithmus wie im Entwurf
3.2: Beweis- und Widerlegungszahlen in der TT, Schwellen aus dem zweitbesten
Geschwister (Nagai 2002). Drei Fassungen in Folge:

**1. Unbegrenzt ("irgendein Matt").** Zyklen durch Wiederholungsprüfung auf dem
Pfad abgeschnitten (eine Stellung, die schon auf dem Pfad liegt, ist Remis durch
Wiederholung, also für den Angreifer widerlegt; ein erzwungenes Matt wiederholt
nie). Ohne diese Prüfung läuft df-pn mit den veralteten kleinen Zahlen aus der
TT immer wieder in dieselben Stellungen (KRR-K über 75 Mio. Besuche ohne Ende).
Mit ihr: KRR-K bewiesen in 0,26 s mit 133 Tsd. Besuchen (Faktor 147 über dem
Beweisgraph). Aber KQ-KN explodiert: Dauerschach-Linien haben winzige
Beweiszahlen (der Verteidiger hat kaum Antworten), führen nirgendwohin und
enden erst an der Tiefenschranke 200, deren Widerlegungen pfadabhängig sind und
die Tabelle vergiften. Genau die im Entwurf vorhergesagte Schwäche.

**2. Tiefenschranke als Beweisziel: "Matt in höchstens N".** Die Resttiefe steht
im Key (Salz wie bei `perftTT`), jeder Eintrag ist damit pfadunabhängig und
Zyklen sind unmöglich (die Tiefe fällt streng). Angreifer bei Resttiefe 1 nur
mit Schachgeboten. Terminiert immer, findet an der bekannten Tiefe 2N−1 die
Matts, aber teuer: KQ-KN 2,3 Mio. Besuche, KBB-K nach 54 Mio. abgebrochen. Jeder
Besuch expandiert alle Kinder neu (Zug erzeugen, `DoMove`, Probe), und die Tiefe
im Key vervielfacht den Raum ("Stellung × Resttiefe").

**3. Dieselbe Fassung mit drei Stellschrauben**, einzeln gemessen:

| Variante | KRR-K Besuche | KQ-KN Besuche | KQ-KN Zeit |
|---|---|---|---|
| Grundfassung | 354.845 | 2.346.909 | 6,8 s |
| Mobilität (neues Blatt = Anzahl Züge statt 1) | 118.448 | 956.436 | 3,5 s |
| Mobilität + ε = 1/8 | 71.664 | 598.247 | 2,4 s |
| Mobilität + tiefenfreie Endeinträge | 119.592 | 837.705 | 3,5 s |
| Mobilität + ε = 1/8 + Endeinträge | 55.540 | 698.907 | 3,1 s |
| **Mobilität + ε = 1/2 + Endeinträge** | **86.775** | **181.065** | **0,8 s** |
| Mobilität + ε = 1 + Endeinträge | 71.498 | 338.925 | 1,4 s |
| Mobilität + ε = 2 + Endeinträge | 364.111 | 1.214.702 | 5,5 s |
| ε = 1/2 + Endeinträge ohne Mobilität | 226.607 | 718.042 | 2,1 s |

- **ε-Trick** (Pawlewicz & Lew): Schwelle des Kindes `second·(1+ε)` statt
  `second+1`. Die Suche kehrt seltener zum Elternknoten zurück und expandiert
  weniger neu. ε = 1/2 ist das Optimum, ε = 2 ist schon wieder fast Tiefensuche.
- **Tiefenfreie Endeinträge:** Ein Beweis bei Resttiefe d gilt für jede größere,
  eine Widerlegung für jede kleinere (dieselbe Semantik wie die `mateab`-Einträge).
  Unter dem ungesalzenen Key stehen kleinste bewiesene und größte widerlegte
  Tiefe (7 + 7 Bit). Bringt 10 bis 20 %.
- **Sättigend gegen Gleitkomma** (12 + 12 Bit): kein messbarer Unterschied,
  sättigend bleibt.

**Ergebnis der besten Fassung** (Mobilität, ε = 1/2, Endeinträge), Zieltiefe 2N−1:

| Stellung | Matt in | Besuche | Zeit Go | Zeit Rust | Faktor über Beweisgraph | `mateab` + TT (Knoten, Zeit) |
|---|---|---|---|---|---|---|
| KRR-K | 7 | 86.775 | 0,29 s | 0,18 s | 96 | 578.963 Expansionen, 0,6 s |
| KQ-KN | 12 | 181.065 | 0,78 s | 0,52 s | 67 | 82,8 Mio., 20,8 s |
| KR-KR | 15 | 1.125.947 | 3,6 s | 2,3 s | | 20,2 Mio., 5,0 s |
| KBB-K | 17 | 109.495.187 (1 GB) | | 336 s | 11.100 | 446,5 Mio., 119 s (Rust 82 s) |

Lesart: Beim **Beweisen** ist df-pn auf dem richtigen Weg, Faktor 67 bis 96 über
dem Beweisgraph statt 640 bis 4.800 bei `mateab`; KQ-KN 26-mal schneller. KR-KR
gleichauf. KBB-K (stille Manöver, viele gleichwertige Züge, 33 Halbzüge) wird
mit 1 GB bewiesen, aber viermal langsamer als `mateab`: 109 Mio. Besuche, 381
Mio. neue Blätter, Füllgrad 45 % und 27 Mio. Ersetzungen (direkte Tabelle,
immer ersetzen), weshalb der Beweisbaum in der Tabelle Lücken hat und keine
Hauptvariante liefert. Die Beweiszahlen bieten dort kaum Führung, weil fast
alle Verteidigerzüge gleich viele Antworten haben. Hier fehlt eine Heuristik
für neue Blätter (df-pn+), zum Beispiel Nähe des Verteidigerkönigs zum Rand,
oder später das Netz aus Milestone 7. Zum Vergleich: Von der Stellung aus sind
6,35 Mio. Stellungen erreichbar (`matelist`, 9,8 s), die ganze KBBK-Tabelle
hat 1,5 Mio. legale Indizes (unter 2 s). Die Suche besucht ein Vielfaches des
Raums, weil für sie "Stellung bei Resttiefe d" für jedes d ein eigener Knoten
ist. Solange der Raum aufzählbar ist, gewinnt Aufzählen; die Suche muss sich
ab sechs Steinen oder in Bauernstellungen beweisen.

**Widerlegen bleibt teuer.** Iterative Vertiefung (kürzestes Matt) kostet bei
KQ-KN 7,9 Mio. Besuche und 37 s, weil "kein Matt in d" immer erschöpfend ist:
Jeder Angreiferzug muss widerlegt werden, und dafür helfen Beweiszahlen nicht.
`mateab` brauchte für dieselbe Aussage 82,8 Mio. Knoten und 20,8 s, war also
pro Knoten billiger. Das bestätigt die Zweistufigkeit und zeigt, wo die
Endspieltabellen hingehören: Die Kürze ist eine Wissensfrage, keine Suchfrage.

Der Beweisbaum in der Tabelle liefert die Hauptvariante (`proofLength`,
`proofLine`); mit Mobilität werden Matt-Kinder nie besucht und haben keinen
Eintrag, sie werden beim Ablauf terminal nachgeprüft.

Offen: Ersetzung nach Wert (Beweise und Endeinträge schonen) statt "immer
ersetzen", Heuristik für neue Blätter, df-pn und `mateab` mit den
Endspieltabellen auf KQ-KBN (fünf Steine): der erste Fall, in dem Suche und
Tabellen zusammenarbeiten müssen.

### Vergleich mit anderen Engines und der nächste Schritt (2026-10-06)

Der Autor hat KQ-KBN (Matt in 39) mehreren Engines vorgelegt. Ergebnis: Alle
scheitern an einem garantierten "Matt in X". Dank Syzygy wissen sie, dass die
Stellung gewonnen ist, finden aber keinen Beweis. Lässt man eine Engine 15 bis
20 Züge gegen sich selbst weiterspielen, findet sie von dort aus ziemlich
zuverlässig einen unvermeidbaren Gewinn; geht man die gespielten Züge dann
zurück und lässt weiterrechnen, bleiben die gefundenen Mattwege erhalten und
die früheren Stellungen werden beweisbar, "quasi wie bei einer
Rückwärtsanalyse".

Das ist genau die Zweistufigkeit des Entwurfs, beobachtet von außen: Die
vorgespielte Linie bringt die Suche in die Nähe des Gewinns, dort gelingt der
Beweis, und weil Beweise tiefenunabhängig in der TT stehen, werden rückwärts
entlang der Linie immer frühere Stellungen beweisbar. Zwei Lehren daraus:

1. **Beweise dürfen nie aus der TT verdrängt werden.** Der KBB-K-Beweisbaum
   war genau deshalb lückenhaft. Ersetzung nach Wert: bewiesene und
   Endeinträge bleiben, offene Zahlen fliegen zuerst.
2. **Beweisziel "Gewinn" statt "Matt".** Mit den Tabellen als Orakel ist eine
   gewonnene Tabellenstellung ein Beweis, egal mit welcher Distanz. Für KQ-KBN
   heißt das: beweise, dass Weiß eine Figur gewinnt und in eine gewonnene KQ-KB-
   oder KQ-KN-Stellung kommt, ein viel flacherer Baum als das Matt in 39 (die
   Tiefenschranke ist dann nur Suchhorizont, nicht Mattlänge). Das Ergebnis ist
   die Garantie "Matt in höchstens k Halbzüge plus Tabellen-DTM" mit
   vollständiger Variante bis ins Matt. Stufe 2 verbessert die Schranke, solange
   Zeit da ist, und kann jederzeit abgebrochen werden, ohne die Garantie zu
   verlieren.

Das trifft das eigentliche Ziel des Autors: eine Suche, die zuerst irgendeine
garantierte Gewinnvariante findet und danach weitersucht, bis das kürzeste Matt
feststeht. Nächster Schritt in `matepn`: Ziel "Gewinn", Ersetzung nach Wert,
Messung an KQ-KBN.

Zwei weitere Ideen aus dem Gespräch, vorerst nur notiert:

- **Tabellen on the fly.** Erlaubt der UCI-Modus viel RAM, kann die Mattsuche
  die Fünf-Steiner-Tabelle des Wurzelmaterials (KQKBN: 242 MB plus etwa 90 MB
  Bitsets, grob zehn Minuten) im Hintergrund mitgenerieren und die Suche so
  lange ohne laufen lassen. Ein lokaler Cache pro Material (nie mitgeliefert)
  spart die Zeit beim zweiten Mal.
- **Symmetrie in Liste und TT.** Bauernlose Stellungen vor dem Hashen
  kanonisieren (die Transformationen aus `egtb` existieren): acht Spiegelbilder
  werden ein Eintrag. Für KQ-KBN würde das die Liste von 700 Mio. auf rund
  90 Mio. Stellungen bringen (Spitze um 40 GB, zuhause machbar).

## Schritt 5: Blatt-Initialisierung als Messexperiment (2026-10-13)

Handout `docs/handout-matepn-mobility.md`: Die Mobilität neuer Blätter (Orakel
plus Generator pro frischem Kind) war 37 % des Profils. Drei Varianten als
UCI-Option `MatePnMobility` und Testbank-Buchstaben: `on` (`m`, bisher),
`off` (1/1-Blätter, keine Vorprüfung), `lazy` (`l`: 1/1-Blätter, und beim
Betreten nimmt `mid` die eigene Zuganzahl als erste Schätzung und kehrt ohne
Enkel-Expansion zurück, wenn sie die Schwelle schon reißt). Go und Rust zählen
in allen drei Varianten besuchsgenau gleich.

**Vorweg eine Entdeckung an der Messgröße.** Die Regressionszahl KQ-KN 181.065
Besuche (`mEf`, 256 MB) gilt seit abff3f2 nicht mehr, dort sind es 368.362.
Bisektion: Der Commit hat die Endeinträge der Verteidigerknoten gesalzen
(`finalKey`), was bei ungeraden Zieltiefen keine Semantik ändert (jede
Stellung hat dort eine feste Rolle), sondern nur die Slot-Lage in der direkten
Tabelle und damit, welche Einträge ersetzt werden. Dass das die Besuche
verdoppelt, zeigt der Lauf über Tabellengrößen (Go, Arbeitsrechner, Zeit eines
Einzellaufs):

| TT | `on` Besuche | Zeit | `off` Besuche | Zeit | `lazy` Besuche | Zeit |
|---|---|---|---|---|---|---|
| 64 MB | 243.179 | 0,78 s | 903.474 | 1,96 s | 1.201.099 | 0,95 s |
| 128 MB | 848.088 | 2,81 s | 722.077 | 1,51 s | 934.910 | 0,74 s |
| 256 MB | 368.362 | 1,23 s | 970.937 | 2,09 s | 1.450.453 | 1,19 s |
| 512 MB | 344.425 | 1,16 s | 1.193.924 | 2,75 s | 1.435.691 | 1,31 s |
| 1024 MB | 213.994 | 0,78 s | 2.823.809 | 7,62 s | 2.602.209 | 2,41 s |
| 2048 MB | 244.912 | 0,95 s | 985.463 | 2,79 s | 2.512.333 | 2,56 s |

Die Besuche schwanken bei 1 bis 8 % Füllgrad um Faktor 4, und 128 MB ist
schlechter als 64 MB: df-pn reagiert chaotisch darauf, welche wenigen tausend
Einträge "immer ersetzen" verdrängt (ein verdrängter Beweis kostet den ganzen
Teilbaum noch einmal). **Eine Besuchszahl bei einer Größe ist deshalb kein
Regressionswert und kein Varianten-Vergleich**, nur die Gleichheit von Go und
Rust bleibt als Prüfung. Das ist das stärkste Argument bisher für die
Ersetzung nach Wert (Beweise und Endeinträge nie verdrängen), die als nächster
`matepn`-Schritt ohnehin vorgemerkt ist; danach wären die Zahlen wieder
belastbar.

**Testbank bis Matt in 15, 256 MB** (Besuche Go = Rust, Zeit Rust, Go und Rust
liefen gleichzeitig, Zeiten daher nur grob):

| Stellung | `on` | Zeit | `off` | Zeit | `lazy` | Zeit |
|---|---|---|---|---|---|---|
| KQQ-K | 31 | | 1.086 | | 1.084 | |
| KQR-K | 10.285 | 22 ms | 51.391 | 36 ms | 50.882 | 19 ms |
| KRR-K | 86.803 | 0,15 s | 223.687 | 0,22 s | 318.089 | 0,14 s |
| KQ-KN | 368.362 | 0,82 s | 970.937 | 1,18 s | 1.450.453 | 0,75 s |
| KR-KR | 1.206.343 | 1,94 s | 1.975.657 | 2,22 s | 6.869.270 | 5,57 s |
| Bauern | 953 | | 4.711 | | 4.917 | |
| Summe | 1.672.777 | 2,93 s | 3.227.469 | 3,67 s | 8.694.695 | 6,49 s |

Lesart: `off` ist in Besuchen und Zeit überall schlechter, die Vorprüfung der
Blätter zahlt sich aus. `lazy` macht die Besuche billig (0,5 bis 0,8 µs statt
2 bis 3 µs, weil viele Besuche nach `terminal` sofort zurückkehren) und ist
auf KQ-KN und KRR-K zeitlich gleichauf oder vorn, braucht aber drei- bis
fünfmal so viele Besuche und verliert auf KR-KR deutlich; mit den vielen
Besuchen füllt es die Tabelle schneller (16 % statt 5 % bei KR-KR) und leidet
entsprechend mehr unter Ersetzungen. Die Vorgabe bleibt `on`. Entscheidend ist
die Benchmark-Stellung KRR-KN mit großem Hash (`test-positions.md`), die der
Autor misst.

**Benchmark KRR-KN mit großem Hash** (Messung des Autors, Rust, ein Thread,
Hash 16384, zuhause, 2026-10-13, Stand d6cdbff plus Option; je Tiefe die Zeit
und die Besuche, bis die Werte dieser Tiefe stehen; `mateab` mit derselben
Tabellengröße zum Vergleich, dort Knoten statt Besuche).

Schwarz am Zug, Wurzel = Verteidiger, `8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1`:

| Tiefe | Züge | `off` | `on` | `lazy` | `mateab` |
|---|---|---|---|---|---|
| 8 | Kf7 (−M4) | 2,7 s, 337 Tsd. | 0,3 s, 81 Tsd. | 0,3 s, 473 Tsd. | 0,06 s, 273 Tsd. |
| 12 | sieben Königszüge (−M6) | 16,9 s, 6,0 Mio. | 7,3 s, 1,7 Mio. | 8,1 s, 11,2 Mio. | 2,8 s, 12,0 Mio. |
| 14 | Nh2 (−M7) | 41,9 s, 15,8 Mio. | 22,4 s, 5,6 Mio. | 26,0 s, 33,9 Mio. | 15,7 s, 61,5 Mio. |
| 16 | Nd2, Ne3 (−M8) | 114,3 s, 43,0 Mio. | 70,3 s, 17,8 Mio. | 83,8 s, 98,7 Mio. | 53,8 s, 198 Mio. |
| 18 | Ng3 (−M9) | 114,4 s, 43,0 Mio. | 70,5 s, 17,8 Mio. | 83,8 s, 98,7 Mio. | 115,7 s, 414 Mio. |
| µs je Besuch | | 2,7 | 4,0 | 0,85 | 0,28 |

Weiß am Zug nach Nh2, Wurzel = Angreifer, `8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2`,
27 Züge, Matt in 7 (`off` nach Tiefe 19 von Hand gestoppt, die anderen nach
Tiefe 21; die 16 übrigen Züge sind länger als Matt in 11):

| Tiefe | Züge | `off` | `on` | `lazy` | `mateab` |
|---|---|---|---|---|---|
| 13 | Rc5 (M7) | 27,3 s, 11,4 Mio. | 13,9 s, 3,5 Mio. | 15,8 s, 21,2 Mio. | 6,9 s, 3,9 Mio. |
| 15 | Ra5 (M8) | 88,0 s, 34,9 Mio. | 48,7 s, 12,5 Mio. | 59,4 s, 73,0 Mio. | 25,3 s, 94,7 Mio. |
| 19 | Ka3, Kb3, Kc3, Ra3, Rc3, Re1+, Rh1 (M10) | 544 s, 199 Mio. | 368 s, 92,6 Mio. | 407 s, 412 Mio. | 382 s, 1,31 Mrd. |
| 21 | Kc2, Ka1, Kb1 (M11) | gestoppt | 450 s, 107 Mio. | 481 s, 475 Mio. | 858 s, 2,81 Mrd. |
| µs je Besuch | | 2,7 | 4,2 | 1,0 | 0,31 |

Lesart:

- **`on` gewinnt auf beiden Stellungen, die Vorgabe bleibt `on`.** `lazy`
  braucht 4,4- bis 5,5-mal so viele Besuche zu einem Viertel der Kosten pro
  Besuch und landet 19 % (Verteidiger) bzw. 7 % (Angreifer) hinter `on`; `off`
  liegt 48 bis 62 % zurück. Die 37 % Blattkosten im Profil sind echt, aber die
  Vorprüfung spart mehr Besuche, als sie kostet: Ohne sie muss jedes frische
  Angreiferkind einmal betreten werden (Besuch plus Eintrag), bevor sein pn über
  1 steigt, und davon gibt es auf jeder zweiten Ebene 27 pro Knoten.
- **`mateab` findet das erste Matt schneller, `matepn` alle.** Ein
  `mateab`-Knoten kostet 0,3 µs, ein `matepn`-Besuch 4 µs; bis Tiefe 13 bzw. 16
  liegt `mateab` deshalb vorn (6,9 gegen 13,9 s, 53,8 gegen 70,3 s). Jede
  weitere Tiefe kostet `mateab` aber den Raum noch einmal: Ng3 bei Tiefe 18
  kostet `mateab` 62 s und `matepn` 0,2 s, Tiefe 21 des Angreifers 858 gegen
  450 s. Je länger die Matts, desto deutlicher gewinnt df-pn.
- **4 µs je Besuch bei 16 GB gegen 3 µs bei 256 MB** (Mechanik-Messgröße in
  `test-positions.md`: 16,5 s für 5,53 Mio. Besuche). Die große Tabelle kostet
  bei jeder Sondierung einen TLB-Miss zusätzlich zum Cache-Miss; Abhilfe wären
  2-MB-Seiten (Large Pages), notiert, nicht Teil dieses Schritts.
- **Fallen beim Messen:** Die erste Tiefenzeile des ersten Laufs einer Sitzung
  enthält die Seitenfehler der frisch angelegten 16-GB-Tabelle (hier 2,7 s bei
  `off`, beim Einzellauf von `on` am Vormittag 2,4 s für dieselben 81 Tsd.
  Besuche, die jetzt 0,3 s brauchen). Und die Tabelle bleibt zwischen Suchen
  stehen, nur `ucinewgame` leert sie; Varianten deshalb aus frischer Engine oder
  nach `ucinewgame` messen. Der `on`-Lauf deckt sich mit dem Einzellauf (70,5
  gegen 72,7 s), die Reihenfolge hat hier also nicht sichtbar verzerrt.

Damit ist das Messexperiment abgeschlossen: Option und Testbank-Buchstabe
bleiben als Schalter für spätere Vergleiche, die Suche läuft weiter mit
Mobilität. Nächster `matepn`-Schritt: Ersetzung nach Wert (Beweise und
Endeinträge nie verdrängen), danach das Beweisziel "Gewinn".

## Schritt 6: Intervall-Orakel (2026-10-15)

Anlass: quantisierte Endspieltabellen (`docs/m5-endgame-tables.md`, Abschnitt
"Quantisierte Werte"), die statt der exakten Distanz nur noch "Matt in höchstens
8 Zügen" speichern und damit halb so groß sind. Damit die Suche solche Werte
nutzen kann, liefert das Orakel jetzt ein **Intervall** statt einer Zahl:
`Probe(b) (Verdict, lo, hi)`, die wahre Distanz in Halbzügen liegt in
`[lo, hi]`. Exakte Tabellen geben `lo == hi`, eine Tabelle mit Schrittweite 4
das Fenster ihres Eimers (acht Halbzüge), eine WDL-Tabelle ohne Distanz
`1..Unbounded`. Die Regel in beiden Suchen (`mateab` `attack`/`defend`,
`matepn` `terminal`) am Knoten mit Resttiefe d:

- `hi ≤ d`: bewiesen, der Knoten endet (wie bisher der exakte Treffer),
- `lo > d`: widerlegt, auch wenn die Stellung an sich gewonnen ist,
- dazwischen: der Knoten wird expandiert wie jede andere Stellung; jedes Kind
  trägt wieder Schranken, also endet die Teilsuche spätestens dort, wo der
  nächste Eimer die Resttiefe entscheidet. Die Suche löst genau das Band auf,
  in dem der Eimer nichts sagt, und das ist höchstens so tief wie das Fenster
  breit.

Der Beweisbaum (`proofLength`) nimmt `hi`, wo das Orakel die Linie beendet hat,
unter derselben Bedingung `hi ≤ d`. Die Korrektheit der Mattzahl hängt nicht am
Wert eines Treffers, sondern an der Tiefe des ersten Beweises: ein Beweis bei
Tiefe d heißt "Matt in ≤ d", und bei d − 2 gab es keinen, also ist d exakt. Das
gilt für den UCI-Wurzeltreiber genauso wie für die iterative Vertiefung der
Testbank.

Die Tabellenstufe des UCI (`eval`) zählt eine quantisierte Tabelle **nicht** als
bekannt (`LookupBounds`, nur `lo == hi` gilt), sonst würde sie Eimergrenzen als
`score mate N` melden; solche Wurzeln gehen in die Suche, die die Schranken als
Orakel hat. Folge: Die Variante endet am Orakelblatt, `extendPV` verlängert nur
durch exakte Tabellen (eine quantisierte Tabelle lässt sich nicht abspielen,
innerhalb eines Eimers könnte die Linie kreisen).

**Messung** mit `5-KRRKN.bin`, im RAM quantisiert (Go und Rust, Arbeitsrechner,
Hash 256, Werte alle exakt wie mit der exakten Tabelle, beide Binaries
zeilenidentisch):

| Stellung | ohne Tabelle (`matepn`, Hash 16384) | Schrittweite 4, `matepn` | Schrittweite 8, `matepn` | Schrittweite 4, `mateab` |
|---|---|---|---|---|
| KRR-KN Schwarz am Zug, zwölf Werte | 70,5 s, 17,8 Mio. Besuche | 288 Besuche, 1 ms | 65.123 Besuche, 0,28 s | 3.532 Knoten |
| nach Nh2, Weiß am Zug, 27 Werte | 450 s bis M11, 107 Mio. | 1.174 Besuche, 15 ms | 71.697 Besuche, 0,3 s | |
| KQ-KBN, Matt in 39 (77 Halbzüge) | ungelöst | 1.844 Besuche, 1 ms | 59.013 Besuche, 0,26 s | |

Die Teilsuche im Fenster kostet wenig: Die Schranken entscheiden fast jedes Kind
sofort, nur die Kette entlang der echten Distanz wird betreten. Die obere
Schranke aus der Zeitleiste (0,3 s für acht Halbzüge) war um drei
Größenordnungen zu pessimistisch; eine Verdopplung des Fensters kostet rund
Faktor 200 (Schrittweite 16: 14 bis 67 s), unabhängig von der Mattlänge.
**Der Autor hat Schrittweite 8 gewählt** und fest ins Dateiformat gelegt
(`egtb.FileStep`, Format v2 in `docs/m5-endgame-tables.md`): Cache-Dateien ab
fünf Steinen halten Eimer von acht Zügen, die Basis bleibt exakt, es gibt keine
Option. Die Antwort an der Wurzel kommt dann nach rund 0,3 s aus der Suche
statt sofort aus der Tabelle.

**Wurzeltreiber und Tabellenwissen (2026-10-16).** Zwei Befunde an der
Stellung `8/8/4k3/8/8/8/R7/K1R2n2 w` (Rxf1 führt in den Vier-Steiner, Matt in
7 laut Basis): Die Tabellenstufe galt als abschließend, sobald der beste Zug
ein bekannter Tabellengewinn war, und hat die anderen Züge nie angesehen. Das
beweist nur "Rxf1 gewinnt", nicht "nichts ist kürzer"; ein stiller Zug mit
schnellerem Matt wäre übersehen worden (hier zufällig nicht: die Suche
widerlegt Matt in 6 mit 5.348 Besuchen). Und ein Wurzelzug, den die Tabelle als
Remis kennt (nach Nh2 der Zug Rf1 mit Sxf1 in KR-KN), wurde von der Suche nie
bewiesen, also lief der Treiber bis zur Tiefenschranke 127 weiter. Beides
behoben:

- Die Tabellenstufe ist nur noch abschließend, wenn **alle** Züge bekannt
  sind (oder es keine gibt).
- `runRoot` bekommt die exakt bekannten Wurzelzüge mit (`Engine.rootTable`):
  Gewinne und Verluste starten als bewiesen mit Wert und Variante aus der
  Tabelle, Remis als erledigt (`rootEntry.draw`, nie gesucht, in der Liste
  hinter den Gewinnen vor den offenen Zügen); gesucht werden nur unbekannte und
  quantisierte Züge, die Schleife endet, wenn alle erledigt sind.
- Bei MultiPV 1 reicht die Kürzer-Prüfung: Kennt die Tabelle ein Matt in n
  Halbzügen, wird nur bis n − 2 gesucht; ein bei Tiefe d bewiesener Gewinn
  beendet die Schleife nur, wenn er in d passt (sonst bräche der bekannte
  längere Gewinn sie sofort ab). Beispiel oben: Tiefen 1 bis 11 mit 7.968
  Besuchen, dann steht Rxf1 mit exaktem Wert; mit MultiPV 32 werden alle 26
  Züge aufgelöst (24 Gewinne bis Matt in 12, zwei Tabellen-Remis), 84 Tsd.
  Besuche, Schleife endet bei Tiefe 23 statt 127. Nach Nh2 ebenso: 27 Züge,
  Rf1 als Remis zuletzt, Ende bei Tiefe 23. Go und Rust zeilenidentisch.

Testbank: `matepnSolveFEN` nimmt eine Stellung außerhalb der Regressionsliste
(`chess.MatePositions` bleibt unverändert, damit die Zahlen aus Schritt 5 stehen
bleiben); mit `t` lädt `egtbLoadForPositions` die Cache-Dateien der Materialien
der gewählten Stellungen (quantisiert, die Suche löst auf). Während der Messung
gab es kurz eine UCI-Option `EgtbQuantize` und einen Buchstaben `q`; beide sind
mit der Entscheidung für die feste Schrittweite wieder entfallen.

## Schritt 7: Ersetzung nach Wert (2026-10-16)

Das Problem stand seit Schritt 5 fest: `matepn` speicherte alle Einträge mit
gleichen Rechten in der direkten Tabelle ("immer ersetzen"), und ein frisches
Blatt löschte dabei beliebig Beweise, Widerlegungen und Endeinträge. Für df-pn
ist das teurer als für Alpha/Beta, weil `mid` ständig in Teilbäume
zurückkehrt: Ein verdrängter Beweis wird komplett neu bewiesen, verdrängte
offene Zahlen ändern über die Schwellen den Weg der Suche, und `proofLength`
findet den Beweisbaum nicht mehr (KBB-K, KR-KR). Die Besuche schwankten
deshalb allein mit der Tabellengröße um Faktor 4 und waren kein Maß für
irgendetwas.

**Einbau.** Jeder Value trägt oben zwei **Prioritätsbits** (`PriorityBits`,
Klassen 0 offen, 1 widerlegt, 2 bewiesen, 3 Endeintrag; Widerlegungen gelten
nur für ihre Tiefe, Beweise für jede tiefere und für die Beweislinie), der
Codec bekommt die Bits darunter (`CodecBits`). `tt.Buckets` kennt die Bits
über `SetPriorityBits(n)`: Der Opfer-Eintrag ist wie bisher der kleinste Value
des Buckets (Klasse zuerst, darunter die Zahlen), aber liegt seine Klasse über
der des neuen Eintrags, wird der neue **verworfen** (`Stats.Dropped`) statt
gespeichert. Ein offenes Blatt findet in einem Bucket voller Beweise also
keinen Platz, und das ist richtig: Die Schätzung ist das Billigste im Spiel.
`Stats.Evicted[4]` zählt Verdrängungen nach der Klasse des verlorenen Eintrags,
auch in der direkten Tabelle. Testbank-Buchstabe `b` wählt die Buckets, UCI
nimmt sie für `matepn` immer (`info string tt: ... replaced (... proofs, ...
finals), ... dropped` nach jeder Suche). Die direkte Tabelle bleibt als
Vergleich, ihre Prioritätsbits sind dort nur Statistik.

**Codec-Breite gedeckelt.** Die ersten Reihen mit Buckets zeigten, dass bei
null Verdrängungen die Besuche nur noch von der halben Codec-Breite abhängen
(64 = 128 MB, 256 = 512 MB, 1024 = 2048 MB jeweils besuchsgleich, weil die
Sättigungsgrenze 510, 1022, 2046 den Weg bestimmt). `MaxCodecBits` = 20
(Sättigung bei 1022) macht die Besuche ab 256 MB Buckets bei jeder Größe
identisch, auch bei 16 GB; darunter sind es weniger Bits. Die Klasse sitzt
immer in den obersten Bits der Tabelle, unabhängig vom Codec.

**Messung** (Go = Rust besuchsgenau, Arbeitsrechner, `mEf` plus `b`, Einzeltiefe
2N − 1; Zeiten eines einzelnen Laufs, grob):

| TT | KQ-KN Buckets | verdrängt | KR-KR Buckets | verdrängt | KQ-KN direkt | davon Beweise | KR-KR direkt | davon Beweise |
|---|---|---|---|---|---|---|---|---|
| 64 MB | 172.346 | 0 | 1.151.675 | 1.755 (0 Beweise, 1 Endeintrag) | 148.697 | 23 + 411 | 1.407.646 | 5.765 + 26.053 |
| 128 MB | 172.346 | 0 | 1.108.539 | 158 (0, 1) | 132.413 | 10 + 162 | 849.617 | 1.307 + 5.006 |
| 256 MB | 271.120 | 0 | 1.023.102 | 19 (0, 0) | 179.602 | 11 + 179 | 901.329 | 772 + 3.285 |
| 512 MB | 271.120 | 0 | 1.022.906 | 2 (0, 0) | 591.589 | 93 + 1.025 | 1.088.826 | 504 + 1.960 |
| 1 GB | 271.120 | 0 | 1.022.904 | 0 | 670.774 | 60 + 627 | 857.926 | 161 + 771 |
| 2 GB | 271.120 | 0 | 1.022.904 | 0 | 577.626 | 24 + 202 | 1.263.454 | 160 + 717 |

Lesart:

- **Die Buckets konvergieren, die direkte Tabelle nicht.** KQ-KN zählt ab
  256 MB bei jeder Größe exakt 271.120 Besuche, KR-KR ab 1 GB 1.022.904 (bei
  256 MB 19 verdrängte offene Einträge, 0,02 % Abweichung). Die direkte
  Tabelle streut weiter um Faktor 5 (KQ-KN 132 Tsd. bis 671 Tsd.) und verliert
  bei **jeder** Größe Beweise und Endeinträge, selbst bei 2 GB und 0,2 %
  Füllgrad: Bei n Einträgen in m Slots gibt es rund n²/2m Kollisionen, und die
  treffen Beweise genauso wie Schätzungen.
- **Beweisbaum komplett.** KR-KR liefert mit Buckets bei jeder Größe die
  Variante, mit der direkten Tabelle bei keiner. Das war in Schritt 4 (KBB-K)
  und 5 der Grund für "Mattlänge nur als Schranke".
- **Tempo ist nicht das Argument.** Testbank bis Matt in 15 bei 256 MB:
  Buckets 1.370.235 Besuche, direkte Tabelle 1.402.380 mit 22 Codec-Bits, aber
  1.156.948 mit 20 (KQ-KN 179.602, KR-KR 901.329, beide ohne Beweisbaum). Dass
  die direkte Tabelle in einer Konfiguration weniger Besuche braucht, ist
  Zufall der Kollisionen, bei 512 MB sind es für KQ-KN 591.589. Die
  Besuchszeit ändert sich nicht messbar, die vier Einträge eines Buckets liegen
  in einer Cache-Line. Die Benchmark-Stellung KRR-KN mit 16 GB misst der
  Autor; dort zählt, ob die `tt:`-Zeile null verdrängte Beweise meldet und ob
  die Tiefen-Zeiten unter den 70,5 s aus Schritt 5 bleiben.
- **Die Besuchszahlen sind wieder Regressionswerte:** Testbank `mEfb`, 256 MB:
  KQQ-K 31, KQR-K 10.284, KRR-K 64.745, KQ-KN 271.120, KR-KR 1.023.102, Bauern
  953, Summe 1.370.235 (Go = Rust). Ab 1 GB KR-KR 1.022.904. Die alten Werte
  aus Schritt 5 (mit direkter Tabelle und 24 Codec-Bits) gelten nicht mehr.

Noch nicht gemacht, als Verfeinerung notiert: Unter offenen Einträgen den mit
dem kleineren **Aufwand** verdrängen (Besuche im Teilbaum als kleiner Zähler im
Value, die klassische df-pn-Ersetzung) statt den mit den kleineren Zahlen; und
Alterung über Suchen hinweg (die Tabelle bleibt zwischen `go`-Kommandos, nur
`ucinewgame` leert sie; Beweise einer alten Wurzel bleiben gültig, offene
Einträge anderer Tiefen nicht). Nächster Schritt: Beweisziel "Gewinn".
