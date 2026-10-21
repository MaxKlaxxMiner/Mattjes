# Milestone 7: WDL-Suche, Ergebnisse

Entwurf in `docs/m7-wdl-search-design.md`, Teststellungen in `test-positions.md`
("WDL-Teststellungen") und `chess/wdldata.go`. Alle Zahlen hier vom
Arbeitsrechner (i5, ein Thread). **Rust-Port am 2026-10-21** (`src/wdlpn/`,
`src/chess/wdldata.rs`, `src/tests_wdl.rs`): Besuche, Einträge und Linien
identisch mit Go (diff der Testbank-Ausgabe ohne Zeiten über fünf
Stellungen), Rust 1,3 bis 1,7-mal schneller (KBB-K 1,68 s statt 2,58 s,
KRR-KN 2,23 s statt 3,74 s, KP-K Randbauer 2,21 s statt 2,26 s: die
Remisfälle sind tabellengebunden). Die Rust-Testbank läuft auf einem Thread
mit 256 MB Stack, weil jede Rekursionsstufe die Kinder mit ihren
Abhängigkeits-Bitsets hält (rund 18 KB bei 255 Halbzügen Deckel).

## Schritt 1: `wdlpn`, df-pn ohne Tiefenschranke (2026-10-21)

Package `wdlpn` (Arbeitstitel), Testbank `wdlpnSolve(names, sizeMB, codec,
options)` in `tests_wdl.go`, Aufruf `./runMattjesGo.exe test <names> <options>
<codec>` mit `names` = Name, Namen mit Semikolon, `wins`, `draws` oder leer
für alle; Optionen wie in `matepn` (`m` Mobilität, `e/E/x/X` ε = 1/8, 1/2,
1, 2, `b` Buckets mit Klassen, `T` Vier-Steiner-Basis als Orakel).

Was anders ist als in `matepn`:

- **Kein Tiefensalz**, der Key ist die Stellung plus Angreiferfarbe. Beide
  Fragen ("Seite am Zug gewinnt" = Wurzel OR-Knoten, "Gegner gewinnt" = Wurzel
  AND-Knoten) laufen über denselben Code, nur die Rolle der Wurzel wechselt.
  Damit liefert der erste Schritt schon die komplette WDL-Antwort: Q1
  bewiesen = Gewinn, sonst Q2 bewiesen = Verlust, beide widerlegt = Remis.
- **Wiederholung als Terminal** über ein Pfad-Array (`path[ply]`, Vergleich
  nur gegen gleiche Seite am Zug und nur bis zum letzten irreversiblen Zug
  per `HalfmoveClock`). Sie zählt als Remis, egal wer wiederholt.
- **Bedingte Widerlegungen (GHI):** jede Widerlegung trägt die Menge der
  Pfad-Tiefen, deren Stellungen sie per Wiederholung braucht (`depSet`, ein
  256-Bit-Bitset). OR-Knoten vereinigen die Mengen aller Kinder, AND-Knoten
  nehmen das Kind, dessen flachste Abhängigkeit am tiefsten liegt; die
  eigene Tiefe wird gelöscht (die Rückkehr zum Knoten selbst ist dort
  aufgelöst). Leer = pfadunabhängig, Klasse 2. Nicht leer = Klasse 1
  (`classConditional`), und die Keys der benötigten Vorgänger liegen in
  einer Nebentabelle `deps` neben der TT; beim Sondieren eines Kindes mit
  Klasse 1 wird die Widerlegung nur verwendet, wenn alle diese Keys auf dem
  aktuellen Pfad liegen (dann bekommt das Kind deren Tiefen als
  Abhängigkeit). Das ist die Abhängigkeitsliste aus Kishimoto und Müllers
  allgemeiner GHI-Lösung, nur dass die Liste außerhalb des 16-Byte-Eintrags
  lebt.
- **Tiefendeckel** 255 Halbzüge: ein Knoten darüber gibt die Schwellen
  zurück (unbekannt, nichts gespeichert). In keinem Lauf erreicht.
