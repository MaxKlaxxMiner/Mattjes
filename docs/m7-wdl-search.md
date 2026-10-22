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

**Politik, zweite Fassung (2026-10-21, danach):** Ein-Zug-Tauschprüfung
(`recapturable`: ein Angreifer-Schlag zählt nur, wenn nicht
zurückgeschlagen werden kann), Angreifer-Mobilität −10 je Zug, der König
bleibt stehen (−300 je Schritt), die übrigen Steine **kleben am König**
(+400 je Stein neben ihm, statt Rückkehr zum Wurzelfeld). Und die
Erweiterung ist fokussiert: der Fixpunkt läuft als Breitensuche und
vergibt jedem unsicheren Knoten seinen Abstand zum Leck (`unsafeLevel`),
erweitert werden nur Verteidigerknoten in den zwei untersten Ebenen über
dem kleinsten Abstand (58 Tsd. statt 1,2 Mio. je Runde bei KR-KB). Die
kleinen Fälle bleiben unverändert (Randbauer 1.358 Expansionen, falscher
Läufer 49 Tsd.).

**KR-KB bleibt offen, und das ist eine Einsicht, kein Fehler:** der
Abschluss der ersten Runde hat 3 Mio. Stellungen mit 16 Tsd. Gewinnen, und
jede weitere Runde bringt mehr Gewinne (112 Tsd. nach sechs Runden), weil
der Turm den Läufer über Schach-Gabeln und Fesselungen gewinnt, was eine
Ein-Zug-Tauschprüfung nicht sieht; der Verteidiger hat dort viele
Zustände, weil der Turm den Läufer von überall angreift. Das Zertifikat
von KR-KB ist inhärent groß (geschätzt mehrere hunderttausend Stellungen),
und wo der Läufer einmal fällt, füllt der Angreifer den ganzen KR-K-Raum.
Für Vier-Steiner beantworten das die Tabellen in Mikrosekunden; die Suche
ist für Festungen und Pendel-Remis mit kleiner sicherer Menge da, und für
Gewinne. Eine taktische Vorschau in der Politik (Schach-Gabeln) ist
notiert, nicht gebaut.

