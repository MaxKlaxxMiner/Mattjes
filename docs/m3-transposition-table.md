# Milestone 3: Transposition Table

Stand: 2026-10-02, Go 1.26.3, Rust 1.95, Windows/amd64, 12 logische CPUs. Alle
Experimente auf dem Bitboard-Generator mit dem festen 128-Bit-Zobrist-Key aus
Milestone 2. Die Absolutwerte dieses Laufs liegen etwas unter denen aus M2 (die
Suite ist größer und enthält Stellung 3 bis Tiefe 8, die pro Knoten langsamer
ist); belastbar sind die Verhältnisse innerhalb des Laufs.

## Zwei Charaktere von Tabellen

Der Milestone hat zwei Packages hervorgebracht, weil "Transposition Table" in
Mattjes zwei verschiedene Dinge meint:

| Package | Charakter | Größe | Verlust | Einsatz |
|---|---|---|---|---|
| `tt` | **Cache** | fest, Zweierpotenz | ja, Einträge werden ersetzt | Suche (Alpha/Beta, df-pn-Heuristiken), Perft |
| `ttstore` | **Store** | fest, Zweierpotenz | nie, meldet "voll" | bewiesene Ergebnisse, Deduplizierung der Listen-Suche, Fortsetzen langer Suchen |

Ein Mattbeweis darf sich nicht auf einen Eintrag stützen, der inzwischen
überschrieben wurde. Deshalb trennt Mattjes die beiden Rollen, statt wie
klassische Engines alles in eine verlustbehaftete Tabelle zu stecken. Beide
Tabellen haben eine vom Aufrufer gewählte feste Größe; dynamisches Wachsen ist
nicht vorgesehen, eine Suche dimensioniert ihre Tabellen vorab.

## Der Eintrag: 16 Byte, voller Key, Value in den Index-Bits

Beide Packages benutzen denselben Eintrag (Entwurf des Autors):

```
Slot-Index:  k0 & mask                     mask = Slots - 1, b = log2(Slots) Bits
word0:       (k0 &^ mask) | value          Key-Wort 0, untere b Bits durch den Value ersetzt
word1:       k1                            Key-Wort 1 komplett
leer:        word0 == 0 && word1 == 0
Treffer:     word1 == k1 && (word0 ^ k0) &^ mask == 0
```

Die b Index-Bits stehen implizit in der Slot-Position und müssen nicht noch
einmal gespeichert werden. Genau diese b Bits nimmt der Value. Damit trägt der
Eintrag die **volle 128-Bit-Identität** der Stellung, ein falscher Treffer ist
ausgeschlossen, und der Eintrag bleibt trotzdem 16 Byte groß. Der Preis: Die
Value-Breite ist an die Tabellengröße gekoppelt.

| Slots | Speicher (direkt) | Value-Bits | Max-Value |
|---|---|---|---|
| 2^20 | 16 MB | 20 | 1.048.575 |
| 2^24 | 256 MB | 24 | 16.777.215 |
| 2^26 | 1 GB | 26 | 67.108.863 |

Bei den Buckets (4 Einträge pro 64-Byte-Bucket = eine Cache-Line) zählt die
Bucket-Zahl: 256 MB sind 2^22 Buckets und damit 22 Value-Bits, 1 GB sind 2^24
Buckets mit 24 Bits.

Zum Vergleich: Stockfish speichert 10 Byte pro Eintrag, davon nur 2 Byte
Prüfwort. Ein falscher Treffer verfälscht dort eine Bewertung, die die Suche
meist wieder korrigiert. Mattjes gibt 6 Byte mehr aus und kauft sich damit
Beweisfähigkeit. Was in die 24 Value-Bits passen muss, entscheidet Milestone 4:
bei df-pn zwei Zahlen (Proof/Disproof, je 12 Bit mit Sättigung), bei einer
Distanzsuche Mattdistanz plus Schranke plus Zugindex.

Zwei Layouts teilen sich den Eintrag und die Statistik:

- **`Table`** (direkt abgebildet): ein Eintrag pro Slot, immer ersetzen.
- **`Buckets`**: vier Einträge pro Cache-Line, ein Probe prüft alle vier zum
  Preis eines Speicherzugriffs. Ersetzt wird der Eintrag mit demselben Key,
  sonst der erste leere, sonst der mit dem **kleinsten Value** (Perft: der
  kleinste Teilbaum, der am billigsten neu zu rechnen ist). Wer eine andere
  Priorität braucht, legt sie in die hohen Value-Bits. Einträge sind von vorne
  gepackt, es wird nie gelöscht. In Rust erzwingt `#[repr(C, align(64))]` die
  Ausrichtung, in Go wird ein Bucket überallokiert und der Slice-Anfang von
  Hand ausgerichtet.

## Perft mit Transposition Table

