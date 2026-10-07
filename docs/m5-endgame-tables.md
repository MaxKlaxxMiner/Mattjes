# Milestone 5: Endspieltabellen, Ergebnisse

Stand 2026-10-05, Arbeitsrechner (i5, 12 Threads, siehe `docs/machines.md`). Entwurf in
`docs/m5-endgame-tables-design.md`, Code in `mattjesGo/egtb` und `mattjesRs/src/egtb`.

## Was gebaut wurde

Alle 35 Materialien bis vier Steine als DTM-Tabellen (1 Byte pro Stellung, aus Sicht der Seite
am Zug), komplett im RAM, 173 MB. Einmal gerechnet, dann als eine Cache-Datei
`mattjes-egtb.bin` neben der Binary, ohne Header, Prüfsumme als Konstante im Code.

| Schritt                               | Go     | Rust   |
|---------------------------------------|--------|--------|
| alle 35 Tabellen erzeugen (12 Threads) | 17,1 s | 15,2 s |
| Vorwärts-Verifikation aller Tabellen   | 8,6 s  | 7,3 s  |
| Cache-Datei laden (173 MB) + Prüfsumme | 0,19 s | 0,13 s |

Beide Generatoren liefern **byteidentische** Tabellen (35 gleiche Prüfsummen, eine
Datei-Prüfsumme `0x865d59cc4efc880b`). Die Rust-Datei lädt in Go und umgekehrt.

## Der Weg dahin: zwei Fehler, die die Selbstprüfung gefangen hat

Der Generator prüft bei jeder Entscheidung die Ebenen-Invariante (ein Gewinn in Ebene n muss
genau ein Kind mit Verlust in n − 1 haben, ein Verlust genau ein längstes Kind mit n − 1).
Zwei Fehler sind daran gescheitert, nicht an einem Test von außen:

1. **Alle Kinder verloren ≠ alle Kinder gewonnen.** Eine Stellung, deren Kinder sämtlich
   Verluste für den Gegner sind (also ein Gewinn, der noch auf seine Ebene wartet), erfüllte
   in der geraden Ebene die Bedingung "alle Kinder gewonnen" leer. Ein verlorenes Kind muss
   die Verlust-Bedingung ausschließen.
2. **Diagonal-Zwillinge.** Stehen beide Könige auf der Diagonale, legten die Könige die
   Transposition nicht fest. Der Entwurf nannte das "Redundanz, kein Fehler": Stellung und
   Spiegelbild bekommen zwei Indizes. Für die Kandidaten-Methode ist es ein Fehler, denn die
   Kinder beider Zwillinge fallen auf dieselben Indizes, und der Rückwärtszug vom Kind erreicht
   nur einen Zwilling. Lösung (Standard): der kleinere der beiden Indizes ist kanonisch, der
   andere ein toter Index. Eine Tabelle KBNK schrumpft dadurch von 3.138.965 auf 3.067.466
   legale Stellungen.

## Zwei Generator-Varianten

**Ebenen-Iteration pur** (jede Ebene scannt alle unbekannten Stellungen): korrekt und einfach,
aber Remis-lastige Tabellen scannen in jeder Ebene fast alles. KBNK 5,4 s, KQKR 12,2 s,
KRKR nach 100 s abgebrochen.

**Kandidaten** (die jetzige Variante): Rückwärtszüge markieren nur die Eltern frisch
entschiedener Stellungen, bewertet wird ausschließlich vorwärts mit derselben Logik.
KBNK 0,4 s, KQKR 0,8 s, KRKR 0,4 s. Die Rückwärtszüge sind damit ein reiner Filter: Ein
fehlender Rückwärtszug kann eine Stellung übersehen, aber keine falsch bewerten. Übersehene
Stellungen findet die **Vorwärts-Verifikation**, die jede Stellung nur aus ihren Kindern
nachrechnet (`Verify`). Beides zusammen ist die im Entwurf geforderte Prüfung
"Rückwärtszug-Generator gegen Vorwärts-Generator".

Konsistenz impliziert Korrektheit: Matts sind kinderlos bestimmt, Gewinn in 1 braucht ein
Matt-Kind, und so weiter per Induktion. Ein falscher Wert ohne Widerspruch zu seinen Kindern
ist nicht möglich.

## Längste Matts (Züge)