- Klassen: 0 offen, 1 bedingt widerlegt, 2 widerlegt, 3 bewiesen.
- Variante: gieriger Lauf durch bewiesene Kinder (Angreifer bevorzugt
  terminale Kinder, Verteidiger das erste bewiesene, besuchte Stellungen
  werden übersprungen); eine Linie des Beweises, nicht die kürzeste.

### Drei Fallen auf dem Weg

1. **Ohne gespeicherte bedingte Widerlegungen** (erste Fassung: bedingt =
   gar nicht speichern) brauchte KP-K Randbauer 40 Mio. Besuche für 57 Tsd.
   Stellungen: jeder neue Pfad rechnete dieselben Teilbäume neu, weil die
   Abhängigkeit fast immer ein wurzelnaher Vorgänger ist und alle Knoten
   dazwischen ungespeichert blieben.
2. **`satAdd` darf Inf nicht erreichen.** Ohne Tiefenschranke überzählt
   der DAG so stark, dass Summen 2^30 erreichten (pn 39 Mio. bei 77 Tsd.
   Stellungen); die Summe wurde Inf und sah an der Wurzel wie eine
   Entscheidung aus (`dn ≥ Inf` bei `pn > 0`, Ergebnis "unentschieden").
   Endliche Summen bleiben jetzt bei Inf − 1 stehen. In `matepn` ist die
   Falle latent, aber mit Tiefenschranke unerreichbar.
3. **Sättigung macht Kinder ununterscheidbar.** Mit dem `sat`-Codec stehen
   Zahlen über 1022 als 1022 in der Tabelle; ein Kind, das mit 1600
   zurückkam, sieht beim nächsten Besuch wieder wie 1022 aus, wird erneut
   betreten, rechnet sofort wieder 1600 und kehrt zurück, jedes Mal mit
   einer Sondierung aller Enkel. Der Versuch, bei gesättigtem Geschwister
   die Schwelle auf Inf zu setzen ("rechne das Kind zu Ende"), war
   schlechter (Timeout statt 2,3 s): mit Schwelle Inf wird df-pn zur reinen
   Tiefensuche über alle einfachen Pfade des Graphen. Der `float`-Codec
   (bis 1 Mio.) war ebenfalls schlechter: ohne Sättigung läuft das
   Überzählen frei, und die Zahlen lenken die Suche in die Breite.

### Messung (ε = 1, `sat`, Buckets 256 MB, Mobilität, Tabellen aus)

ε = 1/2 (das Optimum von `matepn`) brauchte für KP-K Randbauer 21 Mio.
Besuche ohne Ende, ε = 1 2,5 Mio. in 2,3 s, ε = 2 4,9 Mio. in 8,7 s. Der
Rest der Messung mit ε = 1.

Gewinne und Verluste (Besuche der entscheidenden Frage; bei Verlust beide):

| Stellung | Ergebnis | Besuche | Einträge | Zeit |
|---|---|---|---|---|
| KQ-KQ Dame hängt | Gewinn | 8.337 | 6.287 | 33 ms |
| KQ-K | Gewinn | 17.575 | 11.690 | 52 ms |
| KR-K | Gewinn | 147.822 | 86.437 | 0,33 s |
| KQ-KN | Gewinn | 128.760 | 100.939 | 0,43 s |
| KBB-K | Gewinn | 888.614 | 529.224 | 2,6 s |
| KBN-K | Gewinn | nicht fertig in 100 s | | |
| KP-K König vor Bauer, Weiß | Gewinn | 16.958 | 9.621 | 46 ms |
| KP-K König vor Bauer, Schwarz | Verlust | 88 + 66.509 | 16.784 | 0,14 s |
| KBP-K richtiger Läufer | Gewinn | 446 | 446 | 0 ms |
| KQ-KR | Gewinn | 10.337 | 7.831 | 34 ms |
| KQ-KP Springerbauer | Gewinn | 149.982 | 99.167 | 0,47 s |
| KQQ-KN | Gewinn | 4 | 4 | 0 ms |
| KRR-KN Benchmark, Schwarz | Verlust | 406.136 + 442.859 | 697.429 | 3,7 s |
| KQ-KBN | Gewinn | nicht fertig in 100 s | | |