Perft ist der ideale erste Anwendungsfall, weil jeder Fehler sofort eine
falsche Knotenzahl ergibt. Ein Eintrag steht für "(Stellung, Resttiefe) hat
N Blätter":

- Die Resttiefe wird in **beide** Key-Wörter gesalzen. (Stellung, Tiefe) ist
  damit einfach ein anderer 128-Bit-Key, es braucht kein Tiefenbyte im Eintrag.
- Der Value ist die Knotenzahl. Teilbäume, die nicht in die Value-Bits passen,
  werden nicht gespeichert. Das kostet praktisch nichts: Bei perft(7) sind das
  nur die 20 Kinder der Wurzel (je über 100 Mio.) und die Wurzel selbst.
- Blätter werden weiter per Bulk-Counting gezählt, Einträge beginnen bei Tiefe 2.
- Die Tabelle wird vor jedem (Stellung, Tiefe)-Lauf geleert, der Gewinn stammt
  nur aus Transpositionen innerhalb eines Laufs. Das Leeren läuft über den
  Vorbereitungs-Hook des Runners (`perft.RunPrepared` / `perft::run_prepared`)
  **außerhalb** der gemessenen Zeit. In einer ersten Fassung lag es innerhalb,
  und 40 Läufe mit einer 1-GB-Tabelle waren 40 GB `memset`, rund 3 s, die der
  Tabelle als Perft-Zeit angelastet wurden.

Gesamte Referenz-Suite bis 4 Mrd. Knoten pro Stellung (8,42 Mrd. Knoten):

| Variante | Value-Bits | Go | Rust | Trefferquote | Ersetzungen |
|---|---|---|---|---|---|
| ohne TT (rekursiv, make/unmake) | | 49,72 s = 169 Mn/s | 36,88 s = 228 Mn/s | | |
| `Table` 16 MB (1 Mi Einträge) | 20 | 12,90 s = 653 Mn/s | 9,14 s = 922 Mn/s | 52,4 % | 1.071.289 |
| `Buckets` 16 MB | 18 | 11,75 s = 717 Mn/s | 8,80 s = 957 Mn/s | 52,1 % | 320.156 |
| `Table` 256 MB (16 Mi Einträge) | 24 | 11,90 s = 708 Mn/s | 9,14 s = 922 Mn/s | 52,5 % | 68.346 |
| `Buckets` 256 MB | 22 | 11,87 s = 710 Mn/s | 9,03 s = 933 Mn/s | 52,5 % | 36 |
| `Table` 1 GB (64 Mi Einträge) | 26 | 12,18 s = 692 Mn/s | 9,24 s = 912 Mn/s | 52,5 % | 17.293 |
| `Buckets` 1 GB | 24 | 12,20 s = 691 Mn/s | 9,35 s = 900 Mn/s | 52,5 % | 0 |

"Mn/s" ist hier die **effektive** Rate (gezählte Blätter pro Sekunde), nicht die
Zahl besuchter Knoten. perft(7) der Grundstellung fällt von 19,0 s auf 3,9 s
(Go) bzw. von 13,3 s auf 3,0 s (Rust), also Faktor 4 bis 5 bei nur 52 %
Trefferquote, weil jeder Treffer einen ganzen Teilbaum spart.

Die 1-GB-Tabellen sind trotz null Ersetzungen 2 bis 3 % langsamer als 256 MB.
Das ist der Preis zufälliger Zugriffe auf einen viermal größeren Speicherbereich
(TLB-Fehlzugriffe), den die wenigen vermiedenen Ersetzungen nicht aufwiegen.
Lehre: Eine Tabelle, die größer ist als nötig, bringt nichts und kostet etwas.

Die Zähler (Probes, Treffer, Stores, Ersetzungen, Fremdvergleiche) sind in Go und
Rust **bis auf die letzte Stelle identisch**, der Port ist also exakt.

### Direkt gegen Buckets

Bei knapper Tabelle (16 MB für bis zu 3,1 Mio. Stores pro Lauf) fallen die
Ersetzungen mit Buckets von 1,07 Mio. auf 320 Tsd., die Suite wird 4 % (Rust)
bis 9 % (Go) schneller, obwohl die Buckets zwei Value-Bits weniger haben und deshalb einige
größere Teilbäume nicht speichern können. Ist die Tabelle groß genug (256 MB,
Füllgrad unter 20 %), sind beide Layouts gleich schnell, der Scan über vier
Einträge kostet nichts Messbares. Für die Suche, deren Tabelle immer zu klein
ist, sind Buckets die richtige Wahl.

Die Ersetzung nach kleinstem Value hat ohne Alterung einen bekannten Nachteil:
Alte, wertvolle Einträge aus längst verlassenen Teilbäumen blockieren ihre
Buckets. Perft leert die Tabelle pro Lauf und merkt davon nichts. Die Suche
braucht in Milestone 4 eine Generation in den hohen Value-Bits (Stockfish-Stil:
Alter geht in die Opferwahl ein).