**UCI** `Search fastwdl` ist gebaut (`docs/m6-uci.md`, Abschnitt "Option
Search fastwdl").

**Rust-Port (2026-10-21):** `src/wdllist/`, `src/tests_wdllist.rs`,
`src/uci/wdl.rs`. Der reine Beweiszahl-Abstieg (Testbank-Buchstabe `p`)
ist deterministisch und in Go und Rust zeilenidentisch (zwölf Stellungen,
51 Zeilen diff-frei): das sind die Regressionswerte. Das Wettrennen liefert
dieselben Zertifikatgrößen (Randbauer 1.358 Expansionen, falscher Läufer
49.398), welcher Sucher gewinnt und wie viele Stellungen der Verlierer
noch anlegte, hängt vom Timing ab. In Rust laufen beide Sucher auf
Threads mit 16 MB Stack (der Abstieg rekursiert bis 1024 Ebenen mit rund
4 KB je Ebene), Go-Stacks wachsen von selbst.

Der Port hat einen Fixkostenposten aufgedeckt: Rust brauchte für KQQ-KN
(4 Expansionen) 160 ms je Frage, Go 43 ms, im UCI 6,6 s statt 2,7 s für
39 Züge. Ursache war der Store, der je Frage für 16 Mi Stellungen angelegt
wurde (512 MB): Go bekommt vom Betriebssystem lazy gemappte Nullseiten,
Rusts Allokator nullt. Seitdem **wächst der Store**: Start bei 2^16 Slots,
bei "voll" Faktor 4 bis zum Budget, die Keys liegen je Knoten für das
Umhashen bereit (`Graph.keys`, 16 Byte je Knoten). KQQ-KN damit 4 ms je
Frage in beiden Sprachen und 0,8 s im UCI, und Rust ist jetzt überall
schneller (KBB-K 1,84 s statt 2,54 s, falscher Läufer 81 ms statt 121 ms).

## Runden über die Wurzelzüge: der fortsetzbare Solver (2026-10-21)

Im UCI ging die Zeit in Wurzelzug-Reihenfolge weg: der erste offene Zug
fraß das ganze Budget, und Arena sah bei MultiPV nichts von den anderen.
Der Treiber (`uci/wdl.go`, `src/uci/wdl.rs`) läuft jetzt in **Runden über
alle offenen Züge** mit einem Stellungsbudget je Graph, das von 2^16 um
Faktor 4 je Runde bis zum `Hash`-Budget wächst (`wdlFirstBudget`,
`wdlBudgetGrowth`). Die erste Fassung baute die Graphen je Runde neu
auf; das kostete bei KRR-KN nicht das erwartete Drittel, sondern verdoppelte
die Zeit (25 s statt 15 s für elf Verluste), weil die Verluste erst in der
dritten Runde fallen und die ersten beiden Runden je Frage komplett
verloren waren.

Deshalb ist eine Frage jetzt ein **`Solver`-Objekt** (`NewSolver`,
`Run(maxPositions)`, `Positions()`): beide Sucher des Wettrennens leben
darin weiter, `Run` setzt das Rennen bis zum nächsten Budget fort, eine
Entscheidung gibt die Graphen frei. `Solve` ist ein einzelner `Run`. Damit
das Fortsetzen korrekt ist, gilt:

- **Eine Expansion ist atomar.** Das Budget wird vor dem Anfassen eines
  Knotens geprüft (`Graph.full`), nie mitten in der Kinderliste; der Store
  darf bis Budget plus `chess.MaxMoves` wachsen (`setBudget`), damit
  `getOrPut` unterhalb des Budgets nie "voll" meldet. Vorher ließ der
  Abbruch halb expandierte Knoten zurück (Flag gesetzt, Kinder fehlen),
  die der Abstieg nie nachexpandiert hätte.
- **Die Breitensuche räumt beim Abbruch auf.** `closure` ließ die
  `flRegion`-Markierungen der nächsten Welle und des Rests der laufenden
  Welle stehen; `unsafeDefenders` hätte diese Knoten beim Fortsetzen als
  schon besucht übersprungen. Jetzt werden beide Listen am Ende entmarkiert.
- **Die Warteschlange der unterbrochenen Breitensuche ist weg**, die
  angelegten Knoten nicht: ein fortgesetzter `runCert` startet die
  nächste Welle mit `openUnexpanded` (alle offenen Knoten ohne Kinder,
  erreichbar über offene Knoten), teilexpandierte Verteidiger behalten
  ihren Zug.
- Der Abstieg braucht nichts: `mid` räumt die Pfadflags auf jedem
  Abbruchweg auf, die gespeicherten Zahlen bleiben, der nächste `Run`
  steigt von der Wurzel ab.
- Die Stop-Flags des Rennens werden je `Run` neu angelegt (Go: lokale
  Atomics, deren Zeiger in die Optionen der Sucher gesetzt werden; Rust:
  `Options.race_stop` je Sucher).

Ergebnis (Arbeitsrechner, `go movetime 40000`, Hash 1024, MultiPV 64):
KRR-KN mit Schwarz am Zug hat nach **0,4 s acht Verluste** (Runde 1 mit
64 Ki Stellungen je Graph), nach 14,6 s elf (wie vorher seriell), und
**alle zwölf nach 21 s** (Go) bzw. **11 s** (Rust); Nh2 war vorher nie
entschieden worden, siehe unten. KQ-KRR Dauerschach hat nach 25 s zehn
Verluste und drei "kann nicht gewinnen" (Rust; vorher zwei Verluste und
zwölf offene Züge). Zwei Züge bleiben dort offen. `nodes` zählt jetzt die
Stellungen aller Graphen eines Zuges (beide Fragen, beide Sucher), nicht
mehr nur die des entscheidenden Graphen; KQQ-KN meldet 743 Tsd. statt
84 Tsd. bei unveränderten 0,8 s.

Der Speicher ist der Preis: jede offene Frage hält zwei Graphen, bis zu
`Hash`·2^20/64 Stellungen je Graph, über alle offenen Züge hinweg. Bei
KRR-KN in Runde 4 (4 Mi Stellungen je Graph, vier offene Züge) sind das
bis zu 16 Graphen; die wachsenden Stores halten den Verbrauch bei dem,
was wirklich angelegt wurde. `Hash` als pauschales Budget über alle
Graphen ist für später notiert.

Die Suche endet außerdem, sobald so viele Gewinne bewiesen sind, wie
MultiPV Zeilen hat (nichts kann über einem Gewinn stehen); bei MultiPV 1
also mit dem ersten Gewinn.

**Fund dabei: eine Schwelle ist nie eine Entscheidung.** Bei KRR-KN nach
Nh2 drehte der Abstieg leer: 25 Mio. Besuche je fünf Sekunden ohne eine
Expansion, Wurzel `pn 112 dn 1073741824`, also dn = Inf. Ursache: `mid`
gab am Tiefendeckel (1024) und bei "bestes Kind ist Wiederholung" die
Schwellen als Zahlen zurück, und die Schwellen sind an der Wurzel Inf/Inf
und werden entlang einzelner Kinder (`second` = Inf, also `bound` = Inf)
unverändert nach unten gereicht. Ein Inf in der Rückgabe machte die Summe
des Elternknotens Inf, `clip` speichert Inf als Inf, und an der Wurzel
galt dann `dn >= thDn` sofort: jeder Abstieg kehrte nach einem Besuch
zurück, die äußere Schleife lief endlos. Jetzt deckelt `finite` jede als
Zahl zurückgegebene Schwelle bei Inf − 1 (Go und Rust). Der reine Abstieg
(`px`) ist über die 14 schnellen Stellungen weiter zeilenidentisch zwischen
Go und Rust, und Nh2 ist seitdem als Verlust bewiesen (die Frage "Weiß
gewinnt nach Nh2" per Abstieg mit 3,2 Mio. Stellungen).

Zweiter Fund: der Abstieg meldete **STUCK (bug)**, wenn das Budget mitten
in einem `exhaust` ausging (das Kind ist dann weder gewonnen noch sicher,
was ohne Abbruch unmöglich wäre). Das war ein Fehlbericht und hätte mit
dem Solver die Frage fälschlich beendet; jetzt wird nach dem `exhaust` erst
auf Abbruch geprüft.

## Der einsame Springer: Fragereihenfolge und blanker König (2026-10-21)

Frage des Autors an `8/8/4k3/8/8/8/R7/K1RN1n2 w - - 0 1` (KRRN gegen KN,
Weiß am Zug, alle 27 Züge gewinnen laut Stockfish/Syzygy): warum steht da
lange `cp 1`, wo Schwarz mit einem Springer doch nicht gewinnen kann?
Müsste es nicht sofort `cp 10` sein?

Die Antwort hat einen Grundsatz und zwei Hebel. Der Grundsatz: `cp 10`
zeigt die Engine nur, wenn "der Gegner gewinnt nicht" **bewiesen** ist,
und das ist beim einsamen Springer kein Materialsatz. Ein Springer kann
mattsetzen, wenn die eigenen Figuren dem König die Felder nehmen
(ersticktes Matt: Ka1 mit eigenen Steinen auf a2, b1, b2, Sc2#); mit zwei
Türmen und Springer neben dem König ist das sogar nah. Der Terminal "totes
Material" (FIDE 5.2.2) greift nicht, der Zertifikat-Bauer muss die
geschlossene Menge wirklich finden. Das ist billig: nach Ne3 widerlegt der
Abstieg "Schwarz gewinnt" mit 160 Tsd. Stellungen in 0,18 s, die sichere
Menge hat 1.003 Stellungen. Im UCI dauerte es trotzdem, aus zwei Gründen:

1. **Fragereihenfolge.** Der Treiber fragte je Zug zuerst "gewinnen wir"
   (KRRN gegen KN: 2,2 Mio. Stellungen, Matt in 108 Halbzügen) und erst
   dann "gewinnt der Gegner". Die teure Frage fraß je Runde das Budget,
   bevor die billige drankam. Jetzt kommt **die Frage der materiell
   schwächeren Seite zuerst** (`wdlMaterial`: Dame 9, Turm 5, Läufer und
   Springer 3, Bauer 1, ohne König; bei Gleichstand wie bisher "gewinnen
   wir" zuerst). Beim KRR-KN-Benchmark mit Schwarz am Zug war das schon
   die alte Reihenfolge.
2. **Blanker König als Terminal.** Ein Angreifer, der nur noch den König
   hat, kann nie Schach geben, also nie mattsetzen: `add` setzt solche
   Knoten auf `stDrawT` ("Verteidiger hält"), ohne Suche, in beiden
   Sprachen. Vorher musste der Fixpunkt für jede Stellung nach dem
   Springerverlust eine geschlossene Menge aus Königswanderungen und
   Turm-Pendeln bauen. Der Beweis nach Ne3 kostet damit 44.781 statt
   160.291 Stellungen (331 statt 1.003 sichere), und die Fragen mit
   blankem König an der Wurzel (KP-K Randbauer, Opposition, Frage 2) sind
   eine Stellung groß.

Dabei fiel ein latenter Fehler: eine Wurzel, die selbst Terminal ist
(jetzt der blanke König, vorher schon eine mattgesetzte Wurzel), ließ
`runPN` endlos kreisen, weil die Schleife nur `stSafe` als Abbruch
kannte, und `runCert` meldete STUCK. Beide prüfen jetzt `defenderHolds`,
wie das Ergebnis.

Messung (Arbeitsrechner, `go movetime 30000`, Hash 1024, MultiPV 64):

| | vorher | jetzt Go | jetzt Rust |
|---|---|---|---|
| erstes `cp 10` | 2,7 s | 0,34 s | 0,17 s |
| "kann nicht verlieren" nach 4,5 s | 1 Zug | 10 Züge | |
| nach 30 s | ~5 von 27 | 25 von 27 | 25 von 27 |
| erster bewiesener Gewinn (Rc3) | – | – | 23 s |

Die zwei übrigen Züge brauchen mehr als 1 Mi Stellungen je Graph für die
Widerlegung; dort kleben die Türme nach der Politik am König
(`gluedPieces` +400), was gegen einen Springer das erstickte Matt
vorbereitet. Die Politik braucht für Springer-Angreifer ein Maß für freie
Königsfelder statt Klebens; notiert, nicht gebaut. Die zwei Stellungen
stehen als "KRRN-KN lone knight, white/black" in `WdlPositions` (31
Stellungen jetzt) und `test-positions.md`.

Offen: der Stop kommt in Rust bis zu zwei Sekunden nach dem Zeitende,
weil `safeSet` über einen großen Graphen das Flag nicht pollt.

## Ein geteilter Graph je Frage für alle Wurzelzüge (2026-10-22)

Bis hierher baute der Treiber je Wurzelzug und Frage ein eigenes
Graphenpaar: bei 27 Zügen bis zu 108 Graphen, die alle dasselbe Material
anlegten (die Kinder von Ta3 und Tc2 überschneiden sich massiv), 46 Mio.
Stellungen in 30 s für die Springer-Stellung des Autors. Jetzt gibt es
**zwei `Solver` für die ganze Suche**, einen je Frage ("wir greifen an",
"der Gegner greift an"), jeder mit seinem Graphenpaar (Abstieg und
Zertifikat-Bauer), die Wurzel ist Knoten 0 und die Wurzelzüge sind ihre
Kinder:

- `Solver.Run(extra)` fragt die Wurzel selbst (Testbank, `Solve` ist ein
  einzelner `Run`), `Solver.RunMove(m, extra)` das Kind nach dem Zug.
  `locate` findet den Frageknoten über den Key und expandiert die Wurzel
  in jedem Graphen beim ersten `RunMove` (im Zertifikat-Graphen komplett,
  nicht mit einem Politik-Zug).
- Das Budget ist ein **Zuwachs je Lauf**: `begin(extra)` setzt die Grenze
  auf Größe plus `extra`, gedeckelt durch `Options.MaxPositions` je
  Graph. Jeder Zug bekommt in einer Runde dieselbe Zahl neuer Stellungen,
  die Runden wachsen weiter um Faktor 4. Die Suche endet, wenn eine Runde
  weder den Graphen wachsen ließ noch einen Zug entschied (`progressed`).
- Alle Knotenfunktionen, die bisher bei 0 begannen, nehmen den
  Frageknoten: `fillResult(r, n)`, `reachableSafe(from)`,
  `unsafeDefenders(from)`, `openUnexpanded(from)`, `line(n)`. Der
  Fixpunkt `safeSet`, `levels` und `resetNumbers` bleiben global, was
  korrekt ist (sicher ist monoton und gilt für jeden Frageknoten).
- `rootDefender`, der Bezug der Bleib-stehen-Politik, wird je Lauf auf
  die Stellung des Frageknotens gesetzt.
- Die Frontier des Zertifikat-Bauers wird je Lauf aus `openUnexpanded(n)`
  gebildet, weil eine andere Frage inzwischen Knoten unter n expandiert
  haben kann.

**Was es bringt** (Arbeitsrechner, Hash 1024, MultiPV 64):

| | je Zug eigene Graphen | geteilt, Go | geteilt, Rust |
|---|---|---|---|
| KRRN-KN Springer, bewiesene Gewinne nach 30 s | 0 (Go) / 1 (Rust) | 9 | 18 |
| erster Gewinn | 23 s (Rust) | 17,2 s | 9,9 s |
| Stellungen nach 30 s | 26 Mio. | 18 Mio. | 26 Mio. |
| KRR-KN, alle zwölf Verluste | 12,7 s / 8,5 s | 7,7 s | 5,4 s |
| Stellungen dafür | 12 Mio. | 6,2 Mio. | 6,3 Mio. |
| KQ-KRR Dauerschach nach 25 s | 10 Verluste, 1 offen | 10 Verluste, 0 offen | 10 Verluste, 0 offen |

Die Gewinne kommen in Kaskade (Go: 17,2 s, 17,9 s, 19,7 s, 20,3 s, 20,6 s,
21,0 s, 21,2 s, 21,5 s), weil der zweite Zug die Gewinne des ersten im
Graphen vorfindet. Der reine Abstieg (`px`) über die 14 schnellen
Stellungen ist unverändert und zeilenidentisch Go = Rust.

**Falle dabei in Rust:** `unsafe_defenders` setzte die `FL_REGION`-Marke
nicht beim Einreihen, sondern nach jedem Knoten über die ganze Liste der
bisher Markierten, um den Borrow-Checker zu umgehen. Das war quadratisch,
fiel mit kleinen Graphen nicht auf und stand mit dem geteilten Graphen
nach einer halben Sekunde still (keine zweite Runde, kein `bestmove`).
Jetzt Index-Schleife und Marke sofort, wie in Go.

Speicher: vier Graphen insgesamt, jeder bis `Hash`·2^20/64 Stellungen;
damit ist `Hash` zum ersten Mal eine echte Obergrenze (mal vier).

## Ehrliches Hash, Tabellen-Orakel und der Dauerschach-Befund (2026-10-22)

**Speicher gemessen:** ein Einzelgraph (reiner Abstieg, KQ-KP c-Bauer,
4,05 Mio. Stellungen) belegt 1.037 MB in Go und 968 MB in Rust, also
rund **250 Byte je Stellung** (32 Byte Satz, 16 Byte Key, 48 Byte
Slice-Köpfe für Kinder und Eltern an jedem Knoten, Kanten mit
Wachstumsreserve, Store mit 16 Byte je Slot bei 75 % und Faktor-4-Sprung).
Das Budget rechnete mit 64. Jetzt ist `Hash` die harte Obergrenze:
`Hash`·2^20 / (4 Graphen · 256 Byte) Stellungen je Graph
(`wdlBytesPerPosition`, `wdlGraphs`), Hash 16384 erlaubt 16 Mi je Graph.
Die Vorgabe 256 MB reicht für die WDL-Suche damit nicht mehr; in Arena
Hash auf 8192 oder 16384 setzen. Die Bytes zu senken (Key aus dem Satz
rekonstruieren, Kanten-Arena statt Slices, Store ohne Sprung; Ziel rund
150) ist notiert, nicht gebaut.

**Tabellen als Orakel in `wdllist`** (`Options.Oracle`, nil = totes
Material): `add` nimmt das Verdikt der Tabellen für die Seite am Zug als
Terminal (`stWin`/`stLoss`/`stDrawT`, nie expandiert), `levels` nimmt
Tabellengewinne als Saat der Ebene 1, die Politik bewertet
Tabellenstellungen mit ±100000. Im UCI ist das `mateab.Tables` mit dem
geladenen Set, also die Vier-Steiner-Basis plus die erreichbaren
Cache-Dateien; Testbank-Buchstabe `t` (Basis). Wirkung:

| | ohne Orakel | mit Orakel |
|---|---|---|
| KQ-KRR Dauerschach, Frage "Weiß gewinnt" | 1,6 Mio. Stellungen per Abstieg | **1.179 per Zertifikat** |
| KQ-KRR Dauerschach im UCI, elf Verluste | nach 25 s | in Runde 1 |
| KRR-KN Benchmark im UCI (Tabelle `5-KRRKN.bin` im Cache) | 7,7 s / 5,4 s | **52 Stellungen, 0,0 s** |
| KRRN-KN Springer im UCI, alle 27 Gewinne | 18 von 27 nach 30 s (Rust) | **27 in 3,0 s** |

Die Fünf-Steiner-Tabellen im Cache (KRRKN, KRNKN, KRRKB...) sind für die
Sechs-Steiner-Stellung das, was die Basis für Fünf-Steiner ist: jeder
Schlag landet in einem Terminal, der Graph der Springer-Stellung schrumpft
von 26 Mio. auf 1 Mio. Stellungen.

**Politik: Schachs** bekommen +1500 (`policyScore`), damit der Zirkel
eines Dauerschachs die Kleb- und Bleib-stehen-Terme schlägt. Ohne
Wirkung auf Randbauer, Opposition, falschen Läufer (1.358 / 25.186 /
49.398 Expansionen wie vorher, Go = Rust).

**Der Dauerschach-Befund.** Die vier Remiszüge von `3k4/8/8/8/8/8/5rr1/K6Q
w` (Dh4+, Dh8+, Dd1+, Db1 laut Stockfish/Syzygy) bleiben offen, auch mit
Orakel: die Frage "Schwarz gewinnt nach Dh8+" läuft in beiden Suchern bis
16 Mi Stellungen. Der Dump des Zertifikat-Graphen zeigt, dass die Politik
richtig schacht (jeder weiße Knoten hat ein Schach gewählt und weitere
untried), das Leck aber 9 bis 25 Halbzüge tief in der Schachkette liegt:
der schwarze König wandert, die Türme ziehen zwischen, die Region hat
hunderttausende Stellungen, und irgendwo ist der gewählte Schach der
falsche. Drei Versuche an einem Nachmittag, alle wieder ausgebaut:

1. **Begrenzte Wellen** (Breitensuche bis 2^16 Expansionen, dann
   Fixpunkt): der Bauer erreicht Fixpunkte (nach Runde 1 1.882 sichere
   Stellungen), aber die Wurzel wird nie sicher, und der falsche Läufer
   kostet 83 Tsd. statt 49 Tsd. Expansionen, weil frühe Fixpunkte die
   Politik umlenken (bekannt-sichere Kinder +100000).
2. **Expansion nach Kosten** (Heap, Angreiferknoten mit wenigen Antworten
   zuerst, teure bleiben als Leck unexpandiert) mit Kostenschwelle und
   Erweiterung am Leck: die Runden halten ihre Grenzen, der Graph wächst
   trotzdem auf Millionen, weil die Erweiterung in einer Region, in der
   alles unsicher ist, nicht weiß, welcher Verteidigerknoten den
   falschen Zug gewählt hat; mit Schwelle 8 blieben außerdem Angreifer mit
   zehn Zügen (KP-K!) komplett liegen, Randbauer fiel an den Abstieg,
   falscher Läufer kam in 100 s nicht mehr.
3. **Erweiterung nach Politik-Score** des nächsten unprobierten Zugs
   (ein weiteres Schach vor jedem stillen Zug, global): trifft irgendwo,
   nicht am Leck.

Die Einsicht: ein Dauerschach-Zertifikat ist ein geschlossener Zyklus,
und das Leck eines Zyklus ist nicht lokal erkennbar; die Reparatur muss
den Zug finden, nach dem der König entkommt, das ist eine
Tiefensuche-Frage (wo endet die Schachkette?), keine Breitensuche-Frage.
Ein Ansatz dafür: den Zertifikat-Bauer von der Schachkette her bauen
(Schachs als Pfad verfolgen, bei Kettenende zurück und den letzten Schach
tauschen, df-pn-artig mit Schachs als einzigen Verteidigerzügen), also
ein eigener "Dauerschach-Sucher" neben Abstieg und Zertifikat. Notiert.

Dazu wieder gelernt: ohne Tabellen ist die Testbank für Fünf-Steiner-Remis
blind, weil jeder Turmzwischenzug ein KD-KT-Gewinn ist, den der
Zertifikat-Bauer nicht beweisen kann (er sucht nur Remis); mit `t`
stimmt die Messung.

## Syzygy-WDL als Orakel (2026-10-22)

Die Erzeugung nach Bedarf lief während einer WDL-Suche nicht an: nur
`reporter.progress` klopfte den Bedarfs-Tick an, der WDL-Treiber meldet
aber über `text` und `note`. Jetzt klopft auch `note` (jede Rundenzeile).
Dazu meldete der Treiber `answered`, wenn die Graphen voll waren, und
pausierte damit die Erzeugung des Wurzelmaterials, obwohl Züge offen
blieben; `answered` gilt nur noch, wenn alle Züge entschieden sind. Mit
`EgtbGenerate5` an läuft KRRKQ neben der Dauerschach-Suche, und nach der
Übernahme stehen alle vier Remis als `cp 0` (Go und Rust).

Der eigentliche Schritt: **Package `syzygy`** (Go `syzygy/`, Rust
`src/syzygy/`), ein Leser für Syzygy-WDL-Dateien (`*.rtbw`) als Port von
Fathom (`tbprobe.c`, MIT, Ronald de Man / basil00 / Jon Dart; Kopie in
`tests/fathom/`). Nur WDL: Gewinn, Remis, Verlust für die Seite am Zug,
keine Distanz, keine DTZ. Aufbau:

- `Open(dir)` liest die Dateinamen (`KQvKRR`), legt je Material einen
  Eintrag unter dem Schlüssel beider Farbzuordnungen an (Flip-Flag) und
  prüft die Dateigröße (`size & 63 == 16`, sonst unvollständig). Die
  Datei wird beim ersten Zugriff per Memory-Map geladen (Go `syscall`,
  Rust mit zwei von Hand deklarierten Kernel-Funktionen, kein Crate) und
  ihr Kopf gelesen: je Kodierung die Figurenreihenfolge und die
  Mischradix-Faktoren (`initEncInfo`), die Huffman-Symbole mit ihren
  Lauflängen (`setupPairs`), dann Indextabelle, Größentabelle und die
  Code-Blöcke (64-Byte-ausgerichtet; die Map ist seitenausgerichtet, also
  gelten Dateioffsets wie Adressen).
- `encode` berechnet den Index einer Stellung: Spiegelungen auf das
  Dreieck a1-d1-d4, Königspaar-Index (462) oder die drei führenden
  Figuren (31332), bei Bauern der führende Bauer nach Linie, jede weitere
  Gruppe gleicher Figuren als Binomialplatzierung auf den freien Feldern.
  Felder sind Syzygy-Felder (a1 = 0), unsere sind a8 = 0: `sq ^ 0x38`.
- `decompress` sucht den Block über Index- und Größentabelle und
  dekodiert die kanonischen Huffman-Codes (64-Bit-Register), bis die
  Position im Lauf erreicht ist, und expandiert das Symbol rekursiv zum
  Wert. **Falle:** der Dekoder liest bis zu vier Byte über das Blockende
  hinaus (Vorauslesen), Fathom liest in die Datei hinein, die dafür 16
  Byte Reserve am Ende hat; ein Datenbereich, der exakt am letzten Block
  endet, stürzt ab (bei KQvKP gefunden, weil dort der letzte Block
  getroffen wurde). Jetzt reichen die Slices bis zum Dateiende, das letzte
  Wort wird gepolstert.
- `ProbeWDL` löst erst alle Schläge auf (`probeAB`, Alpha-Beta nur über
  Schlagzüge, En passant getrennt mit dem Patt-Sonderfall), weil die
  Tabellen dort "egal"-Werte speichern, und fragt dann die Tabelle.

**Verifikation:** `syzygyVerify` würfelt legale Stellungen je Material
und vergleicht mit unseren eigenen Tabellen (Basis und Fünf-Steiner-
Cache): 25 Materialien × 20.000 Stellungen, alle gleich (Go und Rust,
Dreisteiner bis Fünfsteiner, mit und ohne Bauern, beidseitige Bauern);
`syzygyPositions` sondiert die Wurzelzüge aller 32 WDL-Teststellungen und
vergleicht die Verteilung mit Stockfish: alle gleich, auch die
Sechs-Steiner. Sondierrate 0,8 bis 1,8 Mio./s bei kleinen Tabellen,
45 bis 90 Tsd./s bei Fünf-Steinern mit kaltem Dateicache.

**Einbindung:** `mateab.Tables` hat ein optionales Feld `WDL`
(Rust `wdl: Option<&Tables>`): erst die eigenen Tabellen (exakt), dann
Syzygy mit Distanz `1..Unbounded` (cursed win = Gewinn, blessed loss =
Verlust, die Fünfzig-Züge-Regel bleibt ignoriert), dann totes Material.
Damit gilt es für alle Suchen. UCI-Option `SyzygyPath` (String, Vorgabe
leer; Rust teilt die Tabellen als `Arc` mit dem Such-Thread),
Testbank-Buchstabe `s` (Ordner des Autors).

| | ohne Syzygy | mit Syzygy |
|---|---|---|
| KQ-KRR Dauerschach (5 Steine), alle 15 Züge | 11 Verluste, 4 offen | 4 Remis, 11 Verluste in 2 ms |
| KRRN-KN Springer (6 Steine), 27 Gewinne | 3,0 s mit Cache-Tabellen | 2 ms |
| KRRN-KNP (7 Steine, neue Teststellung), 27 Gewinne | – | 2,6 s Go, 1,5 s Rust, 640 Tsd. Stellungen |

Mit allen Sechs-Steinern ist jede Stellung bis sechs Steine ein
Tabellentreffer und die Suche beginnt bei sieben; die Sieben-Steiner-
Stellung (Springer-Stellung plus Bauer f7, Stockfish: 27 Gewinne) ist in
`WdlPositions` die erste, die mit Syzygy noch Suche ist.

## Der Dauerschach-Sucher: die Schachkette als Pfad (2026-10-22)

Umsetzung der notierten Idee als dritter Mechanismus im Zertifikat-Bauer
(`wdllist/chase.go`, Rust `chase.rs`), auf dessen Graphen und Thread:
die **Jagd** (chase). Sie verfolgt die Schachkette als Pfad in die Tiefe,
der Verteidiger nur mit Jagdzügen (Schachs, Schläge und Umwandlungen,
die mit Tabellen meist Terminals sind, und Züge in Knoten, die bekannt
halten), der Angreifer mit allen Antworten; endet eine Kette, geht sie
zurück und tauscht den letzten Schach. Eine Wiederholung eines Knotens
auf dem Pfad schließt die Kette. Jeder berührte Knoten liegt im Graphen
(Angreifer komplett expandiert, Verteidiger mit den probierten Schachs),
der Fixpunkt bleibt der Richter. Wo die Kette scheitert, bekommt der
Knoten wie bisher seinen Politik-Zug, Breitensuche-Abschluss und
Erweiterung laufen unverändert.

**Exakte Holds mit Tarjans Lowlink.** Der erste Entwurf memoisierte
jeden "hält" und produzierte 97.000 Holds, die der Fixpunkt verwarf: eine
Kette, die auf einem Vorfahren schließt, hält nur, wenn der Vorfahre am
Ende hält (das GHI-Problem). Jetzt trägt jeder Hold die Tiefe des
höchsten Vorfahren, den seine Wiederholungen erreichen. Ist das die
eigene Tiefe, ist die Struktur in sich geschlossen und der Hold endgültig
(`chHold`); sonst ist er bedingt (`chHoldCond`) und steht in der Liste
dieses Vorfahren: schließt der in sich, werden alle gelisteten Knoten
endgültig, schließt er bedingt, wandern sie zu seinem Vorfahren,
scheitert er oder wird er geschnitten, werden sie vergessen. Fehlschläge
sind pfadunabhängig (jeder Schach hat eine Antwort, nach der der König
entkommt, egal woher der Knoten kam) und endgültig (`chFail`); Schnitte
merken sich die Resttiefe (`chCut`) und werden nur mit mehr Tiefe erneut
gejagt. Seitdem verwirft der Fixpunkt keinen Hold mehr (0 "forgotten" in
allen Läufen), die Rücknahme bleibt als Versicherung.

**Ordnung und Vertiefung.** Der Angreifer probiert die Antworten
"Fehlschlag zuerst": bekannte Fehlschläge, dann die mit den wenigsten
Schachs danach (die Flucht), neue Stellungen vor bekannten, bekannte Holds
zuletzt; eine scheiternde Kette muss billig scheitern, weil darüber jeder
Schach jedes Knotens an die Reihe kommt. Der Verteidiger wählt den Schach
nach einer Zwei-Halbzug-Vorschau (`lookAhead`), die den Schluss sucht:
8 × die wenigsten Schachs nach der schlechtesten Antwort, minus 40 je
Antwort in eine neue Stellung, minus 20 je Antwort in eine bekannte
offene; Schachs, nach denen eine Antwort für den Angreifer gewinnt
(Tabellen, Graph, Memo), fallen weg. Die Jagd vom Frageknoten vertieft
iterativ (Deckel 16, 32, 64, 128, 256 Halbzüge; der Memo behält, was die
flachen Jagden entschieden haben): mit festem Deckel 256 lief die Kette
hunderte Halbzüge ohne Wiederholung (der König wandert über das Brett),
und die Schnitte vervielfachten sich mit jedem alternativen Schach
darüber (1,47 Mio. Schnitte bei 164 Tsd. Holds).

**Wo gejagt wird.** Vom Frageknoten immer (unbegrenzt), an den
Politik-Knoten der Wellen nur, wenn der Verteidiger materiell schwächer
ist (Q9/R5/B3/N3/P1), und dann mit 4.096 Jagdknoten Budget: das
Dauerschach ist das Mittel der schwächeren Seite, und die Jagd an jedem
Wellenknoten einer stärkeren Verteidigung (KRR+PP gegen KQ, Frage "Weiß
gewinnt") lief mit den Turmschachs gegen den weißen König auf 5,8 Mio.
Stellungen, wo der Abstieg die Frage in 31 ms entschied.

**Zwei Fallen dabei.** `link` (eine Kante mit Zählern) rief `propagate`
nach jeder neuen Kante, auch wenn der Knoten schon entschieden war, und
jede Wiederholung nahm den Eltern einen Zug vom Zähler: nach Qh4+ Rg5
stand der Angreiferknoten als verloren, das Zertifikat hatte sechs
Stellungen und der König ein Feld. Jetzt wird genau beim Statuswechsel
propagiert (`expandPartial` nutzt dasselbe `link`). Und die Pfad-Prüfung
muss vor der Memo-Abfrage stehen: ein Knoten auf dem Pfad trägt noch den
Memo seiner letzten Jagd, ein `chCut` dort verbarg die Wiederholung.

**Ausgebaut und verworfen:** die Antwort-Ordnung nach der Distanz des
Königs zu den eigenen Steinen (die Fehlschlag-Blätter zeigten immer den
König hinter einem Turm, die Ordnung änderte nichts: 1,18 statt 1,15 Mio.
Knoten); ein Schnitt, der den Knoten sofort beendet statt Alternativen zu
probieren (die Ketten schlossen bis Deckel 128 nicht); und der
**beschränkte Abstieg**: `mid` auf dem Zertifikat-Graphen mit
Jagdzügen als einzigen Verteidigerzügen, dn = 0 ruft den Fixpunkt, pn = 0
heißt "der Angreifer entkommt jeder Kette" (hielt KQP-KRRP in 101 ms mit
24 Tsd. Besuchen, trashte aber auf KQ-KRR mit 4,2 Mio. Besuchen bei
245 Tsd. Expansionen ohne einen Fixpunkt, die bekannte DAG-Sättigung).

**Messung** (Zertifikat-Bauer, Jagdknoten in Go und Rust identisch):

| Stellung | Frage | vorher | mit Jagd |
|---|---|---|---|
| KQ-KRR Dauerschach (5 Steine, Vier-Steiner als Orakel) | Schwarz gewinnt | offen bei 16 Mi | widerlegt (Deckel 64), Zertifikat 78.280 (König auf 60 Feldern, 65 Verteidiger-Stellungen), 3.266.718 Jagdknoten, 1.453.445 Stellungen, Go 58,7 s / Rust 22,0 s |
| KQP-KRRP vier Remis (7 Steine, Syzygy) | Schwarz gewinnt | – | widerlegt, Zertifikat 3.467 (König auf 57 Feldern), 123.800 Jagdknoten, 165.784 Stellungen, Go 1,9 s / Rust 0,96 s |
| KP-K Randbauer, KBP-K falscher Läufer (ohne Tabellen) | Weiß gewinnt | 1.423 / 53.048 sicher | 1.410 / 54.262 sicher (die Jagden scheitern sofort, der blanke König hat keinen Schach) |

Das Zertifikat der KQ-KRR-Stellung erklärt, warum die Politik dort
scheiterte: es ist kein Käfig, der König läuft über 60 Felder, und die
Dame hat trotzdem immer ein Schach; eine Breitensuche-Politik mit einem
Schach je Knoten findet diese Menge nur mit der richtigen Wahl an jedem
der 13.833 Angreifer-Stellungen. Zwischen Jagd (1,45 Mio. Stellungen)
und Zertifikat liegt noch Faktor 19; vor der Pfad-Prüfung-vor-Memo-
Korrektur hatte die Jagd mit 28.712 ein kleineres Zertifikat gefunden
(Zufall der Ordnung, die Korrektur ist trotzdem richtig).

**Drei neue Sieben-Steiner** in `WdlPositions` (Stockfish 19 mit Syzygy,
20 s): `KQP-KRRP perpetual, four draws` (`3k4/p7/8/8/8/8/P4rr1/K6Q w`,
0/4/13), `KQ-KRRPP perpetual, one draw` (`3k4/p3p3/8/8/8/8/5rr1/K6Q w`,
0/1/14, nur Qh8+) und `KQP-KRRP perpetual, two draws`
(`3k4/p7/8/8/8/7P/5rr1/K6Q w`, 0/2/8, Qb1 und Qd1+). Die beiden
letzten bleiben **offen**: ihr Remis ist kein reines Dauerschach. Nach
Qh8+ Kd7 hat Weiß keine schließende Schachkette (die Jagd scheitert
nach 121 Tsd. Knoten, Stockfish bestätigt es: die Remislinie Qh3+ e6
Qh7+ Kd6 Qd3+ Ke5 Qe3+ Kf5 Qd3+ Kf6 Qd8+ Kg7 **Qd7** braucht einen
stillen Zug), und Qb1 ist still. Dort muss die Politik den stillen Zug
finden und die Jagd darunter die Ketten; die erste Breitensuche-Welle
endet aber nicht (4 Mio. Jagdknoten, der schwarze Bauer läuft bis e4),
weil eine Politik ohne Schluss den ganzen Raum aufzieht. Das ist die
nächste Baustelle, mit den beiden Stellungen als Maßstab.

## Gemischte Remis: freie Ausweichzüge, stille Züge am Jagdstart (2026-10-22)

Der Versuch, die beiden offenen Sieben-Steiner mit stillen Zügen in der
Jagd zu lösen. Ergebnis vorweg: die reinen Dauerschachs werden billiger
(KQ-KRR halbiert), die beiden offenen Stellungen bleiben offen, und der
Grund ist jetzt bekannt: ihr Remis ist überhaupt keine Schachkette.

**Was die Stockfish-Linien zeigen** (`tests/sf-pv.sh`, 20 bis 30 s):
nach Qh8+ Kd7 in `KQ-KRRPP` laufen die Linien als Schachreihen mit
stillen Damenzügen dazwischen (Qd7, Qd4, Qc4 als Fesselung), und
zwischendurch geben die Türme dem weißen König Schach (Rg1+ Kb2 Rf2+
Ka3 ...), der ausweichen muss. Nach Qb1 in `KQP-KRRP` ist es gar kein
Dauerschach: der h-Bauer läuft h4 h5 h6 h7, Schwarz muss einen Turm
gegen ihn geben (Kc7 h4 Rc2 h5 Rh2 h6 a6 h7 Rc5 Qe1 Rhxh7), und jede
Linie endet in einer Syzygy-Stellung. Das erste ist ein dynamisches
Remis aus Schachs und Drohungen, das zweite ein Bauernlauf; beide
Zertifikate enthalten an jedem stillen Zug alle rund 40 Antworten des
Angreifers statt der fünf nach einem Schach.

**Drei Regeln in der Jagd**, davon zwei behalten:

1. **Ausweichen ist frei.** Steht der Verteidiger im Schach, sind alle
   seine Züge Jagdkandidaten (erzwungen, wenige, kein eigenes Tempo), und
   die Gegenschachs des Angreifers werden in der Antwortordnung nach
   hinten gestellt (Score 1000: keine billige Widerlegung mehr, die
   Antworten mit den wenigsten Schachs danach bleiben vorn). Die Vorschau
   `lookAhead` zählt nach einem Gegenschach keine Schachs. Das halbiert
   KQ-KRR (siehe Tabelle). **Nur für einen Verteidiger mit Figur**
   (`defenderHasPiece`: Dame, Turm, Läufer oder Springer): der blanke
   König kann nicht jagen, und seine freien Ausweichzüge ließen die Jagd
   an jedem Politik-Knoten den Schachs des Angreifers bis zum Deckel
   folgen (KBP-K falscher Läufer war in 110 s nicht fertig; ohne Figur
   wird jetzt gar nicht gejagt, Randbauer und falscher Läufer stehen
   wieder exakt auf den Zahlen ohne Jagd).
2. **Stille Züge nur am Startknoten der Jagd** (`ChaseQuietMoves` = 1 je
   Pfad, `ChaseQuietTopOnly`, Breite `ChaseQuietWidth` = 3 nach derselben
   Vorschau, erst nachdem alle Jagdzüge des Knotens gescheitert sind, und
   erst dann bewertet, weil ein stiller Zug der Vorschau die ganze
   Zugliste des Angreifers öffnet; ein stiller Zug, nach dem eine Antwort
   gar kein Schach lässt, wird nicht probiert). Fehlschläge und Schnitte
   tragen das Budget, mit dem sie probiert wurden (`chaseQuiet`), ein
   Pfad mit mehr Budget jagt den Knoten erneut; Holds sind budgetfrei,
   ihre Struktur steht im Graphen. Die Jagd ist damit die Vorschau der
   Politik für einen stillen Zug: Qb1 an der Wurzel ist jetzt ein
   Jagdkandidat. Gelöst hat es nichts, geschadet auch nichts.
3. **Verworfen: stille Züge an jedem Knoten der Kette** (erst mit Budget
   1 je Pfad, dann mit "ein stiller Zug nach jedem Schach"): die Jagd
   wird zur Vollbreitensuche, KQ-KRRPP schnitt bei Deckel 16 nach 416
   Tsd. Jagdknoten mit 1,9 Mio. neuen Stellungen, ohne Deckel 32 zu
   erreichen. Ebenfalls verworfen: ein Politik-Term +800 für
   Bauernvorstöße (der Bauernlauf nach Qb1): die Welle endete trotzdem
   nicht (8,3 Mio. Stellungen, die Dame auf 7.762 Platzierungen), und
   die Pattfallen KQ-KP löste er auch nicht per Zertifikat (die bleiben
   Sache des Abstiegs: 4,0 und 5,9 Mio. Stellungen). Keine Zahl, kein
   Term.

**Messung** (Zertifikat-Bauer allein, `P`, Jagdknoten in Go und Rust
identisch; Zeiten mit fremder Engine nebenher, nur Größenordnung):

| Stellung | Frage | vorher | jetzt |
|---|---|---|---|
| KQ-KRR Dauerschach (Vier-Steiner als Orakel) | Schwarz gewinnt | Zertifikat 78.280 (Deckel 64), 3.266.718 Jagdknoten, 1.453.445 Stellungen | Zertifikat 37.789 (Deckel 32), 1.662.905 Jagdknoten, 1.312.429 Stellungen, Go 45 s / Rust 17 s |
| KQP-KRRP vier Remis (Syzygy) | Schwarz gewinnt | Zertifikat 3.467, 123.800 Jagdknoten | Zertifikat 932 (Deckel 16), 104.879 Jagdknoten, 149.166 Stellungen |
| KP-K Randbauer, KBP-K falscher Läufer (ohne Tabellen) | Weiß gewinnt | 1.410 / 54.262 sicher, 757 / 33.118 Jagdknoten | 1.423 / 53.048 sicher, 0 Jagdknoten (= ohne Jagd) |
| KQ-KRRPP ein Remis, KQP-KRRP zwei Remis | Schwarz gewinnt | offen | offen: die Jagd vom Frageknoten scheitert sauber (163 Tsd. / 32 Tsd. Jagdknoten), die Welle endet nicht |

**Lehre.** Die Jagd ist das Werkzeug für Schachketten, und sie ist damit
fertig: freie Ausweichzüge und der stille Zug am Start sind ihr Rand.
Die beiden offenen Stellungen brauchen etwas anderes, einen
Zertifikat-Bauer, der eine Welle mit vierzig Angreifer-Antworten je
stillem Zug beherrscht: eine begrenzte Welle mit Fixpunkt dazwischen
und eine Politik, die Drohung und Bauernlauf kennt (die Remisseite aus
dem M4-Entwurf), beides mit Zahlen zu belegen. Ob ihr Zertifikat in 16
Mi Stellungen passt, ist offen; Stockfishs `cp 0` ist eine Bewertung,
kein Beweis.

## Speicher je Knoten: Kanten-Arena, Keys aus dem Satz, Store verdoppelt (2026-10-22)

Der Graph kostete rund 250 Byte je Stellung (oben gemessen am
Prozess). Drei Posten waren es, und jeder ist jetzt weg oder kleiner:

1. **Kanten-Arena statt Slices.** Kinder- und Elternlisten waren je ein
   Slice pro Knoten: 48 Byte Köpfe plus Wachstumsreserve, bevor eine
   einzige Kante gespeichert war. Jetzt liegen alle Kanten in zwei
   Arenen aus einfach verketteten Listen (`edge{node, next}`, 8 Byte):
   die Kinder eines Knotens in Einfügereihenfolge (`firstChild`,
   `lastChild`), die Eltern neueste zuerst (`firstParent`), dazu
   `nchildren`; 13 Byte je Knoten und 16 je Kante, keine Reserve je
   Liste. Die Kinder eines Angreiferknotens entstehen am Stück und
   liegen deshalb hintereinander; die Reihenfolge der Eltern spielt für
   kein Ergebnis eine Rolle (`propagate`, `levels`, `safeSet` sind
   reihenfolgeunabhängig), die der Kinder bleibt wie vorher. `mid`
   kopiert die Kinder in ein lokales Array (`childrenOf`), weil es sie
   mehrfach indiziert.
2. **Keys aus dem Satz.** Die 16-Byte-Kopie des Zobrist-Keys je Knoten
   diente nur dem Umhashen beim Wachsen des Stores; jetzt wird der Key
   dafür aus dem 32-Byte-Satz dekodiert (`board(i).Key`). Das kostet
   beim Wachsen eine Dekodierung je Knoten, insgesamt das Doppelte der
   Endgröße, im Rauschen der Suche.
3. **Store verdoppelt statt vervierfacht.** Nach einem Faktor-4-Sprung
   war der Store viermal so groß wie der Graph (bei 4,05 Mio. Stellungen
   16,8 Mi Slots, 66 Byte je Stellung). Mit Faktor 2 sind es höchstens
   zweimal 21 Byte.

Dazu: die Beweiszahlen `pn`/`dn` gibt es nur noch im Graphen des
Abstiegs (8 Byte, der Zertifikat-Bauer braucht sie nicht), und der
Graph führt Buch: `Graph.Bytes()` summiert die Kapazitäten aller
Arrays und die Slots des Stores, `Result.Bytes`, die Testbank zeigt
`graphs N MB (B bytes per position)` je Stellung und zum Schluss die
Spitze des Prozesses (`egtb.PeakMemory`, zugesicherte Größe und
Arbeitssatz).

**Was ein Knoten jetzt kostet:** Satz 32, Listenköpfe 13, Status,
Seite, Züge, Rest, Flags 5, Ebene 2 = 52 Byte; dazu im Abstieg 8 für
die Zahlen, im Zertifikat-Bauer 6 für Leck-Ebene und Jagd-Memo; je
Kante 16; der Store 16 Byte je Slot, am Deckel 21 je Stellung. Der
Abstiegsgraph von KQ-KP hat 3,4 Kanten je Stellung, die Kanten sind
dort der größte Posten.

**Messung** (KQ-KP c-Bauer, Abstieg allein, 4.048.336 Stellungen,
Besuche vor und nach dem Umbau identisch; Prozess-Spitze laut
Betriebssystem, zugesichert / Arbeitssatz; Buchhaltung nach Kapazität):

| | Go vorher | Go nachher | Rust vorher | Rust nachher |
|---|---|---|---|---|
| Spitze MB | 1.081 / 1.071 | 1.039 / 930 | 1.001 / 967 | 692 / 639 |
| Byte je Stellung (zugesichert) | 267 | 257 | 248 | 171 |
| Buchhaltung Byte je Stellung | – | 176 | – | 161 |

Zwischenschritt gemessen: die Arena allein brachte in Go erst eine
höhere Spitze (1.180 MB), weil der Faktor-4-Store und die
Kopier-Transienten der wachsenden Arrays blieben; erst mit Store-Faktor
2 und ohne Keys fiel die Buchhaltung von 226 auf 176. Go bleibt an der
Spitze über der Buchhaltung (Reallokationen der Slices mit Faktor 1,25
kopieren alt und neu nebeneinander, der Garbage Collector gibt verzögert
frei); Rust liegt mit 171 Byte je Stellung an der Spitze nah an der
Buchhaltung, und die zählt für den Hash-Deckel. Zertifikat-Graphen:
KQ-KRR 131 (Go) / 167 (Rust, `Vec` verdoppelt, Go wächst um 1,25),
vier Remis 126 / 189 Byte je Stellung. `wdlBytesPerPosition` im UCI
bleibt vorerst 256 (Go liegt an der Spitze noch darüber); für Rust
wäre 192 gedeckt. Alle Regressionswerte (Jagdknoten, Zertifikate,
Besuche) sind in Go und Rust unverändert und identisch.

**Tempo, A/B-Wechsellauf** (zuhause, Maschine frei, alte und neue
Binary abwechselnd, je zwei bis drei Läufe; die verketteten Listen
hätten den Fixpunkt bremsen können, sie tun es nicht):

| Lauf | alt | neu |
|---|---|---|
| Rust KQ-KRR Zertifikat (`xtP`, 1,66 Mio. Jagdknoten) | 11,8 / 11,5 / 12,0 s | 11,9 / 11,1 / 11,4 s |
| Rust KQ-KP Abstieg (`xp`, 4,05 Mio. Stellungen) | 10,4 / 8,5 s | 8,6 / 7,8 s |
| Go KQ-KRR Zertifikat | 31,0 / 31,6 s | 20,1 / 19,4 s |
| Go KQ-KP Abstieg | 15,1 / 14,9 s | 9,4 / 9,4 s |

Rust: Zertifikat gleich schnell (±2 %), Abstieg rund 10 % schneller
(kein Key-Array mehr, das mitwächst). Go: ein Drittel schneller in
beiden, weil Millionen Slice-Köpfe mit Zeigern verschwunden sind, die
der Garbage Collector bei `SetGCPercent(1)` ständig abgelaufen hat;
die Arenen sind zeigerfreie Arrays.

## Zweiter Anlauf an den gemischten Remis: zwei Befunde, drei verworfene Terme (2026-10-22)

Prüfstand war zuerst die KQ-KP-Pattfalle mit dem c-Bauern ohne Tabellen
(sichere Menge 89.506, der Abstieg braucht 4 Mio. Stellungen), weil sie
billiger ist als die Sieben-Steiner. Der Zertifikat-Bauer allein kam
dort nicht zum Ende, aber die Fortschrittszeilen zeigten zwei Dinge:

1. **Jagd-Freigabe je Knoten statt je Wurzel.** Die Jagd an
   Politik-Knoten galt, wenn der Verteidiger *an der Wurzel* materiell
   schwächer war; in KQ-KP wandelt der Bauer um, und die
   Dame-gegen-Dame-Knoten darunter wurden mit je 4.096 Knoten gejagt,
   ohne einen Hold: 8,4 Mio. Jagdknoten mit 7 Mio. Schnitten in 105 s.
   Jetzt entscheidet `defenderChases` am Knoten (Figur vorhanden und
   materiell schwächer): 2 Mio. Jagdknoten in 50 s, fast nur billige
   Fehlschläge bei Unterverwandlungen. Nicht gelöst, aber ein Viertel
   der Arbeit.
2. **Die Schlagprüfung kennt Patt und Tabellen-Remis.** Die Politik
   wählte c1=D+ (Schach +1500) statt Ka1, weil nach Ka1 der Bauer c2
   "hing" (−2000): dass Dxc2 Patt ist, sah die Ein-Zug-Tauschprüfung
   nicht. `captureFree` zählt einen Schlag nur als freien Gewinn, wenn
   danach weder zurückgeschlagen werden kann noch Patt, totes Material
   oder ein Tabellen-Remis steht.

**Gebaut, gemessen, verworfen** (alle ohne Wirkung auf die gelösten
Stellungen und ohne Wirkung auf die offenen):

- Schach-Bonus nur an Knoten, deren Jagd nicht gescheitert ist (die
  Schachs streuen dort nur). Keine Zahl.
- Bauernvorstoß +800, zum zweiten Mal. Keine Zahl.
- Adaptives Jagdbudget an Politik-Knoten (halbiert nach Fehlschlag oder
  Schnitt bis 64, 4.096 nach einem Hold): die Fortschrittszeilen von
  `KQP-KRRP zwei Remis` waren auf die Zahl identisch mit vorher, die
  1,6 Mio. Holds setzten das Budget ständig zurück.

**Befund an `KQP-KRRP zwei Remis`** (Rust = Go auf die Zahl): nach 100 s
13,4 Mio. Stellungen, davon 10,5 Mio. Jagdknoten mit 7,7 Mio.
Schnitten; die Jagden an den Politik-Knoten erzeugen die Stellungen,
nicht die Welle, und das 16-Mi-Budget ist nach rund 130 s voll, ohne
dass eine Runde endet. Die freien Ausweichzüge lassen die Jagd dort
den Turmschachs folgen, bis der Deckel schneidet. Das Zertifikat des
Bauernlaufs selbst (schwarze Stellungen innerhalb von vier Zügen plus
Schachs, mal die Bauernstufen) dürfte bei mehreren Millionen liegen,
und `KQ-KRRPP ein Remis` darüber. Beide bleiben offen; der nächste
Ansatz muss die Jagd an Politik-Knoten zügeln (kein Budget je Knoten,
sondern je Runde) und die Welle selbst beobachten.

**Nächste Hebel**, nach erwartetem Nutzen:

1. Gemischte Remis als Zertifikat-Problem: Jagdbudget je Runde statt
   je Knoten, die Welle begrenzen (Budget je Runde, Fixpunkt dazwischen,
   Erweiterung nur am Leck), Politik mit Drohung; Maßstab die zwei
   offenen Sieben-Steiner und die KQ-KP-Pattfallen (heute nur per
   Abstieg).
2. Beide Fragen in einem Graphen (die Gewinne der einen Frage sind die
   Verluste der anderen).
3. Politik: freie Königsfelder statt Klebens gegen Springer; taktische
   Vorschau (Schach-Gabeln) für KR-KB-artige Verteidigungen.
4. ~~Stop-Flag im Fixpunkt pollen (Stop bis zu zwei Sekunden verspätet).~~
   Gebaut (2026-10-22): `safeSet(stopped)` pollt das Stop-Flag alle
   `StopPoll` = 65.536 Schritte (Knoten der Anfangsschleife und der
   Breitensuche); ein gestoppter Fixpunkt markiert nichts (die
   Kratzflags setzt der nächste zurück) und meldet `true`, alle fünf
   Aufrufer brechen dann ab. Gemessen: ein Fixpunkt auf 2,5 Mio.
   Stellungen dauert 328 ms (Rust) / 399 ms (Go), die Zeile `fixed
   point at ... took ...` zeigt es; bei 16 Mi waren es die zwei Sekunden,
   jetzt liegt die Latenz im Fixpunkt unter einer Millisekunde.
   Ergebnisse ohne Stop unverändert (Regression knotengleich).
   `tests/uci-stop.sh <binary> "<fen>" [pause] [hash]` misst die Latenz
   von `stop` bis `bestmove` (Stichproben treffen den Fixpunkt selten:
   alt wie neu rund 45 ms außerhalb davon).
5. ~~`Threads` beachten (die zwei Fragen parallel statt nacheinander).~~
   Gebaut (2026-10-22): mit `Threads` ≥ 2 laufen die zwei Fragen eines
   offenen Zugs in zwei Threads (getrennte Graphen, der Fortschritt unter
   Mutex), die Ergebnisse werden in der Reihenfolge des Zugs angewandt,
   das zweite nur, solange es noch gebraucht wird; sobald eine Seite der
   Frage entschieden ist, bleibt der Zug sequenziell. Rust ohne Tabellen,
   bis zum vollen Budget (Hash 2048, MultiPV 3): Sieben-Steiner KRRN-KNP
   11,2 statt 16,0 s, einsamer Springer 13,4 statt 14,6 s, kurze Läufe
   gleich; dieselben Ergebnisse.
6. Syzygy auch in der Tabellenstufe an der Wurzel (heute nur als
   Such-Orakel, die Wurzel antwortet über die Suche in Millisekunden).
7. Speicher weiter: `wdlBytesPerPosition` nach Messung an der Spitze
   senken (Rust 192), Rust-`Vec`s mit Faktor 1,25 wachsen lassen,
   Kanten mit `uint32`-Paaren in Blöcken ohne Reallokation.
