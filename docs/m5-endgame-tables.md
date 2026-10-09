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

### Alle 110 Fünf-Steiner (Messlauf 2026-10-07)

Batch-Skript (seit Format v3 entfallen, siehe unten) auf dem Arbeitsrechner (i5, 12 Threads), Rust-Binary, ein Prozess
pro Material, jede Tabelle anschließend vorwärts verifiziert. Start 16:37, Ende 18:36.
Weder `panicked` noch `MISMATCH` im Konsolenlog, alle 110 Verifikationen melden 0
Abweichungen. Alle Prüfsummen stehen seitdem als Konstanten im Code
(`egtb.TableChecksums` / `TABLE_CHECKSUMS`), damit ist jede weitere Erzeugung ein
Regressionstest.

| | Summe |
|---|---|
| Materialien | 110 (60 ohne Bauern à 231 MiB, 36 mit einem Bauern à 677 MiB, 12 mit zwei Bauern à 507 MiB, 2 mit drei Bauern à 380 MiB) |
| Indizes | 47.288.844.288 = 44,0 GiB roh, mit Basis 47,47 GB |
| legale Stellungen | 25,87 Mrd. |
| Erzeugung | 4.953 s = 83 min (3 s KNNKB bis 149 s KRPKQ, 12 bis 13 Mio. Bewertungen/s) |
| Verifikation | 1.955 s = 33 min |
| komprimiert (`egtb-compress`, danach) | 10.116.567.003 Byte, mit Basis 10,16 GB, **Faktor 4,67** |

Literatur-Maxima (Nalimov-DTM), die die Tabellen treffen: KBBKN 78, KBNKN 107, KNNKP 115,
KQPKQ 124, KPPKP 127 Züge. Diese fünf stehen jetzt in `KnownMaxima` und werden bei jeder
Messung geprüft. Die weiteren Maxima der Tabelle (etwa KRBKR 65, KRPKR 74, KQRKQ 67) habe
ich nicht gegen die Literatur geprüft, sie sind nur unsere Werte.

**KPPKP sprengt den Wertebereich (Überlauf).** Der längste Gewinn ist 253 Halbzüge =
127 Züge und trifft die Literatur, aber die Tabelle meldet "distances exceed 253 plies".
Gewinne können bis 253 Halbzüge gespeichert werden (ungerade), Verluste nur bis 252
(gerade). Der Verteidiger am Zug, der erst nach 254 Halbzügen matt ist, passt nicht ins
Byte: Der Generator bricht nach Ebene 253 ab, diese Stellungen bleiben 0 und lesen sich
als Remis. Seit diesem Lauf rechnet der Generator eine letzte Ebene 254 trocken (nur
zählen, nichts speichern): **mindestens 7 Stellungen** fehlen in KPPKP, Go und Rust
zählen identisch, die Prüfsumme bleibt `d2f3cdafa0bb5ca5`. Es sind ausschließlich
Verluste in 254 Halbzügen für die Seite am Zug, alle anderen 163.262.453 legalen
Stellungen stimmen. Ausweg ohne zweites Byte: den Wert 128 (ungültig) aufgeben, dann
reichen 128 Verlustwerte bis 254 Halbzüge, tote und illegale Indizes würden der Generator
in einem eigenen Bitset führen und `Verify` über `Index(Decode(i)) != i` erkennen; in der
Datei sind sie schon heute "egal". Das ändert jedes Byte 128 in 0, also alle 146
Prüfsummen und erfordert eine Neuberechnung (zwei Stunden mit Verifikation). Für
Sechs-Steiner (KRNKNN 262 Züge) braucht es ohnehin zwei Byte pro Stellung.

Die vollständige Anforderungstabelle. Spalten: Indizes in MiB (= Rohdatei), legale
Stellungen, Gewinne / Verluste / Remis in Millionen, längstes Matt in Halbzügen = Zügen,
Ebenen, Bewertungen in Millionen, Erzeugung in Sekunden, Verifikation in Sekunden,
komprimierte Datei in MiB mit Faktor, Prüfsumme (Zugzahl-Format, über die Rohwerte mit
128ern):

