# Milestone 7: WDL-Suche, Entwurf (2026-10-21)

Eine reine, schnelle Suche nach **garantiertem Gewinn, Verlust oder Remis**, ohne
Mattdistanz. Sobald das Ergebnis der Wurzel (und bei MultiPV jedes Wurzelzugs)
feststeht, hält die Suche an. Mattlängen, Fünfzig-Züge-Regel und Tabellen werden
bewusst ausgeklammert: Ziel ist, ein erzwungenes Remis oder einen erzwungenen
Gewinn mit möglichst wenigen Knoten und Hash-Einträgen zu finden. Die Remisseite
bekommt dabei eine eigene Politik, der Fokus des Autors liegt auf der Remissuche.

Dieser Entwurf ist die Diskussionsgrundlage. Package-Name `wdlpn` ist ein
Arbeitstitel, die Zahlen in den UCI-Abschnitten sind Vorschläge.

## 1. Was anders ist als in `matepn`

| | `matepn` (Milestone 4) | WDL-Suche |
|---|---|---|
| Frage | Matt in ≤ N Halbzügen | Gewinn, Verlust, Remis erzwingbar |
| Tiefenschranke | ja, im Key gesalzen (zyklenfrei) | keine, der Graph hat Zyklen |
| Widerlegung | "kein Matt bis N", verfällt mit N | echter Beweis "hält mindestens Remis" |
| Remis-Terminale | nur Patt und totes Material | dazu Stellungswiederholung |
| Ergebnisse | bewiesen, widerlegt | sechs Zustände (Abschnitt 3) |
| Tabellen | Orakel mit Distanz | abschaltbar, später WDL-Orakel |

Ohne Tiefenschranke ist eine Widerlegung vollständig: Jede Linie endet in Matt,
Patt, totem Material oder Wiederholung, der Graph entscheidet die Stellung. Das
ist der Grund, warum eine Remis**suche** überhaupt möglich wird; mit der
Tiefenschranke von `matepn` gab es nur "bis hier kein Matt".

## 2. Zwei Fragen statt drei Werte

Proof-Number-Suche beantwortet eine Ja/Nein-Frage. Drei Ergebnisse bekommt man
über zwei Fragen pro Stellung, beide aus Sicht der Seite am Zug:

- **Q1:** Kann ich den Gewinn erzwingen?
- **Q2:** Kann der Gegner den Gewinn erzwingen?

Q1 bewiesen = Gewinn. Q2 bewiesen = Verlust. Beide widerlegt = Remis bewiesen.
Jede Frage ist ein AND/OR-Baum: Bei Q1 ist die Seite am Zug der Angreifer (OR,
ein Zug genügt), der Gegner der Verteidiger (AND, jeder Zug muss gedeckt sein).
Die Widerlegung von Q1 ist genau die Remissuche aus Sicht des Verteidigers: Er
wählt (OR für die Widerlegung), der Angreifer muss alles decken.

Die Alternative, ein dreiwertiger Baum mit einem Zahlentripel pro Knoten, wird
nicht verworfen, sondern als Experiment vorgemerkt (Abschnitt 11): Beide
Varianten gehören durchprobiert, erst die Zwei-Fragen-Fassung, weil sie die
`matepn`-Mechanik (`mid`, Schwellen, Prefetch, Klassen) wiederverwendet.

Welche Frage zuerst läuft, entscheidet eine Heuristik an der Wurzel (Material,
später eine Bewertung). Meist räumt die erste Frage eine Seite ab und die zweite
findet ihre Beweise schon in der Tabelle.

## 3. Ergebnisraum: Schranken über L < D < W

Jeder Eintrag hält aus Sicht der Seite am Zug eine **untere und eine obere
Schranke** über den drei Werten Verlust < Remis < Gewinn, je zwei Bit, sechs
gültige Intervalle:

| Intervall | Bedeutung | Offene Frage | Klasse |
|---|---|---|---|
| [W, W] | Gewinn bewiesen | keine | 3 |
| [D, D] | Remis bewiesen | keine | 3 |
| [L, L] | Verlust bewiesen | keine | 3 |
| [D, W] | verliert nicht, könnte gewinnen | Q1 | 2 |
| [L, D] | gewinnt nicht, könnte verlieren | Q2 | 2 |
| [L, W] | unbekannt | Q1 und Q2 | 0 oder 1 |

