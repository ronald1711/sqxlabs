# Voorlopig EUR-onderzoeksprofiel

Account: EUR, maximaal 1:30 leverage. Markt: EURUSD, contractgrootte 100.000.
Deze waarden komen uit de opgeslagen MT5-rekenreferentie. Die referentie bewijst
geen volledige strategie-uitvoering.

Voor de eerste synthetische onderzoeksvariant nemen we €10.000 startkapitaal,
2 FX-pips spread, nul extra slippage, nul commissie en nul financiering.
Dit zijn expliciete onderzoeks­aannames, geen vastgestelde Tickmill-tarieven.
De twee-pips spread komt uit de eerdere synthetische proef; die proef was in USD.
De EUR-versie moet opnieuw worden doorgerekend en is nog niet gelijkwaardig bewezen.

Een afzonderlijke synthetische rekencontrole staat in
`crates/sqx-backtest/tests/eur_research_profile.rs`: bij een vlakke biedkoers
van 1,1000 en 0,1 lot controleert die beide richtingen, alle drie scenario’s,
EUR-conversie, eindliquidatie en commissie van 0 of 6 EUR per lot.
De bruto round-tripverliezen zijn respectievelijk 2, 3 en 6 USD; omrekening
gebeurt tegen de sluitprijs en commissie wordt daarna in EUR afgetrokken.
Herhaling moet byte-identieke resultaten geven. Dit is beperkte rekenkundige
dekking, geen volledige vergelijking met brokeruitvoering.

Vergelijk daarnaast 3 pips spread en een variant met 4 pips spread plus 1 pip
extra slippage. Commissie en financiering blijven onbekend totdat een tariefbron
beschikbaar is; nul betekent hier uitsluitend dat de basissimulatie ze weglaat.
Deze scenario’s zijn gevoeligheidsproeven, geen schatting van brokerkosten.

Voor EURUSD/EUR gebruikt de engine de beschikbare uitvoerings- of waarderingsprijs
voor directe conversie. Andere valutapaden kunnen historische conversiekoersen
vereisen. Gebruik nooit één vaste koers uit de MT5-rekenreferentie voor de historie.

Na een wijziging in instellingen voer je een nieuw onderzoek uit. De oude
opgeslagen invoer en resultaten blijven behouden. Het JSON-bestand naast dit
bestand beschrijft het profiel; het is geen direct importeerbare engineconfig.
De ontwikkelversie biedt een knop in Instellingen om dit profiel toe te passen.
Je kiest zelf wanneer; de knop verandert de geselecteerde markt niet. De UI
vermeldt onbekende kosten en laat handmatige wijzigingen toe. Een volledige
EUR-strategieproef en releasecontroles voor deze uitbreiding staan nog open.

## Aanvullende signaal- en kascontrole

`crates/sqx-backtest/tests/eur_signal_reference.rs` controleert vier handmatig
uitgerekende scenario’s met 0,1 lot, 2 pips spread en EUR 6 commissie per lot.
Een kruising op candle 1 geeft een entry op de open van candle 2; de omgekeerde
kruising op candle 2 sluit op de open van candle 3.

| Richting | Entry | Exit | Bruto USD | Netto EUR |
| --- | ---: | ---: | ---: | --- |
| Long verlies | 1,1022 | 1,0980 | −42 | −42 / 1,0980 − 0,60 |
| Long winst | 1,1022 | 1,1050 | 28 | 28 / 1,1050 − 0,60 |
| Short verlies | 1,0980 | 1,1022 | −42 | −42 / 1,1022 − 0,60 |
| Short winst | 1,0980 | 1,0952 | 28 | 28 / 1,0952 − 0,60 |

De proef controleert signaaltijden, uitvoeringsprijzen, commissie, kasmutatie en
eindsaldo. Een later toegevoegde prijs verandert de afgesloten trade en haar
exitbeslissing niet. Deze proef dekt geen SL/TP, trailing, financieringsrollover,
margin call, ticks of werkelijke brokeruitvoering.