## Keygröße: gemessen statt geschätzt

Mit dem vollen Key im Eintrag ist die Frage für Mattjes entschieden. Die
Statistik zählt trotzdem weiter, was passiert wäre: Jeder Fremdvergleich (ein
Probe sieht einen Eintrag einer anderen Stellung) ist bei gekürztem Prüfwort
eine Gelegenheit für einen falschen Treffer. Gezählt wird, wie oft die unteren
16, 32 oder 48 Bits des zweiten Key-Worts eines Fremdeintrags trotzdem gepasst
hätten:

| Lauf | Fremdvergleiche | 16 Bit | 32 Bit | 48 Bit |
|---|---|---|---|---|
| `Table` 16 MB | 1.072.292 | **6** | 0 | 0 |
| `Buckets` 16 MB | 6.432.387 | **42** | 0 | 0 |
| `Table` 256 MB | 68.355 | 0 | 0 | 0 |
| `Buckets` 256 MB | 462.711 | 2 | 0 | 0 |
| `Buckets` 1 GB | 110.036 | 0 | 0 | 0 |

Lesart:

- Mit einem **16-Bit-Prüfwort** (Stockfish-Stil) hätte schon dieser Perft-Lauf
  6 bis 42 falsche Treffer und damit falsche Knotenzahlen geliefert.
- **32 Bit**: hier 0 beobachtet, Erwartung ein falscher Treffer pro 4,3 Mrd.
  Fremdvergleiche. Eine lange Mattsuche macht 10^10 bis 10^12 Probes, also
  Dutzende falsche Treffer pro Lauf. Nicht akzeptabel für Beweise.
- **64 Bit** wären 10^-13 falsche Treffer pro Lauf gewesen, **128 Bit** (das
  jetzige Layout) sind exakt.
- Buckets sehen mehr Fremdeinträge als die direkte Tabelle (bis zu vier pro
  Probe), bei gekürztem Prüfwort stiege das Risiko um den Faktor der
  Assoziativität. Mit vollem Key spielt das keine Rolle.
- Die beobachteten Werte streuen stark um die Erwartung (6 statt 16, 42 statt
  98), weil Fremdvergleiche nicht unabhängig sind: Derselbe Probe-Key trifft
  denselben Fremdeintrag oft mehrfach, die Zählung ist geklumpt.

## Persistenz

Beide Packages schreiben einen Rohdump: kleiner Little-Endian-Header (Magic,
Version, Art, Eintragsgröße, Slots, Zobrist-Fingerprint), dann das Array.
Der **Fingerprint** ist ein Hash über alle Zobrist-Konstanten
(`bitboard.ZobristFingerprint`). Eine Datei, die mit anderem Seed oder anderem
Layout geschrieben wurde, wird beim Laden abgewiesen, statt still falsche
Treffer zu liefern. Der Zobrist-Seed ist damit ab jetzt ein Dateiformat-Detail
und darf nicht mehr geändert werden, ohne gespeicherte Tabellen ungültig zu machen.

Perft(7) der Grundstellung, 256-MB-`Table` (24 Value-Bits, die Wurzel und ihre
20 Kinder passen nicht hinein, deshalb braucht der warme Lauf 3.365 Probes
statt eines):

| Schritt | Go | Rust |
|---|---|---|
| kalter Lauf | 3,95 s, 910.786 Stores, Füllgrad 5,2 % | 3,01 s |
| Speichern (256 MB) | 1,49 s | 1,44 s |
| Laden (256 MB) | 140 ms | 93 ms |
| warmer Lauf | 2 ms, 3.365 Probes, 3.106 Treffer | 2 ms |
| falscher Fingerprint | abgewiesen | abgewiesen |

Das Laden ist Seitencache-Geschwindigkeit, das Speichern Plattengeschwindigkeit.
Weil Go und Rust denselben Seed, denselben Generator und dasselbe Layout nutzen,
ist der Fingerprint in beiden Sprachen `c2ab5e380fa70d37`, und **die von Go
geschriebene Datei lädt in Rust** und liefert perft(7) in 2 ms. Das ist nebenbei
der schärfste Gleichheitstest der beiden Ports bisher: Jedes Bit jedes Keys muss
übereinstimmen.

## Store: Hash-Set gegen Sortieren

