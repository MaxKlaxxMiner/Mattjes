# Patricias Netz nativ: Package `nnue` (2026-10-22)

Stufe (1) der Fünf-Stufen-Richtung aus Milestone 6: normale Spielzüge
vorhersagen, mit Patricia nativ eingebaut statt als UCI-Unterprozess.
Dieses Dokument erklärt, was ein NNUE ist, was Patricias Netz konkret
rechnet, wie der Port in Go und Rust aussieht, wie er verifiziert wurde
und was er kostet. Die Quellen liegen in `tests/patricia/` (Klon von
Patricia 5.1, MIT-Lizenz, Adam Kulju), die drei Netzdateien samt Lizenz
in `mattjesGo/nnue/nets/` und werden in beide Binaries eingebettet (4,7
MB).

## Was ein NNUE ist

Ein NNUE ("efficiently updatable neural network") ist ein kleines Netz
mit einer großen ersten Schicht, die sich bei einem Zug billig
fortschreiben lässt. Die Eingabe ist die Stellung als 768 Bits: "Stein
der Farbe c und Art k steht auf Feld s" (2 × 6 × 64). Die erste Schicht
hat 1024 Neuronen; ihr Wert für eine Stellung ist der Bias plus die
Summe der Gewichtszeilen aller gesetzten Bits, also eine Summe über die
rund 30 Steine auf dem Brett. Das ist der **Akkumulator**. Zieht ein
Stein, ändern sich höchstens zwei Bits je Seite (Rochade: König und
Turm), und höchstens zwei verschwinden (Schlag, en passant): der neue
Akkumulator ist der alte plus und minus ein paar Zeilen, nicht die Summe
über alle Steine. Darauf folgt die Aktivierung und eine einzige
Ausgabeschicht.

Zwei Dinge sind Patricia-typisch und bei anderen Netzen oft anders:

- **Zwei Perspektiven.** Es gibt einen Akkumulator aus Sicht von Weiß und
  einen aus Sicht von Schwarz; für Schwarz wird das Brett vertikal
  gespiegelt und die Farben werden getauscht, sodass das Netz "meine
  Steine" immer gleich sieht. Die Ausgabe liest zuerst den Akkumulator
  der Seite am Zug, dann den des Gegners, mit getrennten Gewichten.
  Keine Königs-Buckets, keine horizontale Spiegelung: der Feature-Index
  ist einfach `farbe·384 + art·64 + feld`.
- **Quantisierung.** Alles ist int16. Die Gewichte der ersten Schicht und
  die Biases sind mit QA = 255 skaliert, die Ausgabegewichte mit QB = 64.
  Die Aktivierung ist **SCReLU**: `clamp(x, 0, QA)²`. Die Ausgabe
  `Σ screlu(us_i)·w_i + Σ screlu(them_i)·w_{1024+i}` plus `bias·QA` wird
  mit `SCALE = 400 / (QA·QA·QB)` zu Centipawns.

Drei Netze nach Phase: `feanor.nnue` ("sacrifice", die Vorgabe ab 3500
Material), `finarfin.nnue` (Endspiel, darunter), `fingolfin.nnue`
(Mittelspiel, nimmt Patricias Suche erst, wenn sie hinten liegt). Die
Dateien sind die rohen Arrays als little-endian int16 ohne Kopf:
768 × 1024 Feature-Gewichte, 1024 Biases, 2048 Ausgabegewichte, 1
Ausgabe-Bias = 1.579.010 Byte.

## Was Patricia auf das Netz legt

