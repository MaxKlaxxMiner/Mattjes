# Milestone 4: Matt- und Remissuche - Design

Stand: 2026-10-04, Entwurf vor der ersten Zeile Code. Dieses Dokument legt
fest, was gebaut wird, in welcher Reihenfolge, und erklärt die Konzepte, die
dahinterstehen. Messwerte kommen später in `m4-mate-search.md`.

## 1. Ziel und Abgrenzung

Mattjes soll zu einer Stellung beweisen, dass der Angreifer in höchstens N
Zügen mattsetzt, und das kürzeste solche Matt samt Hauptvariante ausgeben.
Oder beweisen, dass es bis Tiefe N kein Matt gibt. Spielstärke, Stellungs-
bewertung, Eröffnungen: nichts davon. Das ändert die Suche grundlegend:

- Es gibt nur drei Antworten: "Matt bewiesen", "kein Matt bis Tiefe N",
  "unbekannt". Keine Centipawns, keine Bewertungsfunktion.
- Der Baum ist ein **AND/OR-Baum**: Am Zug des Angreifers (OR-Knoten) reicht
  **ein** Zug, der zum Matt führt. Am Zug des Verteidigers (AND-Knoten) müssen
  **alle** Züge zum Matt führen. Minimax ist der Sonderfall davon mit Zahlen
  statt Wahrheitswerten.
- Der Angreifer hat wenige brauchbare Züge (Schachs, Schlagzüge, Zugzwang-
  Züge), der Verteidiger hat oft viele. Die Breite der AND-Knoten entscheidet
  über die Kosten, nicht die der OR-Knoten.

Sprachgebrauch: "Matt in N" heißt N Züge des Angreifers, also 2N-1 Halbzüge.
Matt in 3 ist ein Baum der Tiefe 5. Intern rechnet alles in Halbzügen (ply).

Die **Remissuche** (Patt erzwingen, Dauerschach, Zugwiederholung) ist dasselbe
AND/OR-Schema mit anderem Terminalkriterium, aber sie braucht Pfadhistorie
(Wiederholung ist eine Eigenschaft des Weges, nicht der Stellung). Sie kommt
als zweite Phase, nachdem die Mattsuche steht. Abschnitt 8.

## 2. Terminalknoten

Ein Knoten ist fertig, ohne weiter zu suchen, wenn:

| Zustand | Seite am Zug | Ergebnis |
|---|---|---|
| im Schach, keine Züge | Verteidiger | **Matt**, Beweis |
| im Schach, keine Züge | Angreifer | Angreifer ist matt, Widerlegung |
| nicht im Schach, keine Züge | egal | Patt = Remis, Widerlegung |
| Tiefe erschöpft | egal | Widerlegung ("kein Matt in dieser Tiefe") |
| Material reicht nie zum Matt | egal | Widerlegung (K gegen K, K+N, K+B) |

### 2.1 Terminal-Orakel: Endspieltabellen (Entscheidung 2026-10-05)

Die Zeile "Material reicht nie zum Matt" ist der Spezialfall einer
allgemeineren Schnittstelle: Die Suche fragt an jedem Blatt ein **Orakel**
"kennst du diese Stellung?" und bekommt entweder nichts oder ein fertiges
Ergebnis (Remis, Gewinn/Verlust in n Halbzügen). Die Schnittstelle kommt
schon in M4 mit dem Material-Check als einziger Quelle, damit Milestone 5
Tabellen einstecken kann, ohne die Suche anzufassen.

Was eingesteckt wird (Details in `CLAUDE.md`, Milestone 5):

- **Eigene 4-Steiner als Fundament, immer verfügbar.** Alle Materialien bis
  vier Steine per Retrograde-Analyse als **DTM** berechnet, komplett im RAM
  (≈ 250 MB mit Königs-Symmetrie, 1 Byte pro Stellung), beim ersten Start
  gerechnet (Minuten, parallel weniger) und als eine Cache-Datei neben der
  Binary abgelegt (kein Header, Layout fest im Code, Prüfsumme über den Inhalt
  als bekannte Konstante im Code), danach praktisch sofort geladen. DTM statt
  WDL, weil ein Blatt in
  der Tabelle dann sofort den Beweis **mit Distanz** liefert: Die Suche muss
  nur bis in die Tabelle hinein beweisen, nicht bis zum Matt. Die
  Teststellungen 1 bis 3 und 6 bis 7 (alle bis vier Steine) werden damit
  Tabellen-Lookups, erst 4, 5, 8 und 9 bleiben echte Suchaufgaben.