| Material | MiB | legal | G / V / R (Mio.) | Matt | Ebenen | Bew. (Mio.) | s | Verify s | gepackt MiB | Prüfsumme |
|---|---|---|---|---|---|---|---|---|---|---|
| KBBBK | 231 | 28.0 | 8.1 / 11.7 / 8.2 | 31 = 16 | 38 | 34.8 | 3.9 | 2.5 | 11 (21.1×) | `985cad50bec78a9d` |
| KBBKB | 231 | 82.9 | 6.0 / 0.6 / 76.3 | 43 = 22 | 44 | 42.2 | 5.1 | 6.3 | 8 (30.2×) | `35aeff97f4dcf6e9` |
| KBBKN | 231 | 85.4 | 18.5 / 15.9 / 51.0 | 155 = 78 | 156 | 143.2 | 10.6 | 6.2 | 34 (6.8×) | `d4e475056311e413` |
| KBBKQ | 231 | 72.5 | 38.7 / 25.0 / 8.7 | 161 = 81 | 162 | 240.3 | 21.8 | 7.0 | 47 (4.9×) | `e7efffa62ded744d` |
| KBBKR | 231 | 79.2 | 6.9 / 0.6 / 71.7 | 61 = 31 | 61 | 58.1 | 5.9 | 6.9 | 9 (27.0×) | `f1ff23cccfcfdc2f` |
| KBBNK | 231 | 86.6 | 35.3 / 49.2 / 2.1 | 65 = 33 | 66 | 148.9 | 14.1 | 6.3 | 32 (7.2×) | `e5fedab06c47bf9f` |
| KBNKB | 231 | 170.9 | 20.9 / 1.1 / 148.9 | 77 = 39 | 78 | 157.7 | 14.2 | 12.5 | 24 (9.6×) | `de72c4c91280419d` |
| KBNKN | 231 | 175.9 | 26.3 / 3.6 / 146.0 | 213 = 107 | 213 | 183.0 | 16.0 | 11.9 | 35 (6.7×) | `664ecda0ec6e3aed` |
| KBNKQ | 231 | 150.0 | 86.9 / 56.6 / 6.4 | 105 = 53 | 106 | 558.7 | 47.6 | 12.2 | 109 (2.1×) | `b52d6cdf4e9bee6f` |
| KBNKR | 231 | 163.4 | 24.3 / 1.4 / 137.6 | 81 = 41 | 81 | 221.0 | 16.0 | 11.9 | 28 (8.4×) | `7426ac0fcf5e5565` |
| KBNNK | 231 | 88.8 | 37.6 / 46.9 / 4.3 | 67 = 34 | 68 | 150.9 | 13.2 | 6.1 | 39 (6.0×) | `c3c3a9ec4b714bff` |
| KNNKB | 231 | 87.7 | 0.0 / 0.0 / 87.6 | 7 = 4 | 7 | 0.0 | 3.1 | 6.3 | 1 (252.4×) | `dfd8153ec0c88027` |
| KNNKN | 231 | 90.1 | 0.0 / 0.0 / 90.1 | 13 = 7 | 13 | 0.2 | 3.1 | 6.0 | 1 (248.7×) | `674398e2ff0a9a3a` |
| KNNKQ | 231 | 77.2 | 32.0 / 24.7 / 20.5 | 143 = 72 | 144 | 258.2 | 20.6 | 6.9 | 48 (4.8×) | `c3cfe34d51c2073b` |
| KNNKR | 231 | 83.9 | 2.5 / 0.2 / 81.2 | 81 = 41 | 81 | 26.2 | 4.5 | 6.6 | 4 (64.3×) | `654b31c1394f625f` |
| KNNNK | 231 | 30.3 | 13.0 / 12.8 / 4.4 | 41 = 21 | 42 | 48.9 | 4.6 | 2.5 | 15 (15.1×) | `38031b8ca47ce417` |
| KQBBK | 231 | 76.4 | 25.2 / 48.7 / 2.6 | 11 = 6 | 38 | 118.0 | 14.3 | 6.3 | 26 (9.0×) | `6a0ce04fa153f207` |
| KQBKB | 231 | 147.9 | 58.7 / 71.4 / 17.8 | 33 = 17 | 34 | 323.9 | 34.1 | 12.8 | 57 (4.1×) | `681d91e6196bdc7b` |
| KQBKN | 231 | 152.8 | 58.5 / 78.3 / 16.1 | 41 = 21 | 42 | 337.3 | 33.1 | 12.5 | 58 (3.9×) | `2ec6a87537b772d2` |
| KQBKQ | 231 | 127.0 | 53.5 / 5.1 / 68.3 | 65 = 33 | 66 | 311.8 | 29.4 | 13.0 | 40 (5.7×) | `196cfbc1e93c3f5d` |
| KQBKR | 231 | 140.4 | 59.0 / 58.6 / 22.8 | 79 = 40 | 80 | 401.9 | 37.0 | 12.7 | 68 (3.4×) | `29b6ed1ffa6d270a` |
| KQBNK | 231 | 156.7 | 54.3 / 101.3 / 1.1 | 13 = 7 | 66 | 257.9 | 27.9 | 11.5 | 38 (6.0×) | `f91f7ea18fa1d5f6` |
| KQNKB | 231 | 151.8 | 62.6 / 70.4 / 18.8 | 33 = 17 | 34 | 341.8 | 33.4 | 12.8 | 61 (3.8×) | `c992d53be8ca7d92` |
| KQNKN | 231 | 156.8 | 62.4 / 77.3 / 17.1 | 41 = 21 | 42 | 352.4 | 33.0 | 12.4 | 61 (3.8×) | `1f0572e8de3f5dd7` |
| KQNKQ | 231 | 130.9 | 54.2 / 3.1 / 73.5 | 81 = 41 | 82 | 324.3 | 28.5 | 13.2 | 40 (5.8×) | `359f224624fbdb4e` |
| KQNKR | 231 | 144.3 | 64.7 / 57.0 / 22.6 | 81 = 41 | 81 | 442.0 | 38.6 | 12.8 | 73 (3.2×) | `16b8f7fe86f1a71a` |
| KQNNK | 231 | 80.1 | 28.9 / 46.6 / 4.6 | 15 = 8 | 18 | 130.9 | 13.4 | 6.2 | 22 (10.3×) | `cf70620539854aeb` |
| KQQBK | 231 | 70.0 | 18.8 / 49.9 / 1.4 | 7 = 4 | 16 | 86.0 | 13.9 | 5.7 | 14 (16.7×) | `dab613fd75f95631` |
| KQQKB | 231 | 66.5 | 21.9 / 44.5 / 0.0 | 29 = 15 | 34 | 113.5 | 18.4 | 6.2 | 26 (9.0×) | `46585e1abb321c51` |
| KQQKN | 231 | 68.9 | 21.9 / 47.0 / 0.0 | 37 = 19 | 42 | 112.7 | 18.0 | 6.2 | 23 (10.0×) | `5fef0fcd1fbd0bca` |
| KQQKQ | 231 | 56.0 | 21.9 / 22.7 / 11.3 | 59 = 30 | 60 | 130.8 | 17.5 | 6.3 | 29 (7.9×) | `c93de3b5b227a414` |
| KQQKR | 231 | 62.7 | 22.0 / 40.6 / 0.1 | 69 = 35 | 70 | 124.2 | 19.9 | 6.2 | 27 (8.7×) | `07d2ff57a51748f3` |
| KQQNK | 231 | 71.5 | 20.3 / 50.0 / 1.2 | 7 = 4 | 18 | 90.2 | 14.0 | 5.9 | 13 (18.1×) | `0cfbbdde55c6c66e` |
| KQQQK | 231 | 21.6 | 4.5 / 16.4 / 0.7 | 5 = 3 | 8 | 21.9 | 5.4 | 2.3 | 5 (42.4×) | `361952f9bce1e718` |
| KQQRK | 231 | 67.9 | 16.7 / 49.7 / 1.6 | 7 = 4 | 12 | 75.1 | 13.8 | 5.6 | 10 (23.0×) | `501617ecfc7c5e65` |
| KQRBK | 231 | 148.5 | 46.1 / 100.7 / 1.7 | 9 = 5 | 32 | 209.8 | 27.1 | 11.3 | 30 (7.7×) | `ed765e57ef684436` |
| KQRKB | 231 | 142.2 | 53.1 / 78.7 / 10.4 | 57 = 29 | 58 | 280.0 | 35.5 | 12.6 | 58 (4.0×) | `7f73fca2a02230ec` |
| KQRKN | 231 | 147.2 | 53.1 / 86.7 / 7.3 | 79 = 40 | 80 | 284.4 | 35.0 | 12.3 | 52 (4.4×) | `044c25f7fd9be4ad` |
| KQRKQ | 231 | 121.3 | 68.2 / 37.2 / 15.9 | 133 = 67 | 134 | 410.7 | 42.9 | 12.5 | 77 (3.0×) | `11f06758a8510b70` |
| KQRKR | 231 | 134.7 | 53.3 / 67.0 / 14.5 | 67 = 34 | 70 | 312.2 | 37.6 | 12.4 | 56 (4.1×) | `db37688bf9ae145f` |
| KQRNK | 231 | 151.5 | 49.0 / 101.0 / 1.5 | 9 = 5 | 32 | 221.2 | 27.1 | 11.3 | 28 (8.3×) | `7838648715ebec28` |
| KQRRK | 231 | 71.5 | 20.2 / 50.2 / 1.0 | 7 = 4 | 14 | 86.2 | 13.7 | 5.9 | 14 (16.2×) | `d678769751e609b9` |
| KRBBK | 231 | 82.0 | 30.8 / 49.0 / 2.2 | 23 = 12 | 38 | 139.8 | 14.0 | 6.3 | 30 (7.7×) | `2d0e62ab37a33e57` |
| KRBKB | 231 | 160.1 | 69.8 / 61.3 / 29.0 | 59 = 30 | 60 | 471.9 | 35.8 | 12.9 | 89 (2.6×) | `4d575853518d17cd` |
| KRBKN | 231 | 165.1 | 70.3 / 71.4 / 23.4 | 79 = 40 | 80 | 478.3 | 35.7 | 12.7 | 90 (2.6×) | `39d00951cee153f0` |
| KRBKQ | 231 | 139.2 | 76.0 / 11.7 / 51.5 | 139 = 70 | 140 | 571.7 | 44.9 | 13.6 | 75 (3.1×) | `80b2094027f2c1be` |
| KRBKR | 231 | 152.6 | 30.0 / 4.2 / 118.5 | 129 = 65 | 129 | 228.3 | 20.0 | 13.1 | 28 (8.4×) | `c67d83f12317d718` |
| KRBNK | 231 | 167.9 | 65.4 / 102.0 / 0.5 | 57 = 29 | 66 | 304.4 | 27.7 | 11.7 | 45 (5.1×) | `b1f78214b129a2f0` |
| KRNKB | 231 | 164.0 | 73.2 / 60.2 / 30.5 | 61 = 31 | 62 | 512.8 | 36.4 | 12.9 | 91 (2.5×) | `36b6615f73ba2447` |
| KRNKN | 231 | 168.9 | 74.2 / 70.9 / 23.9 | 73 = 37 | 80 | 502.9 | 35.4 | 12.3 | 92 (2.5×) | `bcdf9962966d3019` |
| KRNKQ | 231 | 143.1 | 79.8 / 19.0 / 44.2 | 137 = 69 | 138 | 623.4 | 46.0 | 13.6 | 86 (2.7×) | `2dfcd5142e44c262` |
| KRNKR | 231 | 156.5 | 30.0 / 2.7 / 123.7 | 81 = 41 | 81 | 229.3 | 19.5 | 13.1 | 28 (8.4×) | `72e07be541898ed6` |
| KRNNK | 231 | 85.6 | 34.4 / 46.9 / 4.4 | 29 = 15 | 32 | 149.9 | 13.5 | 6.2 | 27 (8.6×) | `7b284d7a765ad7b5` |
| KRRBK | 231 | 79.0 | 27.8 / 50.8 / 0.4 | 19 = 10 | 32 | 129.6 | 14.4 | 6.4 | 16 (14.8×) | `6ab053ad6e8b66c3` |
| KRRKB | 231 | 76.3 | 31.6 / 34.6 / 10.2 | 57 = 29 | 58 | 201.9 | 18.9 | 6.2 | 40 (5.7×) | `cc40c5df13e820a2` |
| KRRKN | 231 | 78.8 | 31.7 / 40.0 / 7.1 | 79 = 40 | 80 | 195.2 | 18.3 | 6.1 | 37 (6.2×) | `f5670bd98e3a1678` |
| KRRKQ | 231 | 65.9 | 36.2 / 5.4 / 24.3 | 97 = 49 | 98 | 244.1 | 20.7 | 6.4 | 33 (6.9×) | `7e1c69eb8e6ebe80` |
| KRRKR | 231 | 72.6 | 31.7 / 27.0 / 13.8 | 61 = 31 | 62 | 236.3 | 19.6 | 6.5 | 32 (7.3×) | `59544ee4a2dd269d` |
| KRRNK | 231 | 80.5 | 29.3 / 50.9 / 0.3 | 19 = 10 | 32 | 134.9 | 14.2 | 5.6 | 15 (15.9×) | `f2d29f1e867e6927` |
| KRRRK | 231 | 25.2 | 8.1 / 16.9 / 0.2 | 9 = 5 | 14 | 34.6 | 4.9 | 2.3 | 5 (42.5×) | `51e915b368c5a1b5` |
| KBBKP | 677 | 265.1 | 72.5 / 53.5 / 139.0 | 165 = 83 | 165 | 339.4 | 32.3 | 19.2 | 84 (8.1×) | `2e020d9c3dcd2da9` |
| KBBPK | 677 | 265.9 | 110.2 / 143.2 / 12.4 | 59 = 30 | 62 | 399.3 | 41.0 | 18.1 | 99 (6.9×) | `75bc3c5b438ca7f1` |
| KBNKP | 677 | 545.0 | 268.7 / 194.2 / 82.1 | 207 = 104 | 208 | 1317.9 | 97.4 | 36.6 | 326 (2.1×) | `1f81e70c0a562b46` |
| KBNPK | 677 | 546.5 | 239.0 / 304.9 / 2.6 | 65 = 33 | 66 | 866.9 | 88.5 | 34.6 | 213 (3.2×) | `ebd92a83040f5b03` |
| KBPKB | 677 | 527.5 | 107.3 / 35.0 / 385.1 | 101 = 51 | 101 | 604.7 | 70.5 | 38.1 | 130 (5.2×) | `f346d62a2caf0698` |
| KBPKN | 677 | 542.1 | 147.2 / 76.1 / 318.9 | 199 = 100 | 199 | 875.6 | 73.6 | 35.9 | 192 (3.5×) | `28811ff108a4addf` |
| KBPKQ | 677 | 465.1 | 254.1 / 175.3 / 35.6 | 99 = 50 | 100 | 1345.1 | 125.6 | 40.1 | 254 (2.7×) | `0d5fa8594a808917` |
| KBPKR | 677 | 504.9 | 137.7 / 13.2 / 354.0 | 89 = 45 | 89 | 1041.7 | 75.9 | 39.7 | 127 (5.3×) | `15169c4c1ccbb36e` |
| KNNKP | 677 | 279.0 | 59.7 / 23.2 / 196.0 | 229 = 115 | 229 | 373.1 | 32.1 | 18.4 | 114 (5.9×) | `60911ba98b96fc7c` |
| KNNPK | 677 | 279.7 | 124.0 / 135.3 / 20.5 | 55 = 28 | 56 | 418.1 | 38.1 | 17.6 | 110 (6.2×) | `71012ccc3138cb05` |
| KNPKB | 677 | 542.1 | 106.6 / 31.9 / 403.6 | 85 = 43 | 85 | 637.6 | 64.2 | 40.4 | 130 (5.2×) | `c71e41cb6d5e8010` |
| KNPKN | 677 | 556.7 | 135.0 / 64.3 / 357.4 | 193 = 97 | 194 | 784.2 | 67.0 | 34.7 | 172 (3.9×) | `ea24076a0ad0fb74` |
| KNPKQ | 677 | 479.7 | 248.6 / 193.8 / 37.3 | 109 = 55 | 124 | 1383.9 | 129.7 | 39.9 | 272 (2.5×) | `b1b0a25ceea6df8f` |
| KNPKR | 677 | 519.5 | 145.0 / 16.3 / 358.2 | 133 = 67 | 133 | 1014.8 | 77.0 | 37.7 | 138 (4.9×) | `13cdabcbbbf4aa95` |
| KQBKP | 677 | 476.4 | 180.4 / 253.8 / 42.2 | 63 = 32 | 66 | 879.7 | 97.3 | 37.0 | 137 (4.9×) | `631bec864c8f4f30` |
| KQBPK | 677 | 479.6 | 172.1 / 303.7 / 3.8 | 17 = 9 | 62 | 797.4 | 81.3 | 34.6 | 107 (6.3×) | `1e077f9657bf7a12` |
| KQNKP | 677 | 487.9 | 194.6 / 248.4 / 44.9 | 59 = 30 | 82 | 901.6 | 93.1 | 37.1 | 153 (4.4×) | `13173702ff6c02ab` |
| KQNPK | 677 | 491.2 | 183.7 / 304.3 / 3.2 | 17 = 9 | 54 | 816.3 | 80.2 | 34.7 | 107 (6.3×) | `10bf49094145be1b` |
| KQPKB | 677 | 466.9 | 199.1 / 222.8 / 45.1 | 55 = 28 | 58 | 1065.6 | 107.3 | 38.2 | 210 (3.2×) | `acabeece0b69bd9f` |
| KQPKN | 677 | 481.6 | 198.6 / 247.0 / 35.9 | 59 = 30 | 60 | 1055.7 | 106.3 | 37.2 | 211 (3.2×) | `a810d786fb8610b0` |
| KQPKQ | 677 | 404.5 | 208.5 / 28.7 / 167.3 | 247 = 124 | 247 | 1349.3 | 115.7 | 39.7 | 187 (3.6×) | `1254ace34d1ad801` |
| KQPKR | 677 | 444.3 | 246.8 / 182.0 / 15.6 | 73 = 37 | 86 | 1467.9 | 134.8 | 38.6 | 255 (2.7×) | `cfd15f693d4b8b7b` |
| KQQKP | 677 | 215.7 | 66.3 / 148.5 / 1.0 | 43 = 22 | 60 | 301.3 | 51.5 | 18.5 | 50 (13.6×) | `03622f01a50df0ec` |
| KQQPK | 677 | 218.1 | 64.3 / 150.6 / 3.2 | 7 = 4 | 20 | 282.0 | 40.0 | 17.6 | 35 (19.2×) | `217d0b3d41cb3eb9` |
| KQRKP | 677 | 459.2 | 161.2 / 293.9 / 4.1 | 85 = 43 | 134 | 757.1 | 106.2 | 35.9 | 117 (5.8×) | `98aa719c53818d6a` |
| KQRPK | 677 | 463.3 | 155.8 / 304.2 / 3.3 | 13 = 7 | 32 | 691.6 | 78.2 | 34.2 | 81 (8.4×) | `9041b5e5d808351a` |
| KRBKP | 677 | 512.8 | 219.2 / 240.6 / 53.0 | 139 = 70 | 139 | 966.2 | 94.1 | 36.9 | 186 (3.6×) | `f934270018d42bbb` |
| KRBPK | 677 | 515.3 | 207.8 / 305.6 / 1.9 | 31 = 16 | 62 | 861.7 | 81.3 | 34.9 | 134 (5.1×) | `436749a08f8f9db6` |
| KRNKP | 677 | 524.1 | 235.2 / 234.3 / 54.6 | 135 = 68 | 135 | 1032.9 | 93.7 | 36.2 | 218 (3.1×) | `70b5f97e425f7909` |
| KRNPK | 677 | 526.6 | 219.1 / 305.8 / 1.7 | 33 = 17 | 54 | 891.4 | 81.4 | 34.3 | 141 (4.8×) | `e157558ee56a6527` |
| KRPKB | 677 | 506.0 | 229.7 / 180.4 / 95.8 | 145 = 73 | 146 | 1498.1 | 118.1 | 38.9 | 311 (2.2×) | `7e94161b79efe51a` |
| KRPKN | 677 | 520.6 | 232.3 / 212.5 / 75.7 | 107 = 54 | 108 | 1452.6 | 112.7 | 36.9 | 305 (2.2×) | `048da519637ab404` |
| KRPKQ | 677 | 443.5 | 276.7 / 124.0 / 42.8 | 207 = 104 | 207 | 1895.2 | 148.7 | 39.8 | 310 (2.2×) | `de6853cf7896b836` |
| KRPKR | 677 | 483.4 | 207.8 / 63.6 / 212.0 | 147 = 74 | 148 | 1386.6 | 103.9 | 37.6 | 183 (3.7×) | `bf9c4faea5044fe3` |
| KRRKP | 677 | 245.2 | 97.3 / 139.4 / 8.5 | 99 = 50 | 99 | 468.9 | 53.7 | 19.2 | 78 (8.7×) | `be17e09f93cee4c3` |
| KRRPK | 677 | 246.9 | 93.2 / 153.2 / 0.5 | 27 = 14 | 32 | 420.5 | 40.9 | 18.3 | 47 (14.3×) | `1a22fef3d7ebe443` |
| KBPKP | 507 | 417.4 | 205.1 / 140.7 / 71.6 | 133 = 67 | 134 | 759.1 | 70.9 | 27.2 | 199 (2.6×) | `07a9bdc7c59e05aa` |
| KBPPK | 507 | 209.0 | 94.1 / 113.3 / 1.7 | 49 = 25 | 64 | 312.8 | 32.1 | 13.4 | 76 (6.7×) | `e21fd507d5d11286` |
| KNPKP | 507 | 428.0 | 208.8 / 130.2 / 89.1 | 115 = 58 | 115 | 828.5 | 69.8 | 26.0 | 223 (2.3×) | `a0748c8cd02c257a` |
| KNPPK | 507 | 214.4 | 99.6 / 113.6 / 1.2 | 63 = 32 | 64 | 319.3 | 30.7 | 12.8 | 77 (6.6×) | `cd0fa706d6f44238` |
| KPPKB | 507 | 208.4 | 58.9 / 24.6 / 124.8 | 85 = 43 | 86 | 305.0 | 27.7 | 14.2 | 70 (7.2×) | `1d9adcc39dfb48e9` |
| KPPKN | 507 | 213.8 | 70.1 / 39.6 / 104.0 | 99 = 50 | 100 | 381.8 | 31.4 | 12.4 | 88 (5.8×) | `1630b2545112e003` |
| KPPKQ | 507 | 185.2 | 93.0 / 77.4 / 14.9 | 247 = 124 | 247 | 399.3 | 42.4 | 14.2 | 85 (6.0×) | `c974efeaa1359647` |
| KPPKR | 507 | 200.0 | 107.3 / 54.3 / 38.4 | 107 = 54 | 107 | 512.3 | 41.1 | 14.8 | 95 (5.4×) | `b41f4e743c946863` |
| KQPKP | 507 | 372.3 | 156.6 / 199.5 / 16.3 | 209 = 105 | 244 | 639.5 | 81.5 | 28.2 | 129 (3.9×) | `d542a49825cf57d2` |
| KQPPK | 507 | 187.2 | 72.5 / 114.0 / 0.8 | 17 = 9 | 64 | 289.9 | 32.0 | 13.9 | 39 (13.0×) | `a9bf9c3122e2b7e7` |
| KRPKP | 507 | 401.2 | 199.3 / 186.8 / 15.1 | 205 = 103 | 205 | 790.0 | 82.5 | 27.7 | 176 (2.9×) | `634dde86a6ea07bb` |
| KRPPK | 507 | 201.3 | 86.6 / 114.5 / 0.3 | 29 = 15 | 64 | 338.3 | 32.9 | 13.6 | 56 (9.1×) | `558f512241c5cfcb` |
| KPPKP | 380 | 163.3 | 84.6 / 54.6 / 24.1 | 253 = 127 **Überlauf, ≥ 7 fehlen** | 253 | 287.1 | 29.4 | 10.1 | 87 (4.4×) | `d2f3cdafa0bb5ca5` |
| KPPPK | 380 | 54.5 | 26.0 / 28.2 / 0.2 | 65 = 33 | 66 | 77.0 | 8.9 | 3.9 | 18 (20.9×) | `5e63e63e36900d6f` |

