# Milestone 7: WDL-Suche, Ergebnisse

Entwurf in `docs/m7-wdl-search-design.md`, Teststellungen in `test-positions.md`
("WDL-Teststellungen") und `chess/wdldata.go`. Alle Zahlen hier vom
Arbeitsrechner (i5, ein Thread), Go; Rust folgt nach dem Mechanik-Schritt.

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

Vorschlag für Schritt 2 (`wdllist`, Arbeitstitel):

1. Erst die unbeschnittene Fassung: erreichbaren Graphen von der Wurzel
   aufbauen (Breitensuche mit `ttstore`, wie `matelist`), WDL retrograd
   über Zähler, beide Fragen in einem Durchgang. Für alle Teststellungen
   bis vier Steine passt der Raum in den Speicher (KNN-K rund 1 Mio.
   Stellungen); das ist die exakte, schnelle Referenz und zugleich der
   Zähler für den Remis-Beweisgraphen.
2. Dann die Beschneidung, aus der die kleinen Beweise kommen: der
   Verteidiger expandiert zuerst nur seine Politik-Züge (Wiederholung,
   Abtausch, Blockade), der Angreifer alle; wird die Wurzel trotzdem
   gewonnen, bekommen die Verteidigerknoten auf dem Gewinnweg ihre
   nächsten Züge, und die Propagation läuft inkrementell weiter. Für
   Gewinne umgekehrt (Angreifer Politik-Züge, Verteidiger alle). Das ist
   Beweiszahl-Suche auf dem expliziten Graphen, ohne die Zahlen.
3. `wdlpn` bleibt für die Gewinnseite als Vergleich (und ist dort schon
   gut), und als Messlatte für Schritt 2.