- **Ab 5 Steinen nur optionale Syzygy-Dateien**, zuerst WDL (Gewinn, Remis,
  Verlust, mit 50-Züge-Regel), später DTZ. Fehlen sie, läuft die Suche ohne.
  WDL kann kein Matt beweisen, nur "gewonnen", und ist deshalb für die
  asymmetrischen Rollen (3.4) und die Vorab-Suche wertvoll, für den Mattbeweis
  nur als Richtungsgeber.
- Rochade ist mit vier Steinen ausgeschlossen; Stellungen mit En-passant-Feld
  stehen nicht in der Tabelle, die Suche spielt dort einen Halbzug weiter und
  fragt dann (wie Syzygy).
- Verifikation nur gegen Bekanntes oder Eigenes: bekannte Maximaldistanzen
  (KQK 10, KRK 16, KBBK 19, KBNK 33, KQKR 35 Züge), die eigene Mattsuche als
  Orakel für kleine Distanzen, der Rückwärtszug-Generator gegen den
  Vorwärts-Generator. Nichts aus dem alten C#-Code, keine externen Dienste.

Was **nicht** terminal ist, bewusst:

- **Zugwiederholung.** Bei der Suche nach dem kürzesten Matt kann sie gar nicht
  im Beweisbaum vorkommen: Jede Stellung im Beweis hat eine Mattdistanz, die
  pro Halbzug um eins fällt. Eine Wiederholung hätte zweimal dieselbe Stellung
  mit zwei verschiedenen Distanzen. Der Angreifer wiederholt also nie, und
  der Verteidiger kann durch Wiederholung nichts gewinnen, weil er ja gar nicht
  bis zur Wiederholung kommt, wenn der Angreifer einen Beweis hat. Nur für die
  Remissuche (Dauerschach!) wird Wiederholung zum Thema.
- **50-Züge-Regel.** Matt in 39 (Dame gegen Läufer und Springer) sind 77
  Halbzüge, oft ohne Schlagen oder Bauernzug. Nach Turnierregeln kann der
  Verteidiger das Remis reklamieren. Tablebases unterscheiden deshalb DTM
  (distance to mate) und DTZ (distance to zeroing). Mattjes sucht in Phase 1
  DTM, wie der alte C#-Code auch, und ignoriert den Zähler. Der Zähler gehört
  auch nicht in den Hash-Key (sonst wäre jede Stellung 100-mal in der
  Tabelle). Eine spätere Option "DTZ-konform" kann den Zähler als Tiefen-
  schranke mitführen, ohne den Key zu ändern.

## 3. Algorithmen

Drei Ansätze, jeder als eigenes Package, damit er komplett fliegen kann. Die
Namen `mateab`, `matepn`, `matelist` sind Arbeitstitel dieses Entwurfs, keine
Entscheidung des Autors. Der eigene Ansatz des Autors steht in Abschnitt 3.4
und ist noch nicht in den Umsetzungsplan eingeordnet.

### 3.1 `mateab`: Alpha/Beta mit Mattfenster (Referenzorakel)

Der klassische Weg, bewusst als erstes gebaut, weil er einfach, exakt und
leicht zu prüfen ist. Er dient später als **Orakel** für die Korrektheit der
anspruchsvolleren Verfahren: Für jede Stellung, in der df-pn ein Matt in k
meldet, muss Alpha/Beta mit Tiefe 2k-1 dasselbe finden und mit Tiefe 2k-3
nichts.

Funktionsweise:

- Rekursive Suche mit Resttiefe in Halbzügen. Bewertung nur aus der Menge
  {Matt in k Halbzügen, kein Matt}. Das "Fenster" ist ein Mattfenster: Alpha
  und Beta sind Mattdistanzen, kein Bewertungsintervall. Praktisch heißt das:
  Der Angreifer braucht an einem OR-Knoten nur **einen** Zug, der Matt in der
  Resttiefe liefert, dann kehrt er zurück (Cutoff). Der Verteidiger braucht
  **einen** Zug, der dem Matt entkommt, dann kehrt er zurück.
- **Iterative Vertiefung** über N = 1, 2, 3 ... Das erste gefundene Matt ist
  automatisch das kürzeste, und der Aufwand der kleineren Tiefen ist gegen die
  letzte Tiefe vernachlässigbar (der Baum wächst exponentiell).
- **Letzter Angreiferzug nur Schachs.** Ein Mattzug ist immer ein Schachgebot.
  Im letzten Halbzug des Angreifers (Resttiefe 1) dürfen also ohne Verlust nur
  Schachs erzeugt werden. In tieferen Angreiferzügen **nicht**: Zugzwang-Matts
  (König und Dame gegen König) brauchen stille Züge. Diese eine Beschränkung
  bringt den größten Teil des Gewinns, weil sie die Breite der untersten
  OR-Ebene von etwa 30 auf etwa 3 drückt.