Was die Tabelle zeigt:

- **Zeit skaliert mit den Bewertungen**, nicht mit der Größe: KNNKB (fast alles Remis, 41 Tsd.
  Bewertungen) braucht 3 s für 231 MiB, KRPKQ (1,9 Mrd. Bewertungen) 149 s für 677 MiB. Die
  Rate liegt überall bei 12 bis 13 Mio. Bewertungen pro Sekunde auf 12 Threads.
- **Prozess-RAM:** Die Rust-Zeile loggt ihn nicht; Go bei KPPKP 5,7 GB, weil die acht
  Abhängigkeiten (vier Umwandlungen je Seite, je 507 MiB) mitgeladen sind. Ohne Bauern bleibt
  es bei Tabelle plus Bitsets plus Basis (KBNKQ 845 MB).
- **Kompression** folgt der Remisquote: fast reine Remis-Tabellen (KNNKB, KNNKN) auf unter
  1 MiB (250×), dichte DTM-Tabellen mit viel Material beider Seiten (KBNKQ, KBNKP, KRPKx)
  nur 2,1 bis 2,2×. Bauernlose Tabellen packen im Schnitt besser (6,0×) als die mit Bauern
  (4,3×): Dort entscheiden Umwandlungen fast alles, die Remis-Flächen sind kleiner.