| Material | Tabelle | Literatur | | Material | Tabelle | Literatur |
|----------|---------|-----------|-|----------|---------|-----------|
| KQK      | 10 | 10 | | KQKQ | 13 | 13 |
| KRK      | 16 | 16 | | KQKR | 35 | 35 |
| KQQK     | 4  |    | | KQKB | 17 | 17 |
| KQRK     | 6  |    | | KQKN | 21 | 21 |
| KQBK     | 8  |    | | KRKR | 19 | 19 |
| KQNK     | 9  |    | | KRKB | 29 | 29 |
| KRRK     | 7  |    | | KRKN | 40 | 40 |
| KRBK     | 16 |    | | KBKB, KBKN, KNKN | 1 | |
| KRNK     | 16 |    | | KPK  | 28 | 28 |
| KBBK     | 19 | 19 | | KQKP | 29 | |
| KBNK     | 33 | 33 | | KRKP | 43 | |
| KNNK     | 1  |    | | KBKP, KNKP | 29 | |
| KQPK     | 10 |    | | KPPK | 32 | |
| KRPK     | 16 |    | | KPKP | 33 | 33 |
| KBPK     | 31 |    | | | | |
| KNPK     | 27 |    | | | | |

Alle Literaturwerte stimmen. (Mein ursprünglich notierter Wert 27 für KRKN war eine
Erinnerungslücke, die Literatur sagt 40, die Tabelle auch; die Vorwärts-Verifikation hatte
die Tabelle unabhängig bestätigt.) Die Teststellungen aus `chess/matedata.go` bis vier Steine
liefern exakt 2N − 1 Halbzüge: Matt in 3, 5, 7, 12, 15, 17 und 31.

## Anbindung an die Suche

`mateab.Tables` ist das Orakel mit Tabellen: bis vier Steine kommt der exakte Wert mit
Distanz, sonst der Materialtest wie bisher. Die Suche nutzt die Distanz: Passt sie in die
Resttiefe, ist der Knoten fertig ("Matt in n"), sonst gibt es in dieser Tiefe kein Matt.
Stellungen mit EP-Recht beantwortet die Tabelle nicht, die Suche spielt weiter.

Die sieben Teststellungen bis vier Steine sind damit Wurzeltreffer (ein Knoten pro
Vertiefungsstufe). Der Bauerntest (9 Steine) läuft unverändert mit 464.248 Knoten, in Go und
Rust gleich. Offen: KQ-KBN (Matt in 39, fünf Steine), das bisher außer Reichweite war; mit
Tabellen wird jeder Schlagzug zum Blatt.

## Neue Teststellung: KP-KP (Oppositions-Studie)

`8/7k/1p6/1P6/7K/8/8/8 w` (Könige h4/h7, Bauern b5/b6, vom Autor vorgeschlagen). Die
KPKP-Tabelle antwortet sofort: **Weiß am Zug gewinnt in 49 Halbzügen (Matt in 25)**, Schlüssel
1.Kh5! (1.Kg5? Kg7 hält); **Schwarz am Zug: Remis**. `egtbProbe` folgt der Tabelle bis zum
Matt: 1.Kh5 Kg7 2.Kg5 Kf7 3.Kf5 Ke7 4.Ke5 Kd7 5.Kd5 Kc7 6.Ke6 Kb8 7.Kd7 Kb7 8.Kd6 Ka8 9.Kc7
Ka7 10.Kc6 Ka8 11.Kxb6 Kb8 12.Ka6 Kc7 13.Ka7 Kc8 14.b6 Kd7 15.b7 Ke6 16.Kb6 Kf6 17.Kc6 Ke5
18.b8D Kd4 19.Df4 Kc3 20.Kb5 Kd3 21.Kb4 Kc2 22.Kc4 Kb2 23.Dd2 Ka3 24.De2 Ka4 25.Da2#.

Unabhängige Bestätigung durch `matelist` (Hash-basierte Breitensuche plus Retrograde, anderer
Code-Pfad als die Index-Tabellen): 2.878.165 erreichbare Stellungen, 30,2 Mio. Kanten,
Matt in 25 mit derselben Hauptvariante in 4,0 s; Beweisgraph nur 4.125 Stellungen. Die
Stellung ist als `KP-KP` in `chess/matedata.go` aufgenommen (Matt in 25).

## Fünf Steine: Messreihe (ab 2026-10-07)

