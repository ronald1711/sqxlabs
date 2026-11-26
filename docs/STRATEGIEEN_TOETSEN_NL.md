# Strategieën zoeken, toetsen en vergelijken

Selecteer bovenaan een instrument en tijdframe. Controleer onder **Data** de kalender, onderbrekingen en herkomst. Controleer onder **Settings** de rekeningvaluta, spread, slippage, commissie en financiering. De resultaten horen bij deze gekozen aannames.

## Een verkennende zoekopdracht

1. Open **Discover** en stel de zoekruimte in. Kies onder **Exploratory search** het aantal kandidaten en een seed om de zoekopdracht later te kunnen herhalen.
2. Stel de basiseisen in. Kies een minimumaantal trades, minimaal nettoresultaat, minimale profit factor en maximale drawdown. Je kunt een hogere spread en Monte Carlo als verplichte vervolgtoets toevoegen.
3. Kies **Find candidates**, bekijk de datacontrole en klik op **Zoeken starten**. Optionele tests draaien alleen wanneer eerdere eisen gehaald zijn.
4. Open een kandidaatgroep en vouw de kandidaten open. Vink twee of meer aan en klik op **Vergelijk geselecteerde kandidaten**. Per eis staan de meting, grens en uitkomst in de kandidaatdetails. Via **Open opgeslagen regels en trades** kun je de resultaten van de oorspronkelijke backtest bekijken.
5. Open later **Builder → Saved research → Inspect → Open toetsresultaten** om dezelfde beoordelingen terug te zien.

Een lege lijst met geslaagde kandidaten betekent dat niemand aan alle ingestelde eisen voldeed. Een verkennende zoekopdracht gebruikt de beschikbare geselecteerde reeks. Hergebruik van die periode als eindtest wordt als blootstelling behandeld.

## Een studie met aparte perioden

1. Kies onder **Discover** de zoekruimte en toetseisen. Open **Set up a search with separate final-test data**.
2. Kies in **Builder** de start van de validatie en de eindtest in UTC. Geef aan of de data eerder zijn bekeken of getest. Bevries de studie. De app bewaart de dataset, kalender, kosten, perioden en eisen.
3. Kies aantal en seed en gebruik **Start saved search**. De app slaat iedere afgeronde kandidaat op. Met **Pause after current candidate** pauzeer je na de huidige kandidaat. Met **Resume saved search** ga je verder met dezelfde zoekopdracht.
4. Vergelijk kandidaten uit de studie. De basiseisen gelden in ontwikkeling en validatie. Spread en Monte Carlo worden op de ontwikkelperiode getoetst. Open trades en equity per periode om de cijfers te onderzoeken.
5. Leg één kandidaat vast voor de eindtest. De ontwikkelfase sluit daarna. Open de opgeslagen eindtest voor de trades, equity en de melding over eventuele eerdere blootstelling.

Monte Carlo simuleert hier het wegvallen en herschikken van eerder waargenomen trades. De uitkomst hangt af van de gekozen kans, seed en beschikbare trades. De spreadeis meet de verandering van het nettoresultaat als de spread stijgt, in rekeningvaluta. De grenswaarden zijn onderzoekskeuzes. De gegevens over eerdere blootstelling zijn beperkt tot bekende registraties en je eigen verklaring.