Zum Vergleich `matepn` mit Mattziel: KQ-KN 271.120 Besuche, KBB-K 109 Mio.
(336 s), KRR-KN alle zwölf Werte 70 s. Ohne Mattdistanz ist der
Gewinnbeweis bei KBB-K **120-mal kleiner**, und das Benchmark-Urteil "alle
Züge verlieren" steht nach 3,7 s statt 70 s (noch ohne MultiPV je Zug).

Remis (Q1 und Q2 müssen beide widerlegt werden):

| Stellung | Besuche Q1 | Besuche Q2 | Einträge | Zeit |
|---|---|---|---|---|
| KP-K Randbauer | 2.509.289 | 29 | 67.139 | 2,3 s |
| KP-K Opposition | 2.720.916 | 26.591 | 88.362 | 2,4 s |
| KNN-K | nicht fertig in 100 s | | | |
| KBP-K falscher Läufer | nicht fertig in 100 s | | | |

Die übrigen Remis-Stellungen (Patt-Tricks gegen die Dame, KR-KB, ungleiche
Läufer, Festung, Dauerschach, Philidor, KNN-KP) sind nicht gemessen; ihre
Graphen sind mindestens so groß wie der des falschen Läufers.

Lesart: Gewinne sind billig, Widerlegungen sind erschöpfend und df-pn
zahlt dafür Faktor 30 bis 40 über der Zahl der Stellungen (2,5 Mio.
Besuche für 67 Tsd. Einträge). Der Beweisgraph eines Remis ist die "sichere
Menge" des Verteidigers (alle Angreiferzüge, ein Verteidigerzug), bei KP-K
Randbauer vermutlich ein bis zwei Tausend Stellungen; davon sind wir
Faktor 1.000 entfernt. Die drei Fallen oben sind alle Folgen desselben
Umstands: df-pn arbeitet mit lokalen Zahlen auf einem Graphen, dessen
Remis-Eigenschaft eine **globale** ist (ein Zyklus ohne Gewinn ist remis),
und flickt das mit Pfad-Wiederholung, Abhängigkeitslisten und Schwellen.

### Lehre und nächster Schritt: die sichere Menge explizit

Ein Remisbeweis ist eine Menge S mit der Wurzel, abgeschlossen unter "jeder
Angreiferzug bleibt in S oder endet in einem Remis-Terminal" und "jeder
Verteidigerknoten hat einen Zug nach S oder in ein Remis-Terminal", ohne
Angreifer-Gewinn darin. Auf einem **expliziten Graphen** (Knoten mit
Kindkanten und Elternlisten, wie in `matelist`) ist das ein Fixpunkt und
braucht keine Wiederholungserkennung: Gewinne werden retrograd mit Zählern
propagiert (Angreiferknoten gewonnen, sobald ein Kind verloren ist;
Verteidigerknoten verloren, sobald alle Kinder gewonnen sind), und sobald
der von der Wurzel erreichbare Teil keine offene Front mehr hat, ist alles,
was nicht gewonnen ist, remis oder besser. Zyklen kosten nichts, es gibt
keine GHI, keine Schwellen, keine Neu-Expansion.

Vorschlag für Schritt 2 (`wdllist`, Arbeitstitel): erst die
unbeschnittene Fassung als Referenz, dann Beschneidung für kleine Beweise,
`wdlpn` bleibt die Messlatte. Was daraus wurde, steht im nächsten
Abschnitt.

## Schritt 2: `wdllist`, der explizite Graph (2026-10-21)