`ttstore.Store` ist offene Adressierung mit linearem Sondieren auf dem ersten
Key-Wort, gleicher Eintrag wie `tt`, feste Größe. `Put` liefert `(isNew, ok)`;
ab 75 % Füllgrad ist `ok = false` und nichts wird gespeichert (lineares Sondieren
braucht bei 75 % etwa 8 Vergleiche pro Fehlschlag, bei 87 % schon 32). Eine
Besonderheit des Layouts: Weil ein Eintrag beim Sondieren ein paar Slots hinter
seinem Heimat-Slot liegen kann, werden die Index-Bits nur implizit verglichen.
Verglichen werden 104 gespeicherte Bits plus die kurze Sondierdistanz, ein
falscher Treffer hat damit 2^-104 pro Vergleich und wird ignoriert.

Erster Einsatz: die Deduplizierung der Breitensuche aus Milestone 2, bisher
"alle Kinder erzeugen, sortieren, kompaktieren", jetzt "Kind nur behalten, wenn
sein Key neu im Set ist". Das Set wird pro Ebene auf die Hälfte der Kinder
dimensioniert; läuft es voll (Ebene 1 und 2 haben keine Transpositionen), wird
die Ebene mit doppelter Größe wiederholt. Ebene 6 der Grundstellung (20,2 Mio.
Kinder, 9.417.681 eindeutig, OEIS-Wert in beiden Varianten getroffen):

| Variante | Go | Rust | Spitzen-Speicher |
|---|---|---|---|
| sortieren + kompaktieren | 11,38 s | 4,76 s | alle Kinder: 20,2 Mio. × 32 B = 616 MB |
| `ttstore`-Set (2^24 Slots, 56 % voll) | 4,29 s | 3,91 s | 287 MB Sätze + 256 MB Set = 543 MB |

Das Set hält nur die eindeutigen Stellungen, nie alle Kinder, und braucht keinen
zweiten Durchlauf. In Go ist es 2,7-mal schneller als das Sortieren von
32-Byte-Sätzen (`slices.SortFunc` mit `bytes.Compare` ist dort der Flaschenhals),
in Rust 1,2-mal, weil `sort_unstable` auf `[u8; 32]` sehr schnell ist. Für die
listenbasierte Suche ist das Set der richtige Baustein, weil es zusätzlich einen
Value pro Stellung tragen kann (später: Beweisstatus, Distanz zum Matt).

Ein `Store`-Roundtrip gegen eine normale Map (77.796 Keys mit Tiefe als Value,
262.144 Slots) und durch Speichern/Laden ist Teil der Regressionstests.

## Go: GC-Reserve

Gos GC lässt den Heap standardmäßig auf das Doppelte der lebenden Daten wachsen,
bevor er läuft. Mit einer 256-MB-Tabelle wären das 256 MB Reserve für nichts,
bei 1 GB entsprechend mehr. `main.go` setzt deshalb `debug.SetGCPercent(1)`.
Die heißen Pfade allokieren nicht, dort kostet der häufigere GC nichts; die
Perft-Suite läuft mit 1 % genauso schnell wie vorher (192,9 gegen 192,8 Mn/s),
und die allokationsreiche Sortier-Deduplizierung ebenfalls (11,38 s gegen
11,42 s), weil die großen Slices keine Pointer enthalten und der GC sie nicht
durchsuchen muss.

## Erkenntnisse

- Eine TT lohnt sich schon bei Perft enorm: Faktor 4 bis 5 bei 52 % Trefferquote.
- Index-Bits durch den Value ersetzen: voller 128-Bit-Key in 16 Byte, keine
  falschen Treffer, 24 Value-Bits bei 16 Mi Slots. Die Keygrößen-Messung zeigt,
  dass 16 oder 32 Prüfbits für Beweise messbar nicht reichen.
- Buckets zahlen sich aus, sobald die Tabelle zu klein ist, und kosten sonst nichts.
- Eine zu große Tabelle bringt nichts und kostet 2 bis 3 % (TLB). Ihr Leeren
  gehört nicht in die Messung, dafür hat der Runner einen Vorbereitungs-Hook.
- Persistenz ist mit Rohdump trivial und schnell; der Fingerprint ist Pflicht.
- Für Deduplizierung schlägt das exakte Hash-Set das Sortieren, in Go deutlich.
- Go gegen Rust bei identischem Code: Rust 1,3- bis 1,4-mal schneller, die
  Zähler sind identisch.

## Offene Punkte

- Generation (Alterung) in den hohen Value-Bits für die Suche in Milestone 4.
- Gemeinsame TT für mehrere Threads: entweder der klassische Lockless-Trick
  (`word0 ^ word1` speichern, beim Lesen zurückrechnen) oder Buckets mit
  atomaren 16-Byte-Schreibzugriffen. Der Perft-Parallellauf ist der Testfall.
- `ttstore` als Beweisspeicher für df-pn: Proof- und Disproof-Zahlen als Value,
  Speichern/Laden als Fortsetzen einer abgebrochenen Suche.
- Memory-Mapping statt Laden für Tabellen, die größer sind als der RAM.