Das deckt die Wunschliste "Weiß gewinnt garantiert", "Weiß könnte noch
gewinnen", "Remis garantiert" ab und bleibt farbsymmetrisch wie der ganze
übrige Code (Sicht der Seite am Zug, der Treiber dreht). Die Klasse ist die
Verdrängungspriorität (Abschnitt 6).

## 4. Terminale Knoten

- **Matt:** Verlust der Seite am Zug.
- **Patt:** Remis.
- **Totes Material** (FIDE 5.2.2, `mateab.Material`): Remis. Das ist eine Regel,
  keine Tabelle, bleibt also auch bei abgeschalteten Tabellen an.
- **Wiederholung** einer Stellung auf dem Pfad (gleiche Seite am Zug): Remis,
  mit der Besonderheit aus Abschnitt 5.
- **Tiefendeckel** (Sicherung, etwa 200 Halbzüge): Ergebnis unbekannt, nie
  Remis.
- Tabellen (eigene, später Syzygy-WDL): als Orakel zuschaltbar, geben direkt
  das Intervall; abgeschaltet für die Tests der Suche selbst.

Eine Wiederholung ist in beide Richtungen korrekt: Kann die Remisseite die
Rückkehr zu einer Stellung erzwingen, erzwingt sie auch die dreifache (an der
wiederholten Stellung sind alle Angreiferzüge gedeckt, jede Linie endet in
einem Remis-Terminal oder wieder dort). Dauerschach und Pendeln sind damit
keine Sonderfälle, sondern Wiederholungen, die der Remisseite als Terminal
angeboten werden.

## 5. Wiederholung erkennen und das GHI-Problem

**Erkennung.** Der Pfad von der Wurzel ist ein Array von 128-Bit-Keys. Vergleich
nur gegen Einträge mit derselben Seite am Zug und nur zurück bis zum letzten
irreversiblen Zug (Schlag, Bauernzug; der Generator kennt beides). Das sind
meist unter zehn Vergleiche, ein eigener Hash lohnt nicht. Bei 128 Bit sind
Kollisionen kein Thema. Die Partie-Historie aus `position ... moves` kann als
Pfad vor der Wurzel vorangestellt werden; das ist billig, aber nachrangig,
weil die Teststellungen blanke FENs sind.

**Graph-History-Interaction.** "Remis, weil ich zum Vorgänger R zurückkomme"
gilt nur, solange R auf dem Pfad liegt. In der TT gespeichert wäre der Eintrag
falsch, sobald dieselbe Stellung über einen anderen Weg erreicht wird, auf dem
R nicht liegt. Regel:

- Jeder Remisbeweis trägt die **Tiefe des flachsten wiederholten Vorgängers**
  (`depRef`) nach oben; ein Terminal ohne Wiederholung hat `depRef = ∞`,
  eine Wiederholung von R auf Tiefe r hat `depRef = r`, ein Knoten erbt das
  Minimum seiner Kinder (OR: des gewählten Beweiskinds, AND: aller Kinder).
- Solange `depRef < eigene Tiefe`, ist der Beweis **bedingt** und wird nicht
  als Endeintrag gespeichert (offene Zahlen dürfen bleiben, sie sind nur
  Schätzungen). Er gilt lokal auf dem Pfad weiter nach oben.
- Bei `depRef ≥ eigene Tiefe` löst sich die Bedingung auf: Der Vorgänger R ist
  der Knoten selbst, das Remis ist pfadunabhängig und wird gespeichert.

Gewinn- und Verlustbeweise brauchen das nicht: Sie sind endliche Bäume, in
denen entlang jedes Zweigs alle Stellungen verschieden sind. Mit einer fremden
Historie kommt eine Stellung höchstens auf zwei Vorkommen, nie auf drei, die
dreifache Wiederholung kann der Verteidiger also nicht erzwingen. Die
Einfach-Wiederholung ist deshalb nur das Werkzeug der Remisseite, nicht die
Regel des Spiels; der Beweis bleibt unter den echten Regeln gültig.