Package `wdllist`, Testbank `wdllistSolve(names, options)` in
`tests_wdllist.go`, Aufruf `./runMattjesGo.exe test <names> <options>`
(Buchstaben: `B` Brute Force, `p` nur Beweiszahl-Abstieg, `e/E/x/X` ε,
`w` weite gespeicherte Zahlen, `l` Log-Maßstab, `v` Fortschrittszeilen;
Vorgabe `x`). Go, ein Thread je Suche; Rust folgt.

**Was gebaut ist.** Knoten (32-Byte-Satz, Status, Seite, Zugzahl, offene
Züge, pn/dn) und Kanten (Kinder- und Elternlisten) liegen im Speicher,
`ttstore` bildet Key auf Index ab. Drei Mechanismen teilen sich den
Graphen:

- **Gewinne exakt und inkrementell:** ein Knoten ist gewonnen, sobald ein
  Kind verloren ist, verloren, sobald alle legalen Züge gewonnen sind
  (Zähler `remaining`, nicht ausprobierte Züge bleiben offen, darum ist
  jeder Zwischenstand korrekt). `propagate` schiebt Entscheidungen zu den
  Eltern.
- **Remis exakt als Fixpunkt (`safeSet`):** unsicher ist, was der Angreifer
  noch gewinnen könnte, die kleinste Menge aus gewonnenen Knoten,
  unexpandierten Knoten und Angreiferknoten mit offenen Zügen, nach oben
  propagiert (Angreifer: ein unsicheres Kind reicht, Verteidiger: alle
  Kinder unsicher). Der Rest ist sicher, bleibt es für immer und wird
  Status. **Keine Wiederholungserkennung, keine GHI:** ein Zyklus ohne
  Gewinn ist einfach nie gewonnen.
- **Zwei Sucher, die sich den Fall teilen**, je Frage als Wettrennen auf
  zwei Threads, der erste Beweis stoppt den anderen:
  1. **Beweiszahl-Abstieg** (`solvePN`): `mid` wie in `wdlpn`, die Kinder
     aus dem Graphen statt aus der TT, Kinder auf dem Pfad zählen als
     Remis (pn ∞, dn 0), gespeicherte Zahlen bei 1022 gestutzt wie der
     `sat`-Codec, lebende Summen und Schwellen nicht. Gut bei Gewinnen.
  2. **Zertifikat-Bauer** (`solveCertificate`): jeder Verteidigerknoten
     bekommt einen Politik-Zug, jeder Angreiferknoten alle; der
     Breitensuche-Abschluss davon ist ein Kandidat, der Fixpunkt prüft ihn;
     hält er nicht, bekommen nur die unsicheren Verteidigerknoten
     (erreichbar über unsichere Knoten) ihren nächsten Zug. Politik
     (`policyScore`): bekannte sichere Stellung oder Terminal-Remis zuerst,
     bekannte Gewinne und Matt zuletzt, dann **bleib stehen** (Chebyshev-
     Abstand jedes eigenen Steins zum nächsten Wurzelfeld seiner Art, −300
     je Schritt), nichts hängen lassen (−2000 je Angreifer-Schlagzug),
     bekannte Stellungen und oft benutzte eigene Platzierungen (+500,
     +Zähler), Schläge +300. Gut bei Remis mit kleiner sicherer Menge.

**Fallen auf dem Weg** (jede hat gemessen Zeit gekostet):

1. Breiten-Widening beider Seiten ohne Fokus = Brute Force mit 40 Runden
   Fixpunkt obendrauf (KQ-K 107 Tsd. Stellungen, 2 s, gegen 17 Tsd.
   Besuche in `wdlpn`). Verworfen.
2. Die Politik darf "ausprobiert" nicht an einer Position in ihrer
   Reihenfolge festmachen, die Reihenfolge ändert sich mit dem Graphen;
   doppelte Kanten ließen den Verlustzähler zu früh feuern (Opposition
   "gewonnen"). Ausprobiert ist die Kindmenge.