- **Verteidiger im Schach** erzeugt ohnehin nur Ausweichzüge, das macht
  `GenMoves` über `checkers` schon heute.

Erwartung: Matt in 3 bis 7 sofort, Matt in 12 bis 17 im Sekunden- bis
Minutenbereich mit TT, Matt in 31 und 39 nicht erreichbar. Genau dort beginnt
die Rechtfertigung für den zweiten Ansatz.

### 3.2 `matepn`: Proof-Number-Search, Variante df-pn

Proof-Number-Search (Allis, 1994) ist der Spezialist für AND/OR-Bäume. Die
Idee, in einem Satz: **Suche dort weiter, wo der Beweis am billigsten
scheint.**

Jeder Knoten trägt zwei Zahlen:

- **pn** (proof number): Wie viele Blätter müssen mindestens noch bewiesen
  werden, damit dieser Knoten bewiesen ist.
- **dn** (disproof number): Wie viele Blätter müssen mindestens noch widerlegt
  werden, damit dieser Knoten widerlegt ist.

Blätter: unbekannt pn = 1, dn = 1. Matt: pn = 0, dn = unendlich. Kein Matt:
pn = unendlich, dn = 0. Nach innen:

| Knoten | pn | dn |
|---|---|---|
| OR (Angreifer) | min über Kinder | Summe über Kinder |
| AND (Verteidiger) | Summe über Kinder | min über Kinder |

Das ist genau die Logik des AND/OR-Baums in Zahlen: Am OR-Knoten reicht das
billigste Kind, am AND-Knoten müssen alle bewiesen werden. Die Suche steigt
von der Wurzel immer zum Kind mit der kleinsten pn (an OR-Knoten) bzw.
kleinsten dn (an AND-Knoten) ab, bis sie ein Blatt erreicht, expandiert es,
und trägt die neuen Zahlen nach oben. Fertig, wenn die Wurzel pn = 0 (Matt)
oder dn = 0 (kein Matt) hat.

Warum das Verteidigern mit vielen Zügen den Zahn zieht: Ein AND-Knoten mit 40
Antworten hat pn = 40, einer mit 2 Antworten pn = 2. Die Suche geht
automatisch in die Varianten, in denen der Verteidiger wenig Luft hat, also in
die Schachs und Fesselungen, ohne dass jemand "Schachs zuerst" programmiert.

**Initialisierung statt 1/1:** Statt jedes neue Blatt mit pn = dn = 1 zu
starten, nimmt man die **Mobilität**: Am AND-Knoten pn = Anzahl der
Verteidigerzüge. Das kostet einen Generator-Aufruf pro Blatt und zahlt sich
in der Literatur regelmäßig aus. Wird gemessen.

**df-pn** (Nagai, 2002) ist die tiefensuchende Fassung: Statt den ganzen Baum
im Speicher zu halten, hält sie nur den aktuellen Pfad und speichert pn/dn in
einer Transposition Table. Jeder Knoten bekommt **Schwellen** (thresholds)
für pn und dn mit: "Suche hier weiter, solange du unter diesen Schwellen
bleibst". Die Schwellen ergeben sich aus dem zweitbesten Geschwister, genau
wie bei PNS die Auswahl des most-proving node. Das ist der Grund, warum df-pn
und unsere TT zusammenpassen: Die TT **ist** der Baum.

Bekannte Fallstricke, die das Design berücksichtigen muss:

- **Speicher.** Die TT füllt sich mit pn/dn-Paaren. Bei einem Cache werden
  Einträge überschrieben und die Suche rechnet Teilbäume neu. Das ist korrekt
  (df-pn bleibt mit Verlusten korrekt), kostet aber Zeit. Mit 127 GB RAM ist
  das hier weniger ein Problem als die Frage, wie viele Bits pn/dn brauchen
  (Abschnitt 5).
- **GHI, graph history interaction.** Wenn ein TT-Eintrag "widerlegt" sagt,
  weil in einem anderen Pfad eine Zugwiederholung Remis ergab, ist das
  pfadabhängig und im neuen Pfad vielleicht falsch. Für die Mattsuche nach
  Abschnitt 2 tritt das nicht auf (keine Wiederholungserkennung). Für die
  Remissuche wird es relevant.
- **Kein kürzestes Matt.** PNS beweist "es gibt ein Matt", nicht "das
  kürzeste". Deshalb die Zweistufigkeit aus dem Milestone-Text: df-pn findet
  den Beweis und liefert eine Obergrenze k, dann sucht `mateab` mit
  Tiefenfenster das kürzeste, mit dem bewiesenen Baum als Zugsortierung in
  der TT. Alternative: df-pn mit Tiefenschranke, dann ist es wieder ein
  Suchlauf pro N wie bei iterativer Vertiefung.