- **Ebenen > längstes Matt** (etwa KQBNK 66 Ebenen bei Matt in 13, KQRKP 134 bei 85)
  sind Verluste, deren Fortsetzung in einer kleineren Tabelle liegt: In KQBNK muss Schwarz
  manchmal die Dame schlagen und verliert dann das KBNK-Endspiel in 65 Halbzügen (Verlust in
  66); in KQRKP wandelt der Bauer um und verliert KQRKQ in 133 (Verlust in 134). Das
  "längste Matt" zählt nur Gewinne innerhalb der Tabelle.
- **Zwei Bauern derselben Seite** (KBPPK) belegen so viele Indizes wie je ein Bauer (KBPKP),
  507 MiB, obwohl die Hälfte tot ist (gleiche Steine, nur eine Reihenfolge gültig); die
  Dreiecks-Indizierung aus "Offen" würde sie sparen.

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

### Format v3: kein Ungültig-Wert mehr, Verluste bis 254 (2026-10-08)

Der Autor hatte den Fünf-Steiner-Cache ohnehin gelöscht, damit war die Formatänderung
frei (keine Versionierung, keine Rohdatei-Kompatibilität mehr nötig). Das Byte kodiert
jetzt 0 Remis, 1..127 Gewinn in 2v − 1 Halbzügen, 128..255 Verlust in 2(v − 128)
Halbzügen; der Wert 128 für "ungültig" ist weg, dafür reicht der Verlust bis 254 und
KPPKP ist vollständig. Tote Indizes (doppelt belegt, gleiche Steine in anderer
Reihenfolge, Diagonal-Zwilling) und illegale Stellungen (Gegner im Schach) führt der
Generator in einem Bitset: 1 Bit pro Index, also 1/8 der Tabelle (30 MB bei 231 MB,
1,9 GB beim Sechs-Steiner), nur während der Erzeugung und bis zum Schreiben. Ihr Wert
bleibt 0, vor dem Packen werden sie wie bisher mit dem Vorgänger gefüllt, `Verify`
erkennt sie selbst (Index-Roundtrip, `OpponentInCheck`) und überspringt sie, und die
Suche fragt sie nie ab. Datei und geladene Tabelle werden nicht größer.

