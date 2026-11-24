# SqxLabs – eerste onafhankelijke MT5-proef

Voer dit uit in de MT5 die je via SiliconMetaTrader5 opent. Een losse installatie is niet nodig. Deze proef is nog niet geslaagd verklaard: jouw testerlog en rapport moeten eerst worden gecontroleerd.

De proef gebruikt verzonnen koersen en een virtueel testeraccount van **USD 10.000, hefboom 1:30**. Dit wijzigt niet de valuta of het saldo van je EUR-demoaccount. De referentiestrategie weigert buiten de Strategy Tester te draaien en accepteert alleen een eigen testsymbool. Er worden geen orders naar de broker gestuurd.

## 1. Twee bestanden plaatsen en compileren

Open in MT5 **File → Open Data Folder**. Kopieer:

- `prepare_custom_symbol.mq5` naar `MQL5/Scripts`.
- `constant_signal_exit.mq5` naar `MQL5/Experts`.

Open MetaEditor vanuit deze MT5 met F4 (op de Mac eventueel Fn+F4). Open beide bestanden en compileer elk met F7. Beide moeten **0 errors** tonen. De setupbron is nieuw en moet in jouw geïnstalleerde versie worden gecompileerd; bij fouten niet doorgaan, maar de fouttekst bewaren. In het pakket zitten geen uitvoerbare bestanden.

## 2. Eenmalig de testmarkt maken

Ga terug naar MT5. Ververs zo nodig Navigator. Onder **Scripts**: start `prepare_custom_symbol` op een bestaande chart. Zet de invoer **CreateSyntheticTestMarket = true** en bevestig.

Het script maakt uitsluitend het nieuwe symbool **SQX_EURUSD**. Het toont dit nieuwe testsymbool in Market Watch en overschrijft geen bestaand symbool. Het bevat geen orderfuncties. Het importeert 240 M1-candles en 960 bid/ask-ticks: 120 opwarmcandles en 120 testcandles. Swap staat uit; de spread is vast twee pips.

Controleer onder **Toolbox → Experts** de melding:

`SQX_FIXTURE_READY symbol=SQX_EURUSD bars=240 ticks=960 tick_and_bar_readback=matched`

**Ga alleen verder als deze melding er staat.** Bij een bestaande naam of een andere fout: bewaar de melding; verwijder of overschrijf niet op goed geluk iets. Een mislukte setup kan een gedeeltelijk aangemaakt testsymbool achterlaten.

## 3. De long-proef

Open **View → Strategy Tester** (Ctrl+R) en kies een enkele test, geen optimalisatie. Gebruik:

| Instelling | Waarde |
|---|---|
| Expert | constant_signal_exit |
| Symbool | SQX_EURUSD |
| Periode | M1 |
| Begin / einde | 2026.01.05 / 2026.01.06 |
| Model | Every tick based on real ticks |
| Uitvoeringsvertraging | No delay / 0 |
| Deposit / currency | 10000 / USD |
| Leverage | 1:30 |
| Forward / optimization | Uit |
| Agents | Alleen lokaal; remote en MQL5 Cloud uit |
| Profit in pips | Uit |
| Commissie | Expliciet 0 in de geavanceerde testerinstellingen |
| ShortSide | false |
| SignalBar | 2026.01.05 00:00:00 |
| Lots | 0.1 |

Het model heet “real ticks”, maar deze proef gebruikt de door het script geïmporteerde **synthetische** ticks. Kies niet “Math calculations” of alleen openingsprijzen. Laat Algo Trading voor gewone charts uit; deze expert hoort alleen in de tester.

Druk op **Start**. Verwacht één afgeronde longpositie: koop om 00:01 tegen 1.10030 en verkoop om 00:02 tegen 1.10020. Netto **−1,00 USD**, eindbalans **9.999,00 USD**. Een verschoven begindatum, gegenereerde vervangende ticks of een afwijking is een onderzoeksuitkomst, geen reden om instellingen aan te passen totdat het groen is.

## 4. Bewijs bewaren, daarna short

Bewaar na elke proef het volledige testerlog en het HTML-rapport, inclusief deals en gebruikte instellingen. Gebruik verschillende namen: `long-reference` en `short-reference`. Bewaar ook het Experts-log van de setup. Deel geen wachtwoorden of API-sleutels.

Voer daarna dezelfde test uit met alleen **ShortSide = true**. Verwacht verkoop om 00:01 tegen 1.10010 en terugkoop om 00:02 tegen 1.10040: netto **−3,00 USD**, eindbalans **9.997,00 USD**.

Bewaar beide rapporten en logs voordat je verder test. Laat mij weten zodra de setupmelding verschijnt of als er een compilatie-/setupfout is. Ik kan daarna de twee runs vergelijken met de vastgelegde SqxLabs-uitkomsten. Een `SQX_REFERENCE_COMPLETE ok=1`-regel alleen is onvoldoende bewijs: ook instellingen, dataherkomst, tijden, prijzen en kosten moeten overeenkomen.

## Wat dit wel en niet bewijst

Dit is de eerste kleine uitvoeringsvergelijking: constante signalen, één positie, vaste spread, geen commissie of financiering. Het valideert nog niet jouw EUR-account, USDJPY-conversie, indicatoren, stops, dynamische exits of een winstgevende handelsstrategie. Die volgen met afzonderlijke scenario's.

De CSV- en JSON-bestanden in het pakket leggen de 120 testcandles vast; het setupscript voegt daarvoor 120 opwarmcandles toe. De `.ini`/`.set`-bestanden zijn vastgelegde referentie-instellingen; je hoeft geen terminal via een commandoregel te starten. De nulcommissie moet je apart controleren in de tester. Bestandsintegriteit staat in `package-manifest.json`; hashes bewijzen niet dat de proef is uitgevoerd.

Achtergrond over de benodigde voorgeschiedenis: https://www.metatrader5.com/en/terminal/help/algotrading/testing