`evaluate()` ist der rohe Netzwert für die Seite am Zug. Patricias
`eval()` skaliert ihn mit dem Material: `(750 + material/25) / 1024`
(Material: Bauer 100, Springer und Läufer 300, Turm 500, Dame 900, beide
Seiten), ein Zehntel weniger, wenn die Seite am Zug keine Dame hat oder
das Material unter 4000 liegt und der Wert für sie spricht. Dazu kommen
in der Suche Terme, die von der Zughistorie abhängen (Bonus für
gebrachte Opfer, Korrektur-Historien, Skalierung mit der
Fünfzig-Züge-Uhr); für eine einzelne Stellung haben sie keine Bedeutung
und fehlen im Port. `nnue.Evaluate` ist der Wert, den Patricias Suche an
der Wurzel sieht; `nnue.Raw` der reine Netzwert eines gewählten Netzes.

## Der Port

Go `mattjesGo/nnue/` (`net.go`: Konstanten, Laden der eingebetteten
Netze, Feature-Index, Material, Phase; `accumulator.go`: `Accumulator`
mit `Refresh`/`Update`/`Evaluate`, `Delta`/`DeltaOf` aus Brett und Zug,
`Raw`, `Evaluate`), Rust `src/nnue/` (`mod.rs`, `accumulator.rs`) mit
denselben Namen in snake_case.

- **Feldnummern.** Patricia zählt a1 = 0 .. h8 = 63, Mattjes a8 = 0 ..
  h1 = 63: Patricias Feld ist `pos ^ 56`; die schwarze Perspektive
  spiegelt noch einmal (`^ 56`), ist also gerade die Mattjes-Nummer.
  Patricias Artenreihenfolge Bauer 0 .. König 5 ist die Umkehrung des
  Bitboard-Index (König 0 .. Bauer 5): `art = 5 − kind`.
- **Delta aus dem Zug.** `DeltaOf(b, m)` liest Brett und Zug vor dem
  Zug: der gezogene Stein verlässt sein Feld und kommt an (als
  Umwandlungsfigur, falls eine), ein geschlagener Stein verschwindet
  (der En-passant-Bauer von seinem eigenen Feld), bei der Rochade zieht
  der Turm mit. Das läuft neben `DoMove`, nicht in den
  `put`/`remove`-Haken: das Brett bleibt 200 Byte und der Generator
  unberührt; die Suche hält ihren Akkumulator-Stapel selbst.
- **Sättigung.** Die Additionen sättigen im int16-Bereich, wie Patricias
  SIMD-Pfad (`adds_epi16`); mit Gewichten bis ±504 und 32 Steinen wird
  die Grenze nie erreicht, aber so sind beide Pfade dieselbe Funktion.
- **SIMD in Rust.** `std::arch` AVX2 mit Laufzeit-Erkennung
  (`is_x86_feature_detected!`, einmal in einem `OnceLock`), 16 int16-Lanes
  je Register: Zeilen-Addition mit `adds/subs_epi16`, die Ausgabe wie bei
  Patricia mit `clamp` (`max/min_epi16`), `mullo_epi16`, `madd_epi16` in
  int32-Paare, alle 16 Schritte in int64 ausgeleert. Die Ausgabegewichte
  liegen in −126..126, also bleibt `v·w ≤ 255·126` unter 32.767: die
  int16-Multiplikation läuft nie über, der Skalarpfad (und Go) rechnet
  exakt dasselbe. `MATTJES_NNUE_SCALAR=1` erzwingt die Schleifen für die
  Messung. In Go gibt es keinen Vektorpfad (kein Assembler in Go,
  Entscheidung vom 2026-10-09).

## Verifikation