**Bekannte Gefahr:** df-pn kann auf Graphen mit Zyklen in Schwellen-Oszillationen
geraten, wenn pn/dn aus verschiedenen Historien stammen (Kishimoto und Müller,
"A general solution to the graph history interaction problem", 2004). Erst
messen, dann lösen: Tiefendeckel und Besuchszähler sind die Sicherung, die
Literatur die Reserve.

## 6. Transposition Table

Derselbe 16-Byte-Eintrag wie überall (Index-Bits des Keys durch den Value
ersetzt, 128-Bit-Identität). Keys sind **nicht** tiefengesalzen, es gibt keine
Tiefe. Value bei `tt.Buckets` mit 16 Mi Slots: 22 Bit.

| Bits | Inhalt |
|---|---|
| 2 | Klasse (Verdrängungspriorität): 0 offen, 1 offen mit Aufwand, 2 halb entschieden, 3 bewiesen |
| 3 | Intervall (einer der sechs Zustände aus Abschnitt 3) |
| 1 | Tag: zu welcher Frage die Zahlen gehören (Q1/Q2) |
| 16 | pn/dn der offenen Frage, Codec wie `matepn` (`Saturating`, je 8 Bit, oder `Float`) |

Bei [L, W] sind beide Fragen offen, die Zahlen gehören zur gerade verfolgten
(Tag); wechselt die Suche die Frage, beginnt die andere mit 1/1 und
Mobilität. Das kostet wenig, weil die erste Frage meist eine Seite komplett
abräumt. Beweise (Klasse 3) und halb Entschiedenes (Klasse 2) werden nie für
Schätzungen verdrängt, wie seit Schritt 7 in `matepn`. Bedingte Remisbeweise
(Abschnitt 5) werden nicht gespeichert. Die Zahl der TT-Einträge ist neben
den Besuchen die zweite Messgröße, weil der Autor sie ausdrücklich minimieren
will.

Zu prüfen: ob 16 Bit für pn/dn reichen (in `matepn` sind es 20 bei
Sättigung 1022). Mit acht Bit pro Zahl sättigt eine Seite bei 254; der
`Float`-Codec mit 4 Bit Exponent reicht weiter. Beide Codecs existieren,
die Messung entscheidet.

## 7. Remis-Politik (Blatt-Initialisierung)

Der Haken für die Politik ist die Initialisierung neuer Blätter, wie
`Mobility` in `matepn`: pn und dn eines Blattes sind Schätzungen, die den Weg
der Suche lenken. Für die Remisseite (den Verteidiger in Q1, den Angreifer in
Q2):

- **Wiederholung in einem Zug:** liegt der Kind-Key auf dem Pfad, ist das Kind
  ein Terminal-Remis mit dn = 0; die Suche nimmt es sofort.
- **Abtausch:** Schläge, die das Material in Richtung totes oder bekannt
  remisliches Material bringen (ungleichfarbige Läufer, KNN-K, Randbauer mit
  falschem Läufer), bekommen kleines dn.
- **Blockade und Pendeln:** Züge, nach denen der Gegner keine Bauernzüge und
  keine Schläge hat, bekommen kleines dn (der König pendelt, bis die
  Wiederholung kommt).
- **Schach geben:** Kandidat für Dauerschach, mittleres dn.
- Alles andere: Mobilität wie bisher.

Die Angreiferseite behält zunächst die Mobilität. Ob später eine
Engine-Bewertung (Patricia) oder weiter Heuristik die Züge sortiert, bleibt
offen; die Schnittstelle ist dieselbe. Jede Heuristik wird als Option
gemessen (Besuche, Einträge, Zeit), wie `MatePnMobility`.

## 8. Wurzeltreiber und UCI

Der vorhandene Treiber (`runRoot`) bleibt: jeder Wurzelzug einzeln, die
Stellung nach dem Zug als Suchwurzel, Ergebnis gedreht. Stopp, sobald jeder
Wurzelzug (bei MultiPV n jeder der n) ein bewiesenes Intervall hat; ohne
MultiPV reicht ein bewiesener Gewinn, oder alle Züge Remis/Verlust.