Der Generator kann jedes Material bis sechs Steine, dessen Abhängigkeiten er
rekursiv selbst erzeugt. Der Cache ist jetzt ein Ordner `mattjes-egtb-cache/`
neben der Binary: `4-all.bin` für alle Drei- und Vier-Steiner (die bisherige
Datei) und je eine Datei pro größerem Material, `5-KBNKQ.bin` usw. Größere
Materialien werden nie mitgeliefert, nur lokal erzeugt; der UCI-Modus könnte
sie bei genug RAM on the fly rechnen, während die Suche schon läuft. Die
Kanonisierung nennt die Seite mit mehr Steinen zuerst, deshalb heißt KQKBN
intern KBNKQ (die Suche dreht die Farben beim Nachschlagen).

Wertebereich: Die erste Fassung speicherte Halbzüge und fasste 126; beim ersten
Durchlauf aller Fünf-Steiner meldete KBBKN "distances exceed 126 plies" (der
längste Gewinn dort sind Thompsons 66 Züge = 131 Halbzüge), die tiefsten
Gewinne wären als Remis in der Tabelle gestanden. Seit dem 2026-10-07 speichert
das Byte die **Zugzahl** (Gewinne sind immer ungerade, Verluste immer gerade
Halbzüge), Reichweite 253 Halbzüge, alle Fünf-Steiner passen. Alle Prüfsummen
wurden dabei neu; der Cache wurde geleert und neu erzeugt. Erst die längsten
Sechs-Steiner (KRNKNN mit 262 Zügen) sprengen auch das; der Generator meldet es
mit "distance range exceeded" und lässt die offenen Stellungen als Remis stehen.

Arbeitsrechner (i5, 12 Threads), Go:

| Material | roh ohne Symmetrie | Indizes = Datei | legal | Gewinne / Verluste / Remis | längstes Matt | Ebenen | Bewertungen | Zeit | Prozess-RAM | Prüfsumme |
|---|---|---|---|---|---|---|---|---|---|---|
| KBNKQ (= KQKBN) | 1,64 Mrd. | 242.221.056 = 231 MiB | 149.985.528 | 86,9 Mio. / 56,6 Mio. / 6,4 Mio. | 105 Halbzüge = 53 Züge | 106 | 558,7 Mio. | **47,2 s Go / 44,8 s Rust** | 845 MB (355 vorher) | `0xb52d6cdf4e9bee6f` |
| KBBBK | 1,64 Mrd. | 242.221.056 = 231 MiB | 28.017.470 | 8,1 Mio. / 11,7 Mio. / 8,2 Mio. | 31 Halbzüge = 16 Züge | 38 | 34,8 Mio. | 4,0 s Rust | | `0x985cad50bec78a9d` |

(Prüfsummen im Zugzahl-Format; die weiteren 108 Fünf-Steiner misst der Autor mit
`measure-egtb.bat`, Ergebnisse folgen hier.)

Die Schätzung "zehn Minuten" war um Faktor 13 zu pessimistisch: 12 Mio.
Kandidaten-Bewertungen pro Sekunde, der Prozess braucht kaum mehr als die
Tabelle selbst plus die drei Kandidaten-Bitsets (3 × 30 MB) und die Basis.
Go und Rust erzeugen die Datei **byteidentisch** (`cmp`), Rust nur 5 % schneller:
Die Kandidatenphase ist speichergebunden wie die Listensuche. Die
Vorwärts-Verifikation aller 242 Mio. Indizes meldet 0 Abweichungen in 12,7 s.
Die Teststellung KQ-KBN (Matt in 39, tablebase-geprüft) steht in der fertigen
Tabelle als "Gewinn in 77 Halbzügen", exakt wie erwartet.

Hochrechnung für sechs Steine ohne Bauern: 462 × 64⁴ × 2 = 15,5 Mrd. Indizes
pro Material, also 15,5 GB Tabelle plus 5,8 GB Bitsets, und bei gleicher Rate
rund 50 Minuten bis zwei Stunden pro Material, je nach Ebenenzahl. Das ist
zuhause (127 GB) machbar, am Arbeitsrechner nicht.

## Nächster Generator-Schritt: Bauern-Scheiben (vorgemerkt 2026-10-07)

Teiltabellen sind exakt, solange die Teilmenge unter Vorwärtszügen abgeschlossen
ist (jedes Kind liegt in der Teilmenge oder in einer anderen vorhandenen
Tabelle). Zwei Formen lohnen sich:

- **Läuferfarbe:** ein Läufer wechselt die Feldfarbe nie, 32 statt 64 Felder,
  dafür nur 4-fache Symmetrie (Spiegelungen vertauschen die Farben, Transposition
  und 180°-Drehung nicht; 924 Königspaare). Zwei Läufer ergeben zwei Tabellen
  (gleich-/ungleichfarbig), eine Wurzel braucht nur eine. Faktor 2.
- **Bauern-Scheiben** (pawn slices, so arbeiten auch Syzygy und Nalimov): Tabelle
  nach den Bauernfeldern geordnet. Bauern gehen nur vorwärts, nach einem Schlag
  eine Linie zur Seite, Bauernzüge führen also immer in eine spätere Scheibe.
  Jede Scheibe ist eine eigene Kandidaten-Retrograde mit den späteren Scheiben
  als fertigen Kindern (wie heute die Schläge in kleinere Tabellen). Von einer
  Wurzel aus sind nur die erreichbaren Scheiben nötig (Bauer b5: etwa 10 von 48
  Feldern), Speicher ist nur eine Scheibe plus die Bytes der späteren. Damit
  werden Sechs-Steiner mit Bauern für eine konkrete Wurzel machbar (volle
  Tabelle 45 Mrd. Indizes) und der On-the-fly-Fall im UCI-Modus realistisch.
  Prüfsummen solcher Teiltabellen sind wurzelabhängig, geprüft wird per
  Vorwärts-Verifikation.

## Kompression der Cache-Dateien: Vorstudie (2026-10-07)

Wunsch des Autors: etwas Einfaches, nativ geschriebenes (kein zlib), Dekompression
deutlich über 250 MB/s, Kompression nicht langsamer als die Erzeugung, trotzdem
möglichst gute Rate. Messung an den drei vorhandenen Dateien (Raten exakt,
Geschwindigkeiten unter Last und mit naivem Byte-für-Byte-Decoder, nur Richtwerte):

| Datei | Anteil 128 (ungültig) | Entropie 0. Ordnung | 1. Ordnung | Byte-RLE | RLE nach "ungültig = egal" | LZ77 (LZ4-Stil, 64 KB Fenster) | LZ nach "egal" |
|---|---|---|---|---|---|---|---|
| 4-all (173 MB) | 25,8 % | 1,92× | 3,49× | 1,19× | 1,55× | 2,38× | **2,67×** |
| KBNKQ (231 MB, dicht) | 38,1 % | 1,75× | 2,25× | 0,80× | 1,06× | 1,64× | **1,69×** |
| KBBBK (231 MB, dünn) | 88,4 % | 8,3× | 14,5× | 4,45× | 5,75× | **11,1×** | 10,9× |

Nachtrag mit den Codecs des Autors (`webwerfgo/lz.go`, `rle_test.go`, 2026-10-06):
PackBits-artiges RLE (Steuerbyte: Literalblock bis 128 oder Lauf 3..130) und ein
LZ4-Blockformat mit Hash-Ketten (Tiefe 256), Lazy Matching, 4-MB-Blöcken, parallel.

| Datei | RLE-B | RLE-B, ungültig = egal | LZ (Hash-Ketten, lazy) | LZ, ungültig = egal | LZ unpack (12 Threads, unter Last) |
|---|---|---|---|---|---|
| 4-all | 1,62× | 1,94× | **4,02×** | **4,61×** | 1,1 bis 2,2 GB/s |
| KBNKQ | 1,30× | 1,50× | **2,00×** | **2,11×** | 0,9 bis 1,4 GB/s |
| KBBBK | 6,72× | 7,57× | **20,6×** | **21,1×** | 2,0 bis 2,2 GB/s |

Das LZ mit Hash-Ketten schlägt die Entropie nullter Ordnung (4,0× gegen 1,92× bei
4-all), weil es die Wiederholungen zwischen den 64-Byte-Zeilen des Index (gleiche
Könige und erster Stein, Nachbarfeld des letzten) als Matches findet; das naive
Greedy-LZ oben kam nur auf 2,4×. Packen kostete unter Last 28 bis 115 MB/s mit 12
Threads, also 2 bis 6 s pro Tabelle, weit unter der Erzeugungszeit.

Erkenntnisse:

- **Naives RLE (Paar Wert/Länge pro Lauf) vergrößert dichte Tabellen**, ein
  PackBits-Format mit Literalblöcken nicht; aber auch das bringt auf dichten
  DTM-Tabellen nur 1,3 bis 1,6× (mittlere Lauflänge 1,6 bei KBNKQ). Nur die Remis- und
  Ungültig-Wüsten dünner Tabellen laufen lang.