Die Referenz ist Patricia selbst: `tests/patricia-ref.exe` ist ein
lokaler g++-Build von `tests/patricia/` mit einem zusätzlichen
UCI-Kommando `eval`, das für die Stellung den rohen Netzwert aller drei
Netze, Material, Phase und `eval()` druckt (`uci.h`, Kommentar "Mattjes
reference hook"). `tests/patricia-eval.sh <fenfile>` treibt es über eine
Coprozess-Pipe, die Testbank `nnueVerify(fenfile)` /
`tests_nnue::nnue_verify` druckt dieselbe Zeile; `diff` entscheidet.

| Prüfung | Ergebnis |
|---|---|
| `tests/nnue-positions.txt` (20 Mittelspiel- und Taktikstellungen, Perft-Stellungen, Rochade, en passant) | Go und Rust identisch mit der Referenz, alle drei Netze und `eval()` |
| `tests/wdl-positions.txt` (35 Endspiele der WDL-Testbank) | identisch |
| `nnueIncremental(3)`: Zugbäume der Perft-Stellungen bis Tiefe 3, inkrementeller gegen frischen Akkumulator | 287.686 Züge, 0 Abweichungen, Go und Rust, AVX2 und skalar |

Beim ersten Versuch identisch; die einzige Falle war Patricias
`multiplier`: `material/25` ist eine Ganzzahldivision, der Rest float,
das Zehntel wird in double abgezogen.

## Kosten

`nnueBench(3)` / `nnue_bench(3)`: die Zugbäume der Perft-Stellungen bis
Tiefe 3 (287.686 Bewertungen, Prüfsumme 42500238 in allen Läufen),
Sacrifice-Netz, zuhause, Maschine frei, inklusive Zug und Generator des
Laufs:

| Pfad | frisch (Refresh + Evaluate) | inkrementell (Update + Evaluate) |
|---|---|---|
| Rust AVX2 | 1,77 µs, 0,57 M/s | 242 ns, 4,1 M/s |
| Rust skalar (`MATTJES_NNUE_SCALAR`) | 4,5 µs, 0,22 M/s | 1,42 µs, 0,71 M/s |
| Go | 46 µs, 0,02 M/s | 8,2 µs, 0,12 M/s |

Rusts "Skalarpfad" ist von LLVM vektorisiert (`saturating_add` auf i16
wird zu `vpaddsw`), deshalb nur Faktor 2,5 bis 6 zum Handgeschriebenen;
Go vektorisiert nicht und braucht 1,4 ns je Element. Die
`min`/`max`-Builtins und ein Hochziehen der Bereichsprüfung machten Go
langsamer (58 µs), die Verzweigungen bleiben. Für die Politik des
Zertifikat-Bauers, die je Verteidigerknoten rund 30 Kindstellungen
bewertet, sind das in Rust 7 µs je Knoten inkrementell; der Abstieg
expandiert Knoten in derselben Größenordnung.

Offen: das Laden der drei Netze dauert in Rust 200 ms (Go 7 ms), vermutlich
die `Vec`-Kopien des Parsers; und ein Akkumulator-Stapel für die Suche.

## Erster Einsatz: die Politik des Zertifikat-Bauers (gemessen, kein Gewinn)

`Options.NetWeight` (Testbank `n` = 1, `N` = 5 Punkte je Centipawn):
`policyScore` zieht den Netzwert der Kindstellung aus Angreifersicht
ab, der Verteidiger bevorzugt also die Züge, nach denen Patricia den
Angreifer am schlechtesten sieht. Ein Akkumulator je Verteidigerknoten,
die Kinder inkrementell. Zertifikat-Bauer allein (`P`), ohne Tabellen:

| Stellung | ohne Netz | Gewicht 1 | Gewicht 5 |
|---|---|---|---|
| KP-K Randbauer | 1.423 Stellungen, 1.423 sicher | 1.435 / 1.435 | 1.576 / 1.576 |
| KBP-K falscher Läufer | 53.048 / 53.048 | 53.368 / 53.368 | 53.932 / 53.932 |
| KP-K Opposition | 784.607 Stellungen, 1.703 sicher | 783.628 / 1.055 | 783.466 / 717 |
| KNN-K | 4.353.118 / 4.346.771 | 4.651.183 / 4.648.748 | 4.724.098 / 4.721.657 |
| KR-KB b6, KBP-KB, KQ-KRP, KRB-KR, KNN-KP, KQ-KP (offen) | offen in 60 s | offen | offen |

Die Expansionen ändern sich kaum, die Zertifikate werden mit Gewicht
etwas kleiner (die Opposition von 1.703 auf 717 sichere Stellungen), KNN-K
wird um ein Zehntel größer, und keine offene Stellung wird gelöst. Das
ist erwartbar im Nachhinein: das Netz sagt, wer besser steht, der
Zertifikat-Bauer braucht aber den Zug, der die sichere Menge klein hält,
und das ist eine Struktureigenschaft, keine Bewertung. Die Option bleibt
mit Vorgabe 0 als Haken drin, der Term kostet nichts, wenn er aus ist.

## Zweiter Einsatz: die Beweiszahlen des Abstiegs (gemessen, Gewinn)

Dort, wo "wer steht besser" die Frage ist, hilft das Netz: neue Knoten
des Abstiegs bekamen bisher pn = 1 und dn = Mobilität (Angreiferknoten)
bzw. umgekehrt. `Options.NetInit` (Testbank `i` = Stufe 1, `I` = Stufe
2; UCI-Option `WdlNetInit`, Vorgabe an, Stufe 1) skaliert sie nach der
Netzbewertung aus Angreifersicht: Stufe 1 je 150 cp eine Verdopplung,
höchstens drei; Stufe 2 je 100 cp, höchstens fünf. Sieht das Netz den
Angreifer vorn, bleibt die Beweiszahl klein und die Widerlegungszahl
wächst (der Beweis soll billig sein, die Widerlegung teuer), sieht es
ihn hinten, umgekehrt (`biasNumbers`, df-pn mit heuristischer
Initialisierung). Ein Akkumulator je expandiertem Knoten, die Kinder
inkrementell; nur im Abstiegsgraphen. Abstieg allein (`p`), Expansionen,
Go = Rust auf die Zahl:

| Stellung | ohne Netz | Stufe 1 | Stufe 2 |
|---|---|---|---|
| KQ-KN (ohne Tabellen) | 136.751 | 174.267 | 144.724 |
| KBB-K | 305.441 | 263.451 | 288.296 |
| KQ-KR | 5.930 | 4.472 | 9.609 |
| KBN-K | 2.301.901 | 1.787.220 | 1.727.493 |
| KRR-KN Benchmark, "Weiß gewinnt" (Vier-Steiner-Basis) | 10.272 | 5.004 | 8.237 |
| KRRN-KN einsamer Springer, Weiß gewinnt (Basis) | 44.520 | 30.809 | 29.313 |
| KQ-KBN (ohne Tabellen) | offen bei 16 Mi | offen | offen |

Stufe 1 gewinnt überall außer bei KQ-KN (ein Viertel mehr) und ist die
Vorgabe: KRR-KN halbiert, der einsame Springer ein Drittel weniger,
KBN-K ein Fünftel. Mit Syzygy sind die Sechs- und Sieben-Steiner-Wurzeln
Tabellentreffer, dort ändert sich nichts. UCI-Rauchtest KQQ-KN: 3
Gewinne und 2 Remis wie vorher (79.574 statt 51.311 Stellungen, Go und
Rust gleich). Kosten: in Rust 1,8 µs je Expansion plus 0,24 µs je Kind,
im Rauschen; in Go 46 µs plus 8 µs je Kind, dort spürbar.

**Gemessen und verworfen: der Rang unter den Geschwistern.** Statt des
absoluten Werts der Rang des Zugs unter den Kindern des expandierten
Knotens (der beste aus Sicht des Ziehenden behält seine Zahlen, der
r-te bekommt die Zahl gegen sich r-mal verdoppelt, höchstens dreimal):
KQ-KN 365.732 Expansionen, KBB-K 373.157, KQ-KR 14.277, KRR-KN "Weiß
gewinnt" 116.238 (statt 5.004), einsamer Springer 204.096 (statt
30.809); nur KBN-K besser (1.499.552). Der Rang verliert die
Information, wie klar die Stellung ist: ein knapper zweiter Zug und ein
hoffnungsloser bekommen dieselbe Strafe. Ausgebaut.

