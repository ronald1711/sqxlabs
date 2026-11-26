# Strategieën zoeken, toetsen en vergelijken

Selecteer bovenaan een instrument en tijdframe. Controleer onder **Data** de kalender, onderbrekingen en herkomst. Controleer onder **Settings** de rekeningvaluta, spread, slippage, commissie en financiering. De resultaten horen bij deze gekozen aannames.

## Een verkennende zoekopdracht

1. Open **Discover** en stel de zoekruimte in. Kies onder **Exploratory search** het aantal kandidaten en een seed om de zoekopdracht later te kunnen herhalen.
2. Stel de basiseisen in. Kies een minimumaantal trades, minimaal nettoresultaat, minimale profit factor en maximale drawdown. Je kunt een hogere spread en Monte Carlo als verplichte vervolgtoets toevoegen.
3. Kies **Find candidates**, bekijk de datacontrole en klik op **Zoeken starten**. Optionele tests draaien alleen wanneer eerdere eisen gehaald zijn.
4. Open een kandidaatgroep en vouw de kandidaten open. Vink twee of meer aan en klik op **Vergelijk geselecteerde kandidaten**. Per eis staan de meting, grens en uitkomst in de kandidaatdetails. Via **Open opgeslagen regels en trades** kun je de resultaten van de oorspronkelijke backtest bekijken.
5. Open later **Builder → Saved research → Inspect → Open toetsresultaten** om dezelfde beoordelingen terug te zien.

Een lege lijst met geslaagde kandidaten betekent dat niemand aan alle ingestelde eisen voldeed. Een verkennende zoekopdracht gebruikt de beschikbare geselecteerde reeks. Hergebruik van die periode als eindtest wordt als blootstelling behandeld.

## Walk-forward stabiliteitscontrole

Vink in het zoekformulier **Walk-forward verplicht toetsen** aan om de optionele toets toe te voegen. Stel daarna in:

- het aantal vensters (2 tot en met 10) en de out-of-sample-fractie per venster (5% tot 50%). De toegestane zoekdata wordt in opeenvolgende, niet-overlappende blokken verdeeld; elk blok heeft een trainingdeel en een teststaart.
- het minimumaantal trainingstrades en het minimumaantal out-of-sample-trades per venster. Dit zijn bewijsdrempels per venster, geen winsteisen.
- één drempel voor de **gezamenlijke out-of-sample nettowinst** in rekeningvaluta. Er is bewust geen minimumwinst per afzonderlijk venster.

De toets draait pas nadat de basiseisen (en eventueel spread en Monte Carlo) geslaagd zijn. Dezelfde bevroren kandidaat wordt in elk venster opnieuw uitgevoerd; er worden geen parameters op out-of-sample-data gekozen. De toets gebruikt uitsluitend de data die de zoekopdracht al had gezien: het is een stabiliteitscontrole over reeds geziene zoekdata en geen onafhankelijke eindtest. De gereserveerde eindtest blijft buiten deze toets.

Per kandidaat zie je één van vijf uitkomsten:

- **Geslaagd**: elk venster is aanwezig, technisch geldig en heeft genoeg trades, en de gezamenlijke out-of-sample nettowinst haalt de drempel. Een negatief maar verder geldig venster is op zichzelf geen afwijzing; het totaal beslist.
- **Afgewezen**: het bewijs is volledig, maar het gezamenlijke totaal blijft onder de drempel.
- **Onvoldoende bewijs**: een venster ontbreekt of heeft te weinig trades (of er is geen geschikte trainingsuitkomst). Er wordt dan geen totaal getoond; ontbrekende metingen blijven leeg in plaats van nul.
- **Technische fout**: een venster of de motor faalde technisch. Deze uitkomst gaat vóór onvoldoende bewijs.
- **Niet uitgevoerd**: een eerdere eis is niet gehaald, dus de toets is nooit gestart. Er is dan ook geen vensterarchief.

Open **Walk-forward stabiliteit** in de kandidaatdetails voor het archief: per venster de trainings- en testgrenzen (halfopen indices met de inclusieve bar-tijden), het doorgeschoven startkapitaal, de in-sample- en out-of-sample-trades, het nettoresultaat en de profit factor, de lokale bewijsdrempels, het vensterbesluit en de reden. Daaronder staat de gezamenlijke out-of-sample nettowinst tegenover de drempel. Een verliesvenster in een geslaagde rij is een gewone meting en krijgt geen rood afkeurmerk.

Het vensterarchief en de bevroren instellingen worden bij de studie opgeslagen. Bij pauzeren en hervatten wordt dezelfde zoekopdracht met dezelfde instellingen voortgezet, en bij heropenen leest de app hetzelfde archief zonder de berekening opnieuw te doen. Studies van vóór deze toets blijven ongewijzigd leesbaar; zij hebben geen walk-forward-controle en geen vensterarchief.

## Een studie met aparte perioden

1. Kies onder **Discover** de zoekruimte en toetseisen. Open **Set up a search with separate final-test data**.
2. Kies in **Builder** de start van de validatie en de eindtest in UTC. Geef aan of de data eerder zijn bekeken of getest. Bevries de studie. De app bewaart de dataset, kalender, kosten, perioden en eisen.
3. Kies aantal en seed en gebruik **Start saved search**. De app slaat iedere afgeronde kandidaat op. Met **Pause after current candidate** pauzeer je na de huidige kandidaat. Met **Resume saved search** ga je verder met dezelfde zoekopdracht.
4. Vergelijk kandidaten uit de studie. De basiseisen gelden in ontwikkeling en validatie. Spread en Monte Carlo worden op de ontwikkelperiode getoetst. Open trades en equity per periode om de cijfers te onderzoeken. De walk-forward-toets hoort bij de ontwikkeling: hij gebruikt alleen de ontwikkelperiode en dezelfde bevroren kandidaten. Wijzigt later uitsluitend de gereserveerde eindtestdata, dan blijven de kandidaatselectie en alle resultaten van vóór de eindtest gelijk; alleen de eindtest zelf verandert.
5. Leg één kandidaat vast voor de eindtest. De ontwikkelfase sluit daarna. Open de opgeslagen eindtest voor de trades, equity en de melding over eventuele eerdere blootstelling.

Monte Carlo simuleert hier het wegvallen en herschikken van eerder waargenomen trades. De uitkomst hangt af van de gekozen kans, seed en beschikbare trades. De spreadeis meet de verandering van het nettoresultaat als de spread stijgt, in rekeningvaluta. De grenswaarden zijn onderzoekskeuzes. De gegevens over eerdere blootstelling zijn beperkt tot bekende registraties en je eigen verklaring.