Alle Prüfsummen sind damit neu. Basis: `FileChecksum` 0x64fe872c7e217f6e, 35
Tabellenwerte im Code, KBK und KNK sind jetzt identisch (beide komplett 0, vorher
unterschieden sie sich nur in den Ungültig-Markierungen). Go und Rust erzeugen die
Basisdatei byteidentisch (39.413.202 Byte), KBBBK ebenfalls (`a552a70f5be1c3db`,
Verifikation 0 Abweichungen in beiden). Die Fünf-Steiner werden nicht in einem Rutsch
neu gerechnet: Die Konstanten kommen nach und nach aus `egtb-measure`-Läufen, die
Messtabelle oben bleibt als Anforderungstabelle gültig (Zeiten, Größen und Zählungen
ändern sich durch das Format nicht, nur die Prüfsummen). `measure-egtb.bat` und
`egtb-compress` sind entfallen.

## Sechs Steine: erster Messpunkt KRRKBN (2026-10-09, zuhause, UCI-Modus)

KRRKBN (15.502.147.584 Indizes, 14.784 MiB Tabelle) wurde im UCI-Modus per `go infinite`
mit `EgtbGenerate6` und `EgtbWriteCache` erzeugt (Rust, 14 Worker), die
Fünf-Steiner-Abhängigkeiten (KRRKB, KRRKN, KRKBN) davor ebenso:

| | KRRKBN |
|---|---|
| legal | 4.332.968.185 (1.815.118.397 Gewinne, 1.353.380.906 Verluste, 1.164.468.882 Remis) |
| längstes Matt | 87 Halbzüge = 44 Züge |
| Erzeugung | 1.293,7 s = 21,6 min |
| Datei | 2.427 MiB (Faktor 6,1), Laden 5,95 s |
| Prüfsumme | `11049fde56c4b77c` (Konstante im Code) |

Der Autor hat die **zugesicherte Größe** des Prozesses im Task-Manager beobachtet (der
Arbeitssatz lag am Ende der Rechnung rund 3 GB darunter):

| Phase | zugesichert |
|---|---|
| Rechnen, Spitze (am Ende, bevor die `pending`-Listen frei werden) | 36.846.184 kB = 35,1 GiB |
| Schreiben, kurze Spitze | 39.043.460 kB = 37,2 GiB |

Die Rechnung "Tabelle 15,5 GB + vier Bitsets 7,8 GB + Abhängigkeiten und Basis zweimal
2 GB ≈ 25 GB" trifft die Spitze beim Rechnen nicht; rund 10 GB fehlen. Das sind die
**`pending`-Listen**: Stellungen, deren Entscheidung nur aus einer kleineren Tabelle
kommt (hier: ein Turm schlägt Läufer oder Springer und das KRRKB/KRRKN-Kind ist
verloren), werden bei der Initialisierung mit 8 Byte je Index in der Liste ihrer
Zielebene vorgemerkt. Bei KRRKBN sind das offenbar rund eine Milliarde Stellungen, dazu
der Verdopplungs-Spielraum der wachsenden Vektoren. Bei Fünf-Steinern fiel das nie auf
(Listen im zweistelligen MB-Bereich). Beim Schreiben kommt nur wenig obendrauf, weil
Bitsets und Listen bis dahin frei sind; die dortige Kopie der Rohwerte (`writeFile`
verkettet erst alle Tabellen, dann packt es) kostet trotzdem einmal die Tabellengröße.