## Dritter Einsatz: Rollouts, Package `rollout` (2026-10-22)

Stufe 2 der M6-Richtung: eine Stellung ausspielen, bis Gewinn, Verlust
oder Remis klar ist. `rollout.Play(root, Options{MaxPlies, Oracle})`
spielt beide Seiten nach dem Netz und endet bei einem Tabellentreffer
(eigene Tabellen, Syzygy, totes Material), Matt, Patt, Wiederholung
oder der Längengrenze (Vorgabe 200 Halbzüge); Ergebnis für die Seite am
Zug der Ausgangsstellung, mit Linie, Abbruchgrund und Zahl der
Bewertungen. Ein Halbzug: jedes Kind wird bewertet, ein Akkumulator je
Stellung, die Kinder inkrementell, das Netz nach dem Material gewählt;
ein Kind, das das Orakel für den Gegner als verloren kennt, wird sofort
genommen (der Tabellengewinn ist der Zweck), ein für ihn gewonnenes nie,
ein bekanntes Remis zählt null, Matt in eins wird erkannt. Dazu zwei
Korrekturen, beide aus dem ersten Lauf:

- **Wiederholungen meiden.** Der gierige Spieler pendelte; mit beiden
  Seiten gierig endete sogar KQ-K nach 16 Halbzügen als Wiederholung.
  Ein Kind, das schon auf der Linie liegt, bekommt die Strafe 2^28 und
  wird nur genommen, wenn nichts anderes bleibt.