- **Unendliche Tiefe.** Ohne Tiefenschranke kann df-pn in Varianten laufen,
  die nie enden (Dauerschach). Eine Tiefenschranke als Widerlegung
  ("Resttiefe 0 = kein Matt") hält das Verfahren endlich; sie wird mit der
  Zweistufigkeit sowieso gebraucht.

### 3.3 `matelist`: listenbasierte Suche (Breitensuche mit Beweisspeicher)

Die Idee aus dem Milestone-Text und aus Milestone 1 und 3: statt Rekursion
eine Liste von Stellungen pro Ebene, dedupliziert über `ttstore`, wie in
`bitboardUniquePositionsHashed`. Die Mattsuche wird daraus so:

1. Ebene 0 ist die Wurzel. Ebene i+1 enthält alle Kinder von Ebene i,
   dedupliziert. Angreifer-Ebenen nur mit Angreiferzügen, Verteidiger-Ebenen
   mit allen Antworten.
2. Auf der tiefsten Ebene wird jedes Blatt geprüft (Matt oder nicht).
3. **Rückwärts** über die Ebenen: Ein Verteidigerknoten ist bewiesen, wenn
   alle Kinder bewiesen sind; ein Angreiferknoten, wenn eines bewiesen ist.
   Das Ergebnis steht als Value im `ttstore` der Ebene.

Das ist Retrograde-Analyse auf einem Vorwärtsbaum. Vorteile: keine Rekursion,
triviale Parallelisierung (jede Ebene ist ein Datenstrom, wie
`PerftBreadthEncoded`), Transpositionen werden vollständig genutzt (nicht nur
im Cache), der Beweis liegt komplett vor und ist unabhängig prüfbar, Abbruch
und Fortsetzen über Save/Load. Nachteil: Speicher wächst mit der Breite der
Verteidiger-Ebenen, und ohne Steuerung werden alle Varianten gleich tief
getrieben, auch die aussichtslosen.

Deshalb kein Ersatz für df-pn, sondern eine **Ergänzung**: Die Breitensuche
kann die ersten Ebenen ausbreiten (dort sind die Transpositionen am
wertvollsten), df-pn arbeitet die Blätter ab. Und sie ist der direkte Vorläufer
der Endspiel-Generierung in Milestone 5 (`Tests.MateReverse` im alten Code
hat genau diese Richtung: Mattstellungen aufzählen und rückwärts gehen).

Reihenfolge: `mateab` zuerst, dann `matepn`, `matelist` nur, wenn `matepn`
an Speicher oder Transpositionen scheitert oder wenn Milestone 5 ihn braucht.

### 3.4 Asymmetrische Rollen: eine Seite will gewinnen, die andere Remis (Idee des Autors)

Die drei Ansätze oben geben beiden Seiten dasselbe Ziel mit umgekehrtem
Vorzeichen. Der Autor hat einen anderen Algorithmus im Sinn: Jede Seite
bekommt eine **Rolle** mit eigenem Ziel und eigenen Heuristiken.

- Die **Gewinnseite** sucht Fortschritt (Matt, Materialgewinn, Umwandlung).
- Die **Remisseite** versucht nicht zu gewinnen, sondern das Spiel so schnell
  wie möglich remis zu beenden: früh Figuren abtauschen, Bauern pushen und
  abklären, und wenn kein Fortschritt mehr möglich ist, den König nur noch
  minimal pendeln (a→b, b→a), um Stellungswiederholungen heraufzubeschwören.
  Patt, Wiederholung, totes Material und 50-Züge-Regel sind für sie Erfolge.