Folgerungen: (1) ✅ Einzeltabellen werden ohne Rohkopie direkt aus ihren Werten gepackt,
Header und Blöcke gehen einzeln in die Datei (`lz.PackTo`), die Spitze beim Schreiben
fällt damit auf Tabelle plus Komprimat; (2) ✅ Spitzenspeicher wird am Ende jeder
Erzeugung gemeldet (`egtb.PeakMemory`: Windows `K32GetProcessMemoryInfo` ohne Crate,
zugesicherte Größe und Arbeitssatz; Linux `/proc/self/status` VmPeak/VmHWM), als
`info string` im UCI-Modus und als `peak_commit_mb`/`peak_ws_mb` in `measure.log`;
Speicherfragen zählen nur für Rust, Go ist die Testbasis; (3) ✅ **kompakte
`pending`-Listen** (`pendingList`/`PendingList`): 4-Byte-Einträge als Offset in
4-GB-Segmenten (bei 15,5 Mrd. Indizes vier Segmente je Ebene), abgelegt in festen Blöcken
zu 65.536 Einträgen statt in sich verdoppelnden Vektoren. Aus rund 10 bis 12 GB werden
etwa 4 GB bei KRRKBN, ohne zusätzliche Rechenzeit; Prüfsummen von Basis und KBNKQ
unverändert, Go und Rust byteidentisch. Die weitergehende Idee, die Listen durch
**vorläufige Werte** in der Tabelle plus Bitset zu ersetzen, bleibt notiert: Sie bräuchte
pro Ebene eine Aktivierung der vorläufigen Stellungen dieser Ebene, und dafür müsste man
entweder doch Listen halten oder pro Ebene die ganze Tabelle lesen (15,5 GB mal 80 bis
250 Ebenen, einige Minuten extra), daher erst, wenn die 4 GB noch stören.

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
langsamer als Rust, Ursache noch offen (Allokation, Blockkopie).