- **Hängende Figuren.** Die statische Bewertung sieht nicht, dass die
  Dame im Griff des Königs steht (Dc6+ Kxc6 in KQ-K). `hanging` schätzt
  den besten Schlag des Gegners (geschlagener Stein minus Schläger,
  wenn auf dem Feld zurückgeschlagen werden kann) und zieht ihn doppelt
  ab.

**Maßstab:** `rolloutPositions(names, options, maxPlies)` spielt jeden
Wurzelzug der WDL-Teststellungen aus und vergleicht die geratenen
Gewinne, Remis und Verluste mit den bekannten Zahlen ("höchstens n von
m richtig" ist die Überdeckung der Klassen, ohne Tabellen stehen die
Züge einzeln nicht fest). Go und Rust liefern dieselben Zeilen.

| Orakel | richtig von 567 Wurzelzügen | Bemerkungen |
|---|---|---|
| Vier-Steiner und Syzygy | 561 | fast alles Sofort-Treffer; Sieben-Steiner: KRRN-KNP 27/27 nach 3,3 Halbzügen, "ein Remis" 15/15, "vier Remis" 13/17 und "zwei Remis" 8/10 (Remisstellungen für Gewinne gehalten) |
| nur die Vier-Steiner | 478 | KQQ-KN 39/39, KRR-KN 12/12, einsamer Springer 26/27 und 9/10, KRRN-KNP 20/27, KQ-KBN 6/24 (der Läufer-Springer-Block wird in 200 Halbzügen nicht geknackt) |
| totes Material allein | 284 | KQQ-KN wird mattgesetzt (37/37), KQ-K, KR-K, KBB-K nicht in 200 Halbzügen: ohne Suche keine Matttechnik, das ist Tabellensache |

Vor den zwei Korrekturen: 241 bzw. 460. Kosten: Rust 156 ms für alle
567 Rollouts mit den Vier-Steinern, Go 1,9 s.

**Tiefe 2** (`Options.Depth`, Testbank `d`): statt des Kindes, das das
Netz am liebsten mag, das Kind, dessen schlechteste Antwort das Netz am
liebsten mag (Minimax über zwei Halbzüge, `replyMin`: das Blatt aus
Sicht des Ziehenden plus das Doppelte dessen, was er dort frei schlagen
kann; eine Antwort, die mattsetzt oder per Tabelle gewinnt, widerlegt
das Kind). Der gierige Gegner lief in Verluste und ließ Remis wie
Gewinne aussehen; mit Tiefe 2 hält der Spieler Remis für Verluste, was
für die Zugordnung unschädlich ist.