3. Die Breitensuche des Abschlusses muss einen Knoten je Welle einmal
   einreihen: ein Verteidiger, der in einer Welle über mehrere Angreifer
   erreicht wird, bekam je Ankunft einen Zug, der Verteidiger war faktisch
   unbeschnitten (falscher Läufer: 3,6 Mio. Stellungen, König auf 64
   Feldern).
4. **Beweiszahlen überzählen auf dem DAG exponentiell** (ein Kind über
   viele Eltern wird jedes Mal summiert): Summen erreichten 2^30 in
   Graphen von 80 Tsd. Knoten. Folgen: `satAdd` darf endliche Summen nie zu
   Inf machen; wo Zahl und Schwelle beide sättigen, gibt es keinen
   Fortschritt mehr (Kind kommt mit denselben Zahlen zurück, Elternknoten
   wählt es wieder). Ein Deckel auf den lebenden Summen (1022) ließ den
   Randbauer hängen (Verteidigerzahlen ununterscheidbar), nur auf den
   gespeicherten Zahlen (wie der Codec) nicht; Log-Maßstab sättigt nie,
   war aber bei Gewinnen zwei- bis dreimal schlechter (KQ-KN 389 Tsd.
   statt 137 Tsd. Expansionen) und sprengte KRR-KN. Rest-Sicherung: erkennt
   `mid` ein Kind, das zweimal mit denselben Zahlen unter denselben
   Schwellen zurückkommt, wird dessen Teilgraph komplett expandiert
   (`exhaust`, Brute Force lokal) und vom Fixpunkt entschieden.
5. Ein Rückgabewert des Kindes (etwa vom Tiefendeckel) muss den
   Elternknoten für die laufende Schleife überstimmen (`local`), sonst
   liest er die unveränderten gespeicherten Zahlen und steigt endlos ab.
6. Beim Wettrennen muss der Verlierer sein Stop-Flag in jeder Expansion
   sehen, nicht nur zwischen Runden, sonst wartet `Solve` auf einen
   Abschluss von Millionen Stellungen.

**Messung** (Arbeitsrechner, Wettrennen, ε = 1, Store 16 Mi Stellungen je
Sucher; "Expansionen" sind Stellungen, deren Kinder erzeugt wurden, das
Gegenstück zu den Besuchen von `wdlpn`):

| Stellung | Ergebnis | Sucher | Expansionen | Stellungen | Zeit | `wdlpn` Besuche |
|---|---|---|---|---|---|---|
| KQ-KQ Dame hängt | Gewinn | pn | 6.079 | 32.192 | 0,25 s | 8.337 |
| KQ-K | Gewinn | pn | 5.446 | 29.062 | 0,24 s | 17.575 |
| KR-K | Gewinn | pn | 57.565 | 129.677 | 0,29 s | 147.822 |
| KQ-KN | Gewinn | pn | 136.751 | 687.451 | 0,92 s | 128.760 |
| KBB-K | Gewinn | pn | 305.441 | 881.137 | 2,3 s | 888.614 |
| KP-K König vor Bauer, Weiß | Gewinn | pn | 8.987 | 41.966 | 74 ms | 16.958 |
| KP-K König vor Bauer, Schwarz | Verlust | pn | 24 + 19.000 | 83.916 | 0,2 s | 88 + 66.509 |
| KBP-K richtiger Läufer | Gewinn | pn | 446 | 2.141 | 53 ms | 446 |
| KQ-KR | Gewinn | pn | 4.794 | 25.077 | 64 ms | 10.337 |
| KQ-KP Springerbauer | Gewinn | pn | 60.126 | 287.702 | 0,6 s | 149.982 |
| KQQ-KN | Gewinn | pn | 4 | 130 | 39 ms | 4 |
| KRR-KN Benchmark, Schwarz | Verlust | pn | 243.459 + 355.435 | 5,1 Mio. | 5,7 s | 406.136 + 442.859 |
| KQ-KRR | Gewinn | pn | 1.756.347 | 5,4 Mio. | 17,4 s | nicht gemessen |
| KP-K Randbauer | Remis | Zertifikat | 1.313 + 896 | 2.294 | 0,3 s | 2.509.289 + 29 |
| KP-K Opposition | Remis | pn | 25.186 + 14.362 | 87.700 | 0,3 s | 2.720.916 + 26.591 |
| KBP-K falscher Läufer | Remis | Zertifikat | 51.597 + 1 | 55.295 | 0,25 s | nicht in 100 s |
| KQ-KP c-Bauer | Remis | pn | 1.765.769 + 54 | 5,6 Mio. | 18 bis 28 s | nicht gemessen |
| KQ-KP a-Bauer | Remis | pn | 1.942.114 + 19 | 5,6 Mio. | 21 s | nicht gemessen |