**Alle Fünf-Steiner (2026-10-07, nach dem Messlauf mit `egtb-compress`):**
47.470.360.896 Byte roh (Basis + 110 Tabellen) → 10.155.980.183 Byte, **Faktor 4,67**,
rund zehn Minuten für 47 GB auf 12 Threads (Zeitstempel der Dateien, ≈ 80 MB/s, also deutlich
schneller als die Erzeugung mit 83 min). Bauernlos 13,5 GiB → 2,25 GiB (6,0×), mit Bauern
30,5 GiB → 7,17 GiB (4,3×); Spanne 2,1× (KBNKQ, KBNKP) bis 252× (KNNKB). Die Erwartung
"Faktor 2 bis 4" war zu vorsichtig, weil die vielen remis- oder gewinnlastigen Tabellen
stärker packen als die beiden Stichproben. Je Material in der Anforderungstabelle oben.

## Offen

- **KPPKP ist unvollständig** (mindestens 7 Verluste in 254 Halbzügen lesen sich als
  Remis). Entscheidung offen: Wert 128 (ungültig) aufgeben und Verluste bis 254 zulassen
  (alle Prüfsummen neu, Neuberechnung aller Tabellen) oder bis zu den zwei Byte pro
  Stellung warten, die Sechs-Steiner ohnehin brauchen.
- Erster Sechs-Steiner zuhause (15,5 GB Tabelle, 127 GB RAM).
- Optimierungsfragen: Skalierung über die Worker (14 zuhause), Speicherbandbreite
  in der Kandidatenphase, zwei Byte pro Stellung für lange Materialien.
- Syzygy-Leser für alles, was nicht selbst gerechnet wird (Entscheidung M5).
- Die Suche verlängert ihre PV noch nicht aus der Tabelle heraus (die PV endet am
  Tabellen-Blatt); `egtbProbe` zeigt, wie billig das ist: pro Halbzug einmal Züge erzeugen
  und das Kind mit Distanz n − 1 wählen.
- Gleiche Steine (KQQK, KBBK, KPPK) belegen die Hälfte ihrer Indizes als tote Einträge
  (Dreiecks-Indizierung würde sie sparen, 24 MB von 173).
