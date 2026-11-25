# SqxLabs verbinden met een lokale MCP-client

Deze route is voor Claude Desktop en andere clients die een lokaal programma via stdin/stdout kunnen starten. Het webdashboard is geen MCP-endpoint. Een AI-provider-URL in SqxLabs is evenmin een MCP-server.

## Instellen

1. Open **Settings → Connections → MCP connections** en kies **Load saved profile**.
2. Controleer het absolute pad naar `sqx-mcp`, de werkmap, het onderzoeksarchief en de logmap. Gebruik voor een eerste proef een afzonderlijke werkmap en synthetische CSV.
3. Kies een databron. CSV vereist een absoluut bestandspad, symbool en timeframe. QuestDB vereist daarnaast een host en poort. Bij **Specify in each tool request** geeft de client de bron per verzoek op.
4. Kies **Save MCP profile**, daarna **Test saved profile**. Deze test start een tijdelijk proces, doet de handshake en vraagt de tools op. Hij voert geen onderzoek uit en plaatst geen orders. Het testproces wordt beëindigd.
5. Kies **Copy client configuration**. Voeg de `sqxlabs`-vermelding samen met de bestaande `mcpServers` in de clientconfiguratie; vervang niet zomaar de configuratie van andere servers. Bij Claude Desktop is dit op macOS `~/Library/Application Support/Claude/claude_desktop_config.json`.
6. Laat de client de server opnieuw starten zodra er geen ander werk wordt onderbroken. Open daarna opnieuw het opgeslagen profiel om de lokale proceswaarneming te vernieuwen.

Een opgeslagen profiel is een afzonderlijk bestand. Na wijzigen en opslaan moet ook de gekopieerde clientconfiguratie worden bijgewerkt. Reeds draaiende processen blijven hun eerdere profiel gebruiken.

## Wat de status betekent

- **Configuration test:** het resultaat van een eerdere expliciete test. Test opnieuw na een verandering aan het uitvoerbare programma. Dit bewijst geen verbinding vanuit Claude of ChatGPT.
- **Running process:** een momentopname van lokale processen die voor dit precieze profiel een bestandsvergrendeling vasthouden. Een achtergebleven PID-bestand bewijst niets. Oudere of anders geconfigureerde processen kunnen onzichtbaar zijn; geen waarneming blijft **unknown**.
- **Client handshake:** de server heeft initialisatie waargenomen. De clientnaam is een eigen opgave en geen authenticatie. Een interne probe wordt apart aangeduid. Een eerdere handshake bewijst geen nog actieve clientverbinding.

Dashboarddata en dashboardinstellingen worden niet automatisch gedeeld met een MCP-proces. Geef simulatiekosten, accountvaluta en andere aannames expliciet mee in onderzoeksverzoeken. Databrondefaults gelden voor backtest, generate, portfolio en robustness. Zodra een verzoek zelf een bronveld opgeeft, vervangt dat de gehele brondefault.

QuestDB-credentials komen uit de omgeving/defaults van het MCP-proces, alleen als het daar ingestelde endpoint exact overeenkomt met het profiel. Bij een ander endpoint worden geen credentials meegestuurd. Deze instellingenpagina bewaart geen wachtwoorden.

## Logs en problemen onderzoeken

Open **Settings → Diagnostics → MCP server logs**. Filter op UTC-tijd, toolnaam, sessie of resultaat en kies **Read MCP logs**. **Download displayed MCP report** bevat dezelfde gefilterde momentopname.

Het rapport bevat alleen toegestane gebeurtenismetadata: tijd, proces/sessie, lokale verzoekvolgorde, tool/methode, duur en resultaatcategorie. Het bevat geen oorspronkelijke verzoek-ID, toolargumenten, volledige prompts, accountwaarden, ruwe foutteksten, credentials of onderzoeksbestanden. Het is geen onafhankelijk bewijs dat een strategie correct is uitgevoerd.

Logbestanden roteren volgens de ingestelde bestandsgrootte en het maximale aantal bestanden. Oudere gebeurtenissen worden bij het schrijven verwijderd op basis van de bewaartermijn; bij een gestopte server vindt geen achtergrondopruiming plaats. Rapporten filteren ook op bewaartermijn. Een rapport leest maximaal 20 MiB en toont maximaal 2.000 gebeurtenissen; begrenzing en ongeldige regels worden vermeld. Gelijktijdige rotatie kan de momentopname onvolledig maken.

Logs gaan naar de ingestelde map, nooit naar protocol-stdout. Bij een logfout blijft het protocol werken; stderr meldt `SQX_MCP_LOG_UNAVAILABLE`. Waar de processtatus nog kan worden bijgewerkt, toont deze **LOGGING DEGRADED**. Als ook statusopslag niet lukt, meldt stderr `SQX_MCP_STATUS_UNAVAILABLE` en blijft de status onbekend. Controleer dan ook de lokale MCP-logs van de client.

## Bewijs en beperkingen

Automatische proefgevallen controleren profielopslag, handshake/tools-list zonder onderzoek, een kleine CSV-backtest met een terugvindbaar archiefresultaat, ontbrekende data, onjuiste of vastlopende programma's, procesuitval, gelijktijdige logschrijvers en het uitsluiten van gevoelige invoer uit logs.

Een geslaagde interne probe is geen echte Claude/ChatGPT-clienttest. Het bewijs van zo'n clientproef moet apart worden vastgelegd. Een lokale stdio-configuratie kan niet rechtstreeks worden ingevuld als een remote HTTP-connector. Er wordt geen openbare server of tunnel geopend.

Deze MCP-functionaliteit rondt het volledige betrouwbaarheidsprogramma van SqxLabs niet af. De berekeningen, marktdata, uitvoeringsaannames en strategievalidatie behouden hun eigen bewijsstatus en beperkingen. MT5-vergelijkingswerk blijft voorlopig uitgesteld.
