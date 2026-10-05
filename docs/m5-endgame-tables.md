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

## Offen

- Fünf Steine: nur optionale Syzygy-Dateien (Entscheidung), kein eigener Generator.
- Die Tabellen liefern Distanzen, aber keine Züge: Die PV endet am Tabellen-Blatt. Wer die
  Mattführung sehen will, muss aus der Tabelle heraus den Zug mit Distanz n − 1 wählen (1 Zug
  Generierung pro Halbzug, billig). Noch nicht gebaut.
- Gleiche Steine (KQQK, KBBK, KPPK) belegen die Hälfte ihrer Indizes als tote Einträge
  (Dreiecks-Indizierung würde sie sparen, 24 MB von 173).