| Orakel | Tiefe 1 | Tiefe 2 | Kosten Rust (alle 567) |
|---|---|---|---|
| Vier-Steiner und Syzygy | 561 | 561 | 15 / 53 ms |
| nur die Vier-Steiner | 478 | 516 | 167 / 682 ms |
| totes Material, 100 Halbzüge | 237 | 349 | 274 ms / 1,41 s |

Mit den Vier-Steinern springen KQ-KBN von 6 auf 22 von 24, Philidor
von 11 auf 19 von 19, "zwei Remis" auf 10 von 10, "vier Remis" auf 15
von 17, der einsame Springer schwarz auf 10 von 10; ohne Tabellen
findet Tiefe 2 erste Matts mit der blanken Dame (KQ-K 7 von 20, KQ-KN 21
von 24). Go = Rust auf die Zahl (Go 9,4 s für den Vier-Steiner-Lauf).
Tiefe 2 ist die Vorgabe im UCI-Treiber (`WdlRollouts`).

**Im UCI-Treiber** (`WdlRollouts`, Vorgabe an): vor den Runden spielt
`runWdl` jeden offenen Wurzelzug einmal aus (mit dem Orakel der Suche)
und meldet `info string wdl: rollouts guess W win, D draw, L loss, U
unknown of N open moves in T`. Geratene Gewinne kommen zuerst an die
Reihe (dann unbekannt, Remis, Verluste), und jeder Zug bekommt seine
geratene Frage zuerst: ein geratener Gewinn "wir gewinnen", ein
geratener Verlust "der Gegner gewinnt", der Rest die Materialregel.
A/B mit `tests/uci-wdl.sh <binary> tests/uci-ab.txt 20000 3 2048
"SyzygyPath value ...;WdlRollouts value false"` (das Skript nimmt jetzt
weitere Optionen mit `;` getrennt und druckt die Schätzzeile), Rust mit
Syzygy, MultiPV 3:

| Stellung | Schätzung | Stellungen mit / ohne Rollouts |
|---|---|---|
| KRRN-KN einsamer Springer, Weiß | 27 Gewinne von 27, 0,3 ms | 58 / 112 |
| KRRN-KNP Sieben-Steiner | 27 von 27, 4,4 ms | 112.335 / 147.321 (0,38 / 0,41 s), **Rauschen**: die Summe hängt im Wettrennen vom Timing ab, mit Tiefe 2 wieder 147.321 |
| KQQ-KN | 37 von 37 | 82 / 82 |
| KRR-KN Benchmark, Schwarz | 12 Verluste von 12 | 28 / 52 |

Der belastbare Gewinn ist die gesparte falsche Frage je Zug an den
Stellungen, die deterministisch enden (einsamer Springer 58 statt 112,
KRR-KN 28 statt 52 Stellungen); die Rollouts selbst kosten in Rust
Mikrosekunden bis Millisekunden, in Go das Zehn- bis Fünfzigfache. Wo
die Tabellen alles sofort entscheiden, ändert sich nichts; wo die
Beweise lang sind (einsamer Springer ohne Fünf-Steiner-Tabellen: null
Gewinne in 20 s mit und ohne), auch nicht. Go liefert dieselben
Schätzungen. (Seit Syzygy in der Tabellenstufe antwortet, erreichen
nur noch Sieben-Steiner und Stellungen ohne Tabellen die Rollouts.)

## Nächster Schritt

Die Rollout-Linie als Vorgabe für den Abstieg (kleine Beweiszahlen
entlang der Linie), gemessen an den Gewinnen ohne Tabellen; und
Rollouts mit Breite (die zwei besten Züge, Rückkehr), um die "vier
Remis" und "zwei Remis" nicht mehr für Gewinne zu halten.