- Option `Search fastwdl` (neben `none`/`mateab`/`matepn`/`matelist`).
- Option `Egtb` (`on`/`off`): schaltet eigene Tabellen (und später Syzygy)
  als Orakel ab; totes Material bleibt immer an.
- Score-Vorschlag: `cp 9999` Gewinn bewiesen, `cp -9999` Verlust bewiesen,
  `cp 0` Remis bewiesen und unbekannt (intern unterschieden), `cp 10` für
  [D, W] und `cp -10` für [L, D] als Zwischenstand. Arena zeigt
  `info string` im MultiPV-Modus nicht, deshalb tragen die Zahlen die
  Information. Will der Autor Remis und unbekannt auch in Arena
  unterscheiden, bleibt `cp 1` für unbekannt als Option.
- `wdl`-Feld und `lowerbound`/`upperbound` zusätzlich, wenn sie nichts
  stören; sie sind Standard-UCI, aber Arena zeigt sie vermutlich nicht.
- Variante: bis zum Terminal (Matt, Patt, Wiederholung mit Angabe des
  wiederholten Zugs), keine Verlängerung aus Tabellen.

## 9. Teststellungen

Geprüft mit `tests/sf-wdl.sh` (Stockfish 19, Syzygy 3 bis 6 Steine,
Fünfzig-Züge-Regel aus, damit "cursed wins" als Gewinne zählen, wie bei uns).
Liste in `tests/wdl-positions.txt`, Ergebnis in `test-positions.md`, Abschnitt
"WDL-Teststellungen". Alle sind klein genug, dass unsere Vier- und
Fünf-Steiner die Antwort ebenfalls kennen; während der Suche sind die
Tabellen aus.

## 10. Messgrößen

- Besuche (`mid`-Aufrufe) und **TT-Einträge** (gespeicherte Stellungen), Go
  und Rust identisch, als Regressionswerte.
- Zeit bis jeder Wurzelzug entschieden ist, pro Zug wie in der
  Benchmark-Zeitleiste von KRR-KN.
- Maßstab: Beweisgraph. `matelist` zählt bisher den Matt-Beweisgraph; für WDL
  ist der Gewinn-Beweisgraph kleiner (Angreifer ein beliebiger Gewinnzug), der
  Remis-Beweisgraph ein neues Maß. Beides kann `matelist` nach dem gleichen
  Muster zählen, vorgemerkt.

## 11. Umsetzungsplan

1. **Package `wdlpn` in Go, nur Q1:** df-pn ohne Tiefensalz, Pfad-Array mit
   Wiederholung als Verteidiger-Terminal, Tiefendeckel, Mobilität als
   Blatt-Initialisierung. Test an den Gewinn-Stellungen mit Tabellen aus:
   Besuche gegen `matepn` mit Mattziel.
2. **Q2 und Remis:** beide Fragen, Intervall-Eintrag, `depRef` für bedingte
   Remisbeweise. Test an den Remis-Stellungen; KNN-K ist der Härtetest, weil
   die Widerlegung den ganzen erreichbaren Graphen braucht.
3. **UCI `fastwdl`** mit Wurzeltreiber, Scores, Option `Egtb`.
4. **Rust-Port**, Besuche und Einträge identisch.
5. **Remis-Politik** als Optionen, Messreihe an den Remis-Stellungen und am
   Benchmark KRR-KN (alle zwölf Züge als Verlust, Zeit gegen die 70 s mit
   Mattdistanz).
6. **Dreiwertiger Baum** als Vergleichsexperiment, gleiche Messreihe.
7. Später: Tabellen und Syzygy-WDL als Orakel wieder zuschalten (Intervall
   direkt aus der Tabelle), Partie-Historie als Pfad, Verkürzung zum Matt
   als zweite Stufe (`matepn` mit Schranke).

## 12. Offen

- Zwei Fragen oder dreiwertig: Messung (Schritt 6).
- Oszillation von df-pn auf Zyklen: beobachten, Literatur als Reserve.
- Vollständige Widerlegungen (Remisbeweise) sind erschöpfend; wie weit die
  Remis-Politik das verkleinert, ist die zentrale Frage des Milestones.
- Fünfzig-Züge-Regel bleibt draußen; "garantiert" heißt ohne sie.
