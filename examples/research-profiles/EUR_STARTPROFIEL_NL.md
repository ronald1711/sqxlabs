# Voorlopig EUR-onderzoeksprofiel

Account: EUR, maximaal 1:30 leverage. Markt: EURUSD, contractgrootte 100.000.
Deze waarden komen uit de opgeslagen MT5-rekenreferentie. Die referentie bewijst
geen volledige strategie-uitvoering.

Voor de eerste synthetische onderzoeksvariant nemen we €10.000 startkapitaal,
2 FX-pips spread, nul extra slippage, nul commissie en nul financiering.
Dit zijn expliciete onderzoeks­aannames, geen vastgestelde Tickmill-tarieven.
De twee-pips spread komt uit de eerdere synthetische proef; die proef was in USD.
De EUR-versie moet opnieuw worden doorgerekend en is nog niet gelijkwaardig bewezen.

Vergelijk daarnaast 3 pips spread en een variant met 4 pips spread plus 1 pip
extra slippage. Commissie en financiering blijven onbekend totdat een tariefbron
beschikbaar is; nul betekent hier uitsluitend dat de basissimulatie ze weglaat.
Deze scenario’s zijn gevoeligheidsproeven, geen schatting van brokerkosten.

Gebruik historische, tijdig beschikbare USD/EUR-conversiekoersen. Een vaste koers
uit de MT5-rekenreferentie mag niet voor een volledige historie worden gebruikt.

Na een wijziging in instellingen voer je een nieuw onderzoek uit. De oude
opgeslagen invoer en resultaten blijven behouden. Het JSON-bestand naast dit
bestand beschrijft het profiel; het is geen direct importeerbare engineconfig.
De ontwikkelversie biedt een knop in Instellingen om dit profiel toe te passen.
Je kiest zelf wanneer; de knop verandert de geselecteerde markt niet. De UI
vermeldt onbekende kosten en laat handmatige wijzigingen toe. Een volledige
EUR-strategieproef en releasecontroles voor deze uitbreiding staan nog open.