Beide Richtungen sind möglich ("Weiß will gewinnen, Schwarz spielt auf
Remis" und umgekehrt). Was sich dadurch gegenüber 3.1 bis 3.3 ändert:

- **Zugsortierung** und **Pruning** sind pro Rolle verschieden. Die Remisseite
  reduziert ihre Breite freiwillig (Abtausch vor allem anderen, Pendelzüge
  statt aller Königszüge), die Gewinnseite behält ihre volle Breite.
- **Terminalkriterien** sind pro Rolle verschieden, und Wiederholung wird zum
  Ziel der Remisseite. Damit braucht die Suche Pfadhistorie von Anfang an
  (Abschnitt 8), nicht erst in Phase 2.
- Die Antwort ist nicht mehr nur "Matt in N", sondern "die Gewinnseite kommt
  gegen einen Gegner, der nur noch vereinfacht, (nicht) durch". Das ist genau
  die **schnelle Vorab-Suche** der ersten Stufe aus dem Milestone-Text: Findet
  die Gewinnseite gegen die Remisseite nichts, ist ein Beweis unwahrscheinlich
  und die exakte Suche kann sich sparen oder gezielt ansetzen.

Offen: ob die Rollen als eigenes Package neben den dreien stehen oder als
Zugsortierungs- und Terminal-Strategie in `mateab` eingehängt werden; und wie
die Heuristiken der Remisseite gemessen werden (Testfall: Stellungen mit
bekanntem Remis durch Dauerschach oder Festung, die Remisseite muss es finden).

## 4. Generator-Erweiterungen in `bitboard`

Die Mattsuche fragt den Generator anders als Perft. Alles, was fehlt, kommt
in `bitboard` dazu (nicht in ein neues Package: es ist derselbe Generator,
dieselben Tabellen), jeweils in Go und Rust.

### 4.1 `HasMoves`, `IsMate`, `IsStalemate`

`GenMoves` erzeugt alle Züge in den Buffer. Für die Frage "ist das Matt?"
reicht der erste legale Zug. `HasMoves` läuft dieselbe Logik (danger,
checkers, pinned) und kehrt beim ersten gefundenen Zug zurück, ohne Buffer.
Reihenfolge nach Trefferwahrscheinlichkeit: Königszüge zuerst (bei
Doppelschach die einzigen), dann Schlagen oder Blocken des Schachgebers.

```
IsMate      = InCheck && !HasMoves
IsStalemate = !InCheck && !HasMoves
```

Messgröße: Anteil der Mattprüfungen, die vor dem ersten Zug abbrechen, gegen
einen vollen `GenMoves`. Bei Blättern, die kein Matt sind (die große
Mehrheit), ist der erste Königszug meist legal, der Gewinn sollte groß sein.

### 4.2 `GenChecks`: nur Schachgebote

Für den letzten Angreiferhalbzug (und als Sortierklasse). Direkte Schachs
über Zielfeld-Masken: Von welchen Feldern aus greift eine Figur den
gegnerischen König an? Das ist die Angriffsmenge vom Königsfeld aus, mit der
jeweiligen Figurenart:

```
rookTargets   = rookAttacks(theirKing, occ)
bishopTargets = bishopAttacks(theirKing, occ)
knightTargets = knightAttacks[theirKing]
pawnTargets   = pawnAttacks[them][theirKing]   // Felder, von denen unser Bauer den König schlägt
```

Ein Turmzug gibt direktes Schach, wenn sein Zielfeld in `rookTargets` liegt.
Dazu kommen vier Sonderfälle, die ein reiner Zielfeld-Filter übersieht:

- **Abzugschach.** Eine eigene Figur steht zwischen eigenem Slider und
  gegnerischem König (das ist `pinned` aus Sicht des Gegners, berechnet mit
  unseren Slidern): Jeder Zug dieser Figur **weg von der Linie** gibt Schach,
  egal wohin. Eine Figur, die auf der Linie bleibt, gibt keins.
- **Umwandlung.** Zielfeld-Test mit der neuen Figurenart, nicht mit dem Bauern.
- **En passant.** Entfernt zwei Bauern von einer Reihe, kann einen Turm oder
  Läufer freilegen. Wie in `enPassantLegal`: expliziter Test.
- **Rochade.** Der Turm landet neben dem König und kann Schach geben (f1/d1
  in `rookTargets`).

Verifikation: Über alle Knoten der Perft-Stellungen bis Tiefe 4 muss gelten
`GenChecks(b) == filter(GenMoves(b), m -> DoMove; InCheck)`, als Menge. Die
Perft-Tabellen auf chessprogramming.org enthalten zusätzlich Spalten
"Checks" und "Checkmates" pro Tiefe; die werden in `chess` als Referenz
eingetragen und von einem Perft-Lauf mit `GivesCheck`/`IsMate` gezählt. Damit
sind `GenChecks`, `GivesCheck` und `IsMate` gegen Fremddaten verifiziert, nicht
nur gegeneinander.

### 4.3 `GivesCheck(m)`: Test für einen einzelnen Zug

Dieselbe Logik als Prädikat statt Generator, für die Zugsortierung in
`mateab` (Schachs nach vorn) und als Verifikationsgegenstück zu `GenChecks`.

### 4.4 Zugsortierung

- **OR-Knoten:** Schachs, dann Schlagzüge, dann Umwandlungen, dann der Rest.
  Innerhalb der Schachs: die mit den wenigsten Verteidigerantworten zuerst
  (das ist die Mobilitätsheuristik aus 3.2, hier als Sortierung; kostet einen
  Generatoraufruf pro Zug, wird gemessen).
- **AND-Knoten:** Der Zug, der die letzte Suche widerlegt hat, zuerst (aus der
  TT: der Verteidiger merkt sich seinen Rettungszug). Dann Schlagzüge
  (Material wegnehmen rettet oft), dann Königszüge ins Freie.
- **TT-Zug** vor allem anderen, auf beiden Knotenarten.

### 4.5 Copy-make

Milestone 1 hat gezeigt: `PerftIterative` (Brett kopieren, 200 Byte) schlägt
`PerftRecursive` (Make/Unmake). Die Suche nimmt deshalb copy-make: Jede
Rekursionsebene hält ihr eigenes `Board` auf dem Stack, `DoMove` auf der
Kopie, kein `UndoMove`. Das passt auch zu den Listen aus 3.3. Go-Falle aus
`CLAUDE.md` beachten: Bretter per Wert oder als Pointer in ein Array fester
Größe, nie an Interface-Methoden (Escape auf den Heap, `alloc` im Runner
verrät es).

## 5. Transposition Tables in der Suche

Milestone 3 liefert zwei Charaktere. Beide werden gebraucht.

### 5.1 `tt` als Suchcache (24 Value-Bits bei 256 MB, 22 bei Buckets)

Die Suche braucht pro Stellung: Ergebnistyp, Tiefe oder Distanz, besten Zug.
Vorschlag für das Value-Layout, von oben nach unten, damit die Priorität (was
darf ersetzt werden) in den hohen Bits liegt, wie in M3 vorgesehen:

| Bits | Inhalt | Bemerkung |
|---|---|---|
| 23..22 | Typ: 0 leer, 1 Matt bewiesen, 2 kein Matt bis Tiefe, 3 reserviert | bewiesene Matts sind am wertvollsten und stehen oben |
| 21..16 | Tiefe in Halbzügen (6 Bit, bis 63) | bei Typ 1: Mattdistanz, bei Typ 2: geprüfte Tiefe |
| 15..0 | Zug: from 6, to 6, promo 2, 2 frei | bester Zug bzw. Rettungszug |

Bei Buckets fehlen 2 Bits: die beiden freien Bits im Zug entfallen. Bei einer
1-GB-Tabelle (26 Bit) wären 2 Bits für Alterung frei. 63 Halbzüge reichen für
Matt in 31, nicht für Matt in 39 (77 Halbzüge): dann 7 Tiefenbits und der
Zug ohne Promo-Bits (Promo aus der Stellung rekonstruierbar, Dame als
Default). Entscheidung bei der Implementierung, der Punkt ist dokumentiert.

**Tiefenunabhängigkeit.** Ein bewiesenes Matt (Typ 1) gilt für jede Suchtiefe
ab der Distanz, ein "kein Matt bis Tiefe d" nur für Resttiefen bis d. Deshalb
**kein** Tiefensalz im Key wie bei `perftTT`; dort war Tiefe Teil der
Identität (Perft-Zahl hängt von der Tiefe ab), hier nicht.

**Für df-pn:** pn und dn statt Typ/Tiefe/Zug. 24 Bit sind knapp: 12 + 12 mit
Sättigung (4095 = unendlich) oder, besser, ein kleines Gleitkommaformat
(z. B. 8 Bit Mantisse, 4 Bit Exponent je Zahl), weil pn/dn über Zehnerpotenzen
wachsen und nur ihre Rangfolge zählt, nicht ihr genauer Wert. Der bewiesene
oder widerlegte Zustand (pn = 0 oder dn = 0) muss exakt sein, das ist beim
Gleitkommaformat gegeben (0 ist darstellbar). Beide Varianten messen.

### 5.2 `ttstore` als Beweisspeicher

Was einmal bewiesen ist, darf nicht verloren gehen. Bewiesene Teilbäume
(Typ 1 mit Distanz und Zug) wandern zusätzlich in einen `ttstore`: Er
verliert nie, kann gespeichert und geladen werden und ist damit das Mittel,
eine abgebrochene Suche fortzusetzen. Bei "voll" meldet er das, die Suche
läuft dann nur mit dem Cache weiter. Gleiches Value-Layout wie 5.1, damit ein
Eintrag ohne Umrechnung wandern kann.

Der Beweisspeicher ermöglicht auch die **unabhängige Verifikation** eines
Beweises (Abschnitt 6): Ein Prüfer, der nichts von der Suche weiß, geht den
Baum ab dem Wurzelzug ab, erzeugt an jedem Verteidigerknoten **alle** Züge mit
`GenMoves` und schlägt für jedes Kind im Store nach. Fehlt eines, ist der
Beweis unvollständig. Am Ende jeder Variante muss `IsMate` wahr sein.

### 5.3 Alterung und Shared-TT

Beide offenen Punkte aus M3 bleiben offen. Eine Mattsuche einer Stellung ist
ein Lauf mit einer Tabelle; iterative Vertiefung will alte Einträge behalten,
nicht altern. Threads kommen, wenn die Einzelthread-Suche steht und gemessen
ist. Der Perft-Parallellauf ist der Testfall für die Shared-TT, wie in M3
notiert.

## 6. Korrektheit vor Geschwindigkeit

Wie bei Perft gilt: Erst stimmen die Zahlen, dann wird gemessen. Drei Ebenen:

1. **Generator-Tests** (4.2): Checks- und Checkmates-Zähler aus den
   Perft-Tabellen, `GenChecks` gegen gefiltertes `GenMoves`, `HasMoves` gegen
   `GenMoves() > 0` über alle Perft-Knoten.
2. **Teststellungen mit bekannter Mattlänge** (Abschnitt 7): gefundene
   Mattlänge muss stimmen, und die Suche mit einer Tiefe weniger muss **kein**
   Matt finden. Beides, sonst ist ein "Matt in 5" vielleicht ein übersehenes
   Matt in 4.
3. **Beweisprüfung** (5.2): jeder gemeldete Beweis wird nachgespielt, in der
   Hauptvariante und, mit Store, im kompletten Baum.

Dazu die Orakel-Beziehung: `matepn` und `mateab` müssen auf allen Stellungen,
die beide schaffen, dieselbe Mattlänge liefern. Und Go gegen Rust wie bisher:
gleiche Knotenzahlen bei gleichem Algorithmus.

## 7. Teststellungen

Aus `old/Program.cs`. Die Mattlängen der acht Endspiele (bis fünf Steine)
wurden am 2026-10-05 gegen die Lichess-Tablebase (Syzygy mit DTM) geprüft:
**alle acht stimmen** mit dem alten C#-Code überein. DTM in Halbzügen,
DTZ (Halbzüge bis zum nächsten Schlag- oder Bauernzug) zur Kontrolle, dass
die 50-Züge-Regel in keiner der Stellungen den Mattweg kreuzt (alle DTZ unter
100). Der Bauern-Test (12 Steine) ist nur durch die eigene Suche in beiden
Sprachen und beiden Algorithmen prüfbar.

| Nr | Material | FEN | Matt in | DTM | DTZ | Erster Zug | Zweck |
|---|---|---|---|---|---|---|---|
| 1 | KQQ-K | `8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1` | 3 | 5 | 3 | Qf7 | Rauchtest |
| 2 | KQR-K | `8/8/8/4k3/8/Q7/R7/K7 w - - 0 1` | 5 | 9 | 6 | Qe3+ | |
| 3 | KRR-K | `8/8/8/4k3/8/R7/R7/K7 w - - 0 1` | 7 | 13 | 8 | Re2+ | Treppenmatt, stille Züge |
| 4 | KQ-KN | `7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1` | 12 | 23 | 21 | Qf6+ | Zugzwang, Pattfallen |
| 5 | KR-KR | `8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1` | 15 | 29 | 1 | Kxf7 | Gegenspiel des Verteidigers |
| 6 | KBB-K | `8/8/4k3/8/8/8/8/K2BB3 w - - 0 1` | 17 | 33 | 33 | Kb2 | lange stille Manöver |
| 7 | KBN-K | `8/8/8/8/3k4/8/N7/KB6 w - - 0 1` | 31 | 61 | 61 | Nc1 | nur mit df-pn oder TT-Transpositionen |
| 8 | KQ-KBN | `8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1` | 39 | 77 | 55 | Kb2 | längster Test, Verteidiger mit zwei Figuren |
| 9 | Bauern | `5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1` | 6 | | | | Umwandlung, En passant (a6 im FEN), schwarze Dame; unbestätigt |

Noch zu ergänzen, bevor die Messungen beginnen:

- **Negativtests:** Stellungen, in denen bis Tiefe N kein Matt existiert, mit
  Pattfallen (KQ-K, Dame nimmt dem König das letzte Feld). Die Suche muss
  "kein Matt" melden, nicht hängen und nicht ein falsches Matt liefern.
- **Mittelspiel-Matts** aus Problemsammlungen (Matt in 2 bis 4 mit vielen
  Steinen), weil die Endspiele oben wenig Verteidigerbreite haben. Quelle und
  Lizenz klären, bevor etwas ins Repo kommt.
- **Rochade-Matts** und **Umwandlungs-Matts** (Unterverwandlung in Springer),
  damit die Sonderfälle aus 4.2 wirklich getroffen werden.

Die Stellungen kommen als Referenztabelle nach `chess` (wie die
Perft-Stellungen), mit Feldern FEN, erwartete Mattlänge in Halbzügen, Quelle
der Bestätigung.

## 8. Remissuche (Phase 2, nach der Mattsuche)

Gleiches AND/OR-Schema, der "Angreifer" ist jetzt die Seite, die Remis
erzwingen will, terminal sind Patt, Dauerschach (Wiederholung unter
ununterbrochenem Schach), dreifache Wiederholung und totes Material.
Neu gegenüber der Mattsuche:

- **Pfadhistorie.** Wiederholung ist eine Eigenschaft des Weges. Der Suchpfad
  hält seine Keys (ein kleines Array pro Tiefe, lineare Suche reicht), und
  Ergebnisse, die aus einer Wiederholung entstanden sind, sind pfadabhängig:
  Sie dürfen nicht ungeprüft in die TT (GHI). Erster Ansatz: solche Ergebnisse
  gar nicht speichern und messen, wie viel Trefferquote das kostet.
- **Dauerschach** ist für df-pn attraktiv: Die Seite mit Dauerschach hat sehr
  wenige Züge (nur Schachs), der Verteidiger ebenfalls wenige (Ausweichzüge).
  Kleine Bäume, viele Transpositionen.

Details, wenn die Mattsuche gemessen ist.

## 9. Umsetzungsplan

Jeder Schritt zuerst in Go, Korrektheit prüfen, dann Rust, dann Gegenprüfung.
Benchmarks auf Ansage (Maschine in der Kopfzeile nennen, `docs/machines.md`).

1. **`bitboard`** ✅ (2026-10-05, Go und Rust): `HasMoves`, `IsMate`,
   `IsStalemate`, `GivesCheck`, `GenChecks`. Referenzzähler in `chess`,
   Perft-Lauf mit Zählung als Regressionstest, Vergleich `GenChecks` gegen
   Filter. Ergebnisse in `m4-mate-search.md`.
2. **`mateab` ohne TT** ✅ (2026-10-05, Go und Rust, knotengenau gleich):
   Mattfenster, iterative Vertiefung, Schachs im letzten Halbzug, Killerzug pro
   Ebene, Terminal-Orakel-Schnittstelle (2.1) mit dem Material-Check als erster
   Quelle. Stellungen 1, 2, 3, 9 stimmen (Länge exakt bei 2N-1, eine Tiefe
   weniger findet nichts). Ergebnisse in `m4-mate-search.md`.
3. **`mateab` mit `tt`:** Value-Layout aus 5.1, TT-Zug, Zugsortierung.
   Stellungen 4, 5, 6. Erste Messung: Knoten und Zeit mit/ohne TT, mit/ohne
   Sortierung, Trefferquote.
4. **`matepn`:** df-pn mit `tt`, pn/dn-Kodierung (zwei Varianten messen),
   Mobilitätsinitialisierung an/aus. Stellungen 7 und 8. Orakelvergleich mit
   `mateab` auf 1 bis 6.
5. **Beweisspeicher und Verifikation:** `ttstore` für bewiesene Teilbäume,
   unabhängiger Prüfer, Save/Load, Fortsetzen einer abgebrochenen Suche.
6. **Rust-Port** von 1 bis 5, Gegenprüfung der Knotenzahlen, Benchmarks beider
   Sprachen in `m4-mate-search.md`.
7. **Remissuche** nach Abschnitt 8.
8. Optional: `matelist`, Mehrthread-Suche mit Shared-TT.

## 10. Offene Entscheidungen

Punkte, die beim Bauen fallen und hier festgehalten werden, sobald sie
entschieden sind:

- Tiefenbits 6 oder 7 im `tt`-Value (Matt in 39 braucht 77 Halbzüge).
- pn/dn als 12+12 mit Sättigung oder als Mini-Gleitkomma.
- Mobilitätsinitialisierung in df-pn: Generatoraufruf pro Blatt lohnt sich?
- Zweistufig (df-pn beweist, `mateab` kürzt) oder df-pn mit Tiefenschranke
  in iterativer Vertiefung.
- Einordnung der asymmetrischen Rollen (3.4) in den Umsetzungsplan: eigenes
  Package oder Strategie innerhalb von `mateab`; Testfälle für die Remisseite.
- Erledigt 2026-10-05: Tablebase-Prüfung der Teststellungen, alle acht
  Endspiele bestätigt (Abschnitt 7).
