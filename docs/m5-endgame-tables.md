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

Wertebereich: Ein Byte fasst Distanzen bis 126 Halbzüge (63 Züge). Materialien
mit längeren Gewinnen (KNNKP hat über 100 Züge) meldet der Generator mit
"distance range exceeded" und lässt die offenen Stellungen als Remis stehen;
dafür bräuchte es zwei Byte pro Stellung.

Arbeitsrechner (i5, 12 Threads), Go:

| Material | roh ohne Symmetrie | Indizes = Datei | legal | Gewinne / Verluste / Remis | längstes Matt | Ebenen | Bewertungen | Zeit | Prozess-RAM | Prüfsumme |
|---|---|---|---|---|---|---|---|---|---|---|
| KBNKQ (= KQKBN) | 1,64 Mrd. | 242.221.056 = 231 MiB | 149.985.528 | 86,9 Mio. / 56,6 Mio. / 6,4 Mio. | 105 Halbzüge = 53 Züge | 106 | 558,7 Mio. | **47,2 s Go / 44,8 s Rust** | 845 MB (355 vorher) | `0x7b54498535f836cb` |

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
