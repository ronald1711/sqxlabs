# Handelskalenders en datacontrole

## Een zoekopdracht starten

1. Laad koersdata in **Data** en selecteer het instrument en tijdframe bovenaan.
2. Kies **Find strategies**. Vóór een nieuwe zoekopdracht verschijnt een datacontrole.
3. Bekijk de aantallen en open **Details bekijken** voor de tijdstippen. Alle weergegeven tijden zijn UTC; het einde van een onderbreking is exclusief.
4. Bij uitsluitend verwachte marktsluitingen kun je **Zoeken starten**. Bij echte of onbekende gaten kun je de bron en kalender onderzoeken, of bewust **Verkennend doorgaan** kiezen.

Verkennend doorgaan geldt alleen voor de beoordeelde dataset en die zoekopdracht. Het zet de algemene instelling voor datagaten niet aan. De datasetidentiteit, de beoordeling, het controletijdstip en de toestemming gaan mee in de opgeslagen zoekopdracht. Veranderde data of een gewijzigde kalender vereisen een nieuwe beoordeling.

Een eerder begonnen zoekopdracht gebruikt zijn oorspronkelijke verzoek. Gebruik de bestaande knop voor een nieuwe zoekopdracht wanneer je bewust nieuwe instellingen wilt gebruiken; daarmee wordt een eerdere opdracht niet geannuleerd.

## Wat betekenen de meldingen?

- **Verwachte marktsluiting:** de vastgelegde kalender geeft aan dat de markt gesloten was. Deze intervallen blokkeren het zoeken niet.
- **Ontbrekende data tijdens handelsuren:** volgens de kalender had er handel kunnen plaatsvinden, maar ontbreekt een candle.
- **Onbekend:** er is onvoldoende bevestigde informatie over de oorspronkelijke bron, tijdzone, kalender of geldigheidsperiode om het interval te classificeren.

Een aantal telt ontbrekende candle-intervallen: 60 ontbrekende minuten is bij M1 een aantal van 60, ook als het één aaneengesloten onderbreking is. De detailtabel groepeert aansluitende intervallen; maximaal 200 groepen worden weergegeven. De controle stopt met een fout bij meer dan één miljoen te onderzoeken intervallen.

Gemengde of onbekende EURUSD-bronnen krijgen **geen automatische brokerkalender**. Een weekendpatroon in de candles is geen bewijs van officiële handelsuren. Laat bron- en tijdzonebevestigingen uit totdat deze zijn onderzocht.

## Een kalender instellen

Open **Data → Bron en handelskalender → Geselecteerde dataset controleren**.

- Vul de oorspronkelijke koersbron in; `unknown` en `mixed` blijven toegestaan zonder bevestiging.
- Bevestig de bron en de UTC-tijdstempels alleen als die daadwerkelijk zijn gecontroleerd. Een bevestiging is een gebruikersverklaring; de app verifieert niet zelfstandig de juistheid van een bronreferentie.
- Schakel een expliciete kalender in en geef naam, identificatie, versie en IANA-tijdzone op, bijvoorbeeld `Europe/Amsterdam`.
- Geef de geldigheidsperiode en de datum waarop je de bron controleerde op. Buiten die periode blijft de classificatie onbekend.
- Vul per dag de lokale openingstijden in. `00:00-12:00, 12:05-24:00` bevat een dagelijkse pauze. Leeg betekent gesloten. Splits sessies die over middernacht lopen over twee dagen.
- Voeg gedateerde uitzonderingen toe, zoals `2026-12-25=closed` of `2026-12-24=00:00-13:00`.
- Voeg referenties en aannames toe. Alleen als kalender én bron én UTC-klok zijn bevestigd, gebruikt de nieuwe kalender de categorieën gesloten/open. Een voorlopige kalender blijft onbekend.

**Beoordeling vooraf bekijken** wijzigt niets. **Opslaan als nieuwe datasetversie** bewaart zowel de vorige als de nieuwe dataset. Bestaande onderzoeksruns veranderen niet.

De optionele keuze voor volgende QuestDB-imports geldt voor dit instrument op de huidige QuestDB-verbinding, voor alle tijdframes. Zij verandert andere reeds geladen tijdframes niet. Controleer en sla die afzonderlijk op, of importeer ze opnieuw met de gekozen instelling. CSV-imports en afzonderlijk gestarte MCP-processen nemen deze dashboardkeuze niet automatisch over.

Oude handmatig opgegeven UTC-kalenders blijven leesbaar en houden hun bestaande gedrag. Ze worden zichtbaar als oude, verklaarde kalender; dat is geen nieuw onafhankelijk bewijs. Het opslaan via de nieuwe editor vervangt zo'n kalender alleen op de geselecteerde datasetversie.

## Ontbrekende data opnieuw ophalen

**Zelfde periode opnieuw uit QuestDB ophalen** leest de huidige verbinding opnieuw, over de eerste tot en met de laatste candle van de geselecteerde dataset. Dit schrijft niets naar QuestDB.

De app bewaart een voorstel en toont toegevoegde, verwijderde en gewijzigde candles. De huidige dataset blijft geladen totdat je **Dit voorstel laden** kiest. De nieuwe bron- en kalenderinstellingen van die verbinding worden gebruikt. Ontbreekt de data ook bij de bron, dan kan opnieuw ophalen dat niet verhelpen. De app verzint of interpoleert geen candles.

## Bewijs en grenzen

Kalenders worden met hun versie, bronbinding, tijdzoneregels en opgeloste UTC-sessies in de dataset bewaard. Een toekomstige kalender- of tijdzonedatabasewijziging herschrijft die historische sessies niet.

De geautomatiseerde scenario's gebruiken afzonderlijk vastgelegde verwachte UTC-uitkomsten voor weekenden, Amerikaanse en Europese zomertijdwissels, een ontbrekend en een herhaald lokaal uur, dagelijkse pauzes, uitzonderlijke sluitingen, korte handelsdagen, ontbrekende candles en onbekende broninformatie. Deze synthetische scenario's bewijzen geen specifieke brokeruren.

De controle onderzoekt onderbrekingen tussen aanwezige candles. Ze bewijst niet dat er vóór de eerste of na de laatste candle geen data ontbreken, dat alle quotes juist zijn, of dat transacties, kosten en accountresultaten correct worden gesimuleerd. Een kalendercontrole rondt het volledige betrouwbaarheidsprogramma van SqxLabs niet af.