- **"Ungültig = egal":** Der Wert 128 ist vollständig redundant (folgt aus der
  Geometrie, wird nie nachgeschlagen). Ersetzt man ihn vor der Kompression durch den
  Vorgängerwert, verlängern sich die Läufe; bringt 10 bis 30 % bei RLE, wenig bei LZ.
  Nachteil: `Verify` muss ungültige Einträge dann überspringen, und die Prüfsumme der
  Rohdaten gilt nur für die dekomprimierte Form mit wiederhergestellten 128ern (oder man
  prüft die komprimierte Datei).
- **LZ77 im LZ4-Stil** (Token, Literale, 2-Byte-Offset, Läufe als überlappende Matches)
  liegt mit 1,7× bis 11× nahe an der Entropie nullter Ordnung, nutzt Wiederholungen
  zwischen den 64-Byte-Zeilen des Index (Nachbarfelder des letzten Steins) und ist
  mit rund 150 Zeilen pro Sprache einfach. Ein richtiger Decoder (Wort- statt
  Byte-Kopien, 1-MB-Blöcke parallel) liegt bei über 1 GB/s pro Kern, die Kompression
  mit Hash-Tabelle bei einigen hundert MB/s, also weit unter der Erzeugungszeit.
- Mehr als etwa 2× auf dichten Tabellen braucht Kontextmodellierung (1. Ordnung wäre
  2,25×) oder Huffman auf Literalen; das ist die Stufe von Nalimov/Syzygy und für
  später.

**Umgesetzt (2026-10-07):** Package `lz` in Go und Rust (das LZ des Autors: LZ4-
Blockformat, Hash-Ketten Tiefe 256, Lazy Matching, 4-MB-Blöcke parallel, Längen 64 Bit
für Sechs-Steiner). Dateiformat: 4 Byte Kennung `MEGT`, 8 Byte Roh-Prüfsumme (die
Konstante aus dem Code, über die Werte mit 128ern, damit bleiben `measure.log` und alle
Konstanten gültig), dann der LZ-Container. Vor dem Packen werden ungültige Einträge
durch den Vorgängerwert ersetzt ("ungültig = egal"); die Suche fragt sie nie ab,
`Verify` überspringt sie. Rohdateien (Größe = Tabellengröße) werden weiter gelesen,
`egtb-compress` wandelt einen Cache-Ordner um (die .bat ruft es am Ende auf). Go und
Rust schreiben byteidentische Dateien. Ergebnis auf den drei Dateien (unter Last):

| Datei | roh | gepackt | Faktor | packen (12 Threads) | laden Rust / Go (8 Threads) |
|---|---|---|---|---|---|
| 4-all | 173 MB | 37 MB | 4,6× | 14 s (Go) | 0,24 s / 0,2 s |
| KBNKQ | 231 MB | 109 MB | 2,1× | 23 s (Go) | |
| KBBBK | 231 MB | 10 MB | 21× | 6 s (Go), 2,8 s (Rust) | 0,22 s / 0,85 s |

Geschwindigkeiten gelten mit Vorbehalt (Messreihe lief parallel); Go lädt hier
langsamer als Rust, Ursache noch offen (Allokation, Blockkopie). Erwartung über alle
Fünf-Steiner: Faktor 2 bis 4.

## Offen

- Weitere Fünf-Steiner messen (vor allem die mit Bauern, deren Abhängigkeiten
  Fünf-Steiner-Promotionen sind), erster Sechs-Steiner zuhause.
- Optimierungsfragen: Skalierung über die Worker (14 zuhause), Speicherbandbreite
  in der Kandidatenphase, zwei Byte pro Stellung für lange Materialien.
- Syzygy-Leser für alles, was nicht selbst gerechnet wird (Entscheidung M5).
- Die Suche verlängert ihre PV noch nicht aus der Tabelle heraus (die PV endet am
  Tabellen-Blatt); `egtbProbe` zeigt, wie billig das ist: pro Halbzug einmal Züge erzeugen
  und das Kind mit Distanz n − 1 wählen.
- Gleiche Steine (KQQK, KBBK, KPPK) belegen die Hälfte ihrer Indizes als tote Einträge
  (Dreiecks-Indizierung würde sie sparen, 24 MB von 173).