Die Zeiten der kleinen Fälle enthalten rund 150 ms für das Anlegen der
beiden Stores (je 512 MB), die Messgröße sind die Expansionen.

Lesart:

- **Gewinne:** der Graph braucht etwa gleich viele oder weniger
  Expansionen als `wdlpn` Besuche (KBB-K ein Drittel), weil nichts neu
  erzeugt wird; die Zeit ist ähnlich, der Graph zahlt mit Speicher.
- **Remis mit kleiner sicherer Menge** sind jetzt billig: der Randbauer
  kostet 1.313 Expansionen statt 2,5 Mio. Besuche, das ist exakt die
  sichere Menge (1.378 Stellungen); der falsche Läufer, bei dem `wdlpn`
  scheiterte und Brute Force 25 Mio. Stellungen gegenüberstand, fällt in
  0,25 s mit 55 Tsd. Stellungen. Das ist die Idee des Autors in Zahlen:
  der König pendelt, der Angreifer darf alles, und mehr ist nicht zu
  beweisen.
- **Remis mit großer sicherer Menge** (KQ-KP: 155 Tsd. bis 234 Tsd. sichere
  Stellungen, die Dame treibt den König, der Verteidiger hat viele
  Zustände) löst noch der Beweiszahl-Abstieg in 20 s mit Millionen
  Stellungen; der Bauer verliert dort, weil der Abschluss seiner Politik
  Millionen Stellungen umfasst (die Dame zwingt den König über das halbe
  Brett).

**Offen (nicht in 50 bis 100 s):** KR-KB in beiden Formen (der
Bauer-Abschluss hat 2,6 Mio. Stellungen mit 17 Tsd. Gewinnen, der Läufer
hängt zu oft), KQ-KRB (Gewinn, Stellungslimit 16 Mi erreicht), KBN-K,
KQ-KBN, KNN-K (Brute Force 12,7 Mio. je Frage, 44 s), und die
Fünf-Steiner-Remis KBP-KB ungleiche Läufer, KQ-KRP Festung, KRB-KR
Philidor, KNN-KP. KQ-KRR Dauerschach: die erste Frage (Weiß gewinnt nicht)
widerlegt der Abstieg nach 27 s mit 1,1 Mio. Expansionen (17 Tsd. sichere
Stellungen), die zweite blieb offen. Das sind die Fälle mit großer
sicherer Menge und vielen Verteidigerzuständen; die Testbank hat damit
eine klare Trennlinie: alles bis vier Steine außer KR-KB und KBN-K fällt
in unter einer Sekunde, fünf Steine brauchen die nächsten Hebel.

**Nächste Hebel**, nach erwartetem Nutzen:

1. Politik mit Tauschbewertung (ein hängender Stein ist nur dann schlecht,
   wenn der Schlag nicht zurückgeschlagen wird) und mit "Angreifer-
   Mobilität klein halten"; das trifft KR-KB.
2. Speicher je Knoten: die Go-Slices für Kinder und Eltern kosten
   Allokationen; CSR-Blöcke wie in `matelist` machen den Graphen halb so
   groß und schneller.
3. Beide Fragen in einem Graphen (die Gewinne der einen Frage sind die
   Verluste der anderen), spart die zweite Frage bei Verlusten.
4. Rust-Port, dann die Besuche und Expansionen als Regressionswerte.
