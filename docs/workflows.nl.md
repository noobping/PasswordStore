# Werkstromen

Deze pagina beschrijft veelvoorkomende taken nadat je minstens één opslag hebt geconfigureerd.

## Items maken, openen en organiseren

### Een nieuw item maken

Druk op `Ctrl+N` en voer een opslagpad in zoals:

```text
personal/github
work/vpn/admin
```

Als er meer dan één opslag is geconfigureerd, laat Keycord je eerst de doelopslag kiezen.

### Hernoemen, verplaatsen en verwijderen

Vanuit de lijstweergave:

- `F2` hernoemt het geselecteerde item.
- `Ctrl+M` verplaatst het geselecteerde item.
- `Delete` verwijdert het geselecteerde item.

### Kopiëren vanuit de lijst

Vanuit de lijstweergave kopieert `Ctrl+C` de wachtwoordregel van het geselecteerde item.

## Gestructureerde velden of het ruwe pass-bestand bewerken

### Gestructureerde editor

Gebruik de standaardeditor voor:

- wachtwoord,
- gebruikersnaam,
- OTP,
- dynamische `key: value`-velden,
- wachtwoordgeneratie,
- snelle kopieeracties.

Bekende aliassen voor gebruikersnamen zoals `user:` en `login:` worden genormaliseerd naar het veld voor de gebruikersnaam.

### Ruwe editor

Druk op `Ctrl+Shift+R` om het ruwe pass-bestand te openen.

Gebruik die wanneer je het volgende nodig hebt:

- de exacte indeling behouden,
- niet-gestructureerde notities bewerken,
- de letterlijke regel `otpauth://` inspecteren,
- ongebruikelijke gegevens repareren die elders zijn geïmporteerd.

### Opslaggedrag

`Ctrl+S` is contextgevoelig:

- op een wachtwoordpagina slaat het het huidige pass-bestand op,
- op de pagina voor opslagontvangers slaat het opslagsleutels op,
- op de startpagina synchroniseert het opslagen wanneer Git-synchronisatie beschikbaar is.

## Sjablonen, opschonen en terugval voor gebruikersnamen

### Sjabloon voor nieuwe wachtwoorden

Voorkeuren bevat **Nieuw wachtwoordsjabloon**. Het sjabloon wordt de inhoud na de wachtwoordregel wanneer je een nieuw item maakt.

Typisch sjabloon:

```text
username:
email:
url:
```

Keycord kan ontbrekende sjabloonvelden ook toepassen op een bestaand gestructureerd pass-bestand zonder velden te overschrijven die al aanwezig zijn.

### Pass-bestand opschonen

Druk op `Ctrl+Shift+K` om het huidige pass-bestand op te schonen.

De opschoonactie verwijdert lege:

- regels met gebruikersnamen,
- regels met gestructureerde velden,
- OTP-regels.

Als je **Lege velden wissen voor opslaan** inschakelt in Voorkeuren, voert Keycord die opschoning automatisch uit vóór validatie en opslaan.

### Terugval voor gebruikersnamen

Wanneer een item geen versleuteld veld voor de gebruikersnaam heeft, kan Keycord één van twee terugvalopties tonen:

- **Mapnaam gebruiken**: het laatste mapsegment wordt de getoonde gebruikersnaam.
- **Bestandsnaam gebruiken**: de naam van het pass-bestand wordt de getoonde gebruikersnaam.

Dit beïnvloedt de weergave en label-afgeleid gedrag wanneer het pass-bestand zelf geen veld voor een gebruikersnaam heeft.

## Wachtwoorden genereren

Druk op `Ctrl+Shift+G` in de wachtwoordeditor om een wachtwoord te genereren.

De generator gebruikt opgeslagen voorkeuren voor:

- totale lengte,
- minimum aantal kleine letters,
- minimum aantal hoofdletters,
- minimum aantal cijfers,
- minimum aantal symbolen.

Als je een minimum op `0` zet, wordt die tekenklasse uitgeschakeld. Als elk minimum `0` is, houdt Keycord de generator toch bruikbaar door intern kleine letters opnieuw in te schakelen.

## Werken met OTP / TOTP

Druk op `Ctrl+Shift+O` om een OTP-veld toe te voegen aan het huidige item.

Hoe het werkt:

- Keycord slaat OTP-gegevens op als een `otpauth://`-URL.
- In de gestructureerde editor toont Keycord een live code en afteltimer.
- Als je op de OTP-rij klikt, schakelt die naar bewerkmodus zodat je het geheim kunt bijwerken.
- Lege OTP-geheimen worden bij het opslaan afgewezen.

Gebruik `find otp` in zoeken wanneer je elk item nodig hebt waarvoor OTP is ingeschakeld.

## Zoeken, zichtbaarheid, herladen en synchroniseren

### Zoeken

Druk op `Ctrl+F` om de zoekbalk te tonen of te verbergen.

Keycord ondersteunt:

- gewoon zoeken op label,
- regex-zoeken met `reg`,
- gestructureerd zoeken met `find`.

Je kunt Keycord ook direct met een zoekopdracht starten:

```sh
keycord 'find url contains github'
```

Zie [Zoekgids](search.md) voor de volledige syntaxis.

### Verborgen en dubbele items

Druk op `Ctrl+H` om zowel verborgen als dubbele items op de startlijst te schakelen.

Gebruik dit wanneer:

- je items met een puntprefix of anderszins verborgen items in de opslag bewaart,
- je bewust dubbele labels over meerdere opslagen heen bewaart,
- je een auditgerichte weergave wilt in plaats van de schonere standaardlijst.

### Vernieuwen en synchroniseren

- `F5` laadt de huidige lijstcontext opnieuw.
- `Ctrl+Shift+S` synchroniseert op Git gebaseerde opslagen vanaf de startpagina wanneer Git-synchronisatie beschikbaar is.

Git-synchronisatie slaagt alleen wanneer elke synchroniseerbare opslag:

- een Git-repository heeft,
- minstens één remote heeft,
- een uitgecheckte branch heeft,
- geen lokale wijzigingen zonder commit heeft.

Als de repo niet schoon is of branchreparatie nodig heeft, gebruik dan eerst Git op de host en keer daarna terug naar Keycord.

## Pagina met hulpmiddelen

Druk op `Ctrl+T` om Hulpmiddelen te openen.

De pagina Hulpmiddelen is opgesplitst in de groepen **Hulpmiddelen** en **Logs**.

### Veldwaarden bekijken

Dit hulpmiddel leest de momenteel geladen lijst en toont:

- doorzoekbare veldnamen,
- unieke waarden voor elk veld,
- hoeveel items een waarde delen.

Als je een waarde selecteert, wordt een exacte `find`-zoekopdracht terug toegepast op de startlijst.

Dit hulpmiddel sluit ruwe OTP-URL's uit van de veldcatalogus.

### Zwakke wachtwoorden vinden

Dit hulpmiddel scant de eerste wachtwoordregel van de momenteel geladen lijst en markeert items die niet voldoen aan de basiscontroles van Keycord.

Het meldt deze gevallen:

- leeg wachtwoord,
- wachtwoord dat alleen uit witruimte bestaat,
- veelvoorkomende wachtwoorden zoals `password`, `123456` of `letmein`,
- herhaalde wachtwoorden met één teken,
- wachtwoorden korter dan 8 tekens,
- eenvoudige opeenvolgende ASCII-tekenreeksen,
- korte wachtwoorden met zeer beperkte tekenvariatie,
- korte wachtwoorden met slechts één tekenklasse,
- korte wachtwoorden met zeer weinig unieke tekens.

Langere meerwoordige wachtwoordzinnen zoals deze worden niet door deze controle gemarkeerd:

```text
correct horse battery staple
```

### Wachtwoorden importeren

De importpagina verschijnt wanneer aan al deze voorwaarden is voldaan:

- Linux-build,
- Host-backend is actief,
- er bestaat minstens één opslag,
- de geconfigureerde host-`pass`-opdracht ondersteunt `pass import`.

Je kunt kiezen:

- de doelopslag,
- de naam van de importeur,
- een optioneel bronbestand of een optionele bronmap,
- een optionele submap in de opslag.

### Logs en helpers voor setup

Linux-builds tonen een logweergave met `F12`.

De groep **Logs** kan het volgende bevatten:

- **Documentatie**, waarmee de losse documentatiepagina wordt geopend,
- **Loguitvoer openen**,
- **Loguitvoer kopiëren** in reguliere builds,
- een actie om de lokale appmenu-installatie te installeren of te verwijderen in builds met setup.

## Werkstromen voor ontvangers en sleutels

Voor wijzigingen op opslagniveau aan sleutels:

1. Open **Wachtwoordopslagen** in Voorkeuren.
2. Open de pagina **Opslagsleutels** van de doelopslag.
3. Voeg ontvangers toe of verwijder ze.
4. Genereer eventueel een privésleutel, importeer er een of koppel een hardwaresleutel.
5. Sla de wijzigingen op.

Op Linux kan de Integrated-backend vereisen dat een beheerde privésleutel is ontgrendeld voordat Keycord items opnieuw kan versleutelen of de Git-commit kan ondertekenen. Als de dialoog voor het ontgrendelen van ondertekening wordt gesloten, kan het opslaan doorgaan zonder Git-handtekening.

## Sneltoetsen

### Pass-bestanden

| Sneltoets | Actie |
| --- | --- |
| `Ctrl+N` | Een nieuw item openen |
| `Ctrl+S` | Huidige pagina opslaan, of synchroniseren vanaf de startpagina wanneer beschikbaar |
| `Ctrl+Shift+R` | Ruwe tekst openen |
| `Ctrl+Shift+C` | Wachtwoord kopiëren |
| `Ctrl+Shift+U` | Gebruikersnaam kopiëren |
| `Ctrl+Shift+T` | OTP kopiëren |
| `Ctrl+Shift+A` | Sjabloon toepassen |
| `Ctrl+Shift+F` | Veld toevoegen |
| `Ctrl+Shift+O` | OTP-veld toevoegen |
| `Ctrl+Shift+P` | Wachtwoordopties |
| `Ctrl+Shift+K` | Pass-bestand opschonen |
| `Ctrl+Shift+G` | Wachtwoord genereren |
| `Ctrl+Z` | Wijzigingen ongedaan maken of terugzetten |

### Lijst en navigatie

| Sneltoets | Actie |
| --- | --- |
| `Ctrl+F` | `find` aan- of uitzetten |
| `Ctrl+C` | Wachtwoord van geselecteerd item kopiëren |
| `F2` | Geselecteerd pass-bestand hernoemen |
| `Ctrl+M` | Geselecteerd pass-bestand verplaatsen |
| `Delete` | Geselecteerd pass-bestand verwijderen |
| `Ctrl+H` | Verborgen en dubbele items tonen |
| `Ctrl+Shift+S` | Opslagen synchroniseren |
| `F5` | Huidige lijstcontext vernieuwen |
| `Escape` | Teruggaan |
| `Home` | Naar start |
| `Ctrl+Shift+N` | Opslag toevoegen of maken |
| `Ctrl+G` | Git-hulpmiddelen openen |

### Algemeen

| Sneltoets | Actie |
| --- | --- |
| `Ctrl+,` | Voorkeuren openen |
| `Ctrl+Shift+D` | Documentatie openen |
| `Ctrl+T` | Hulpmiddelen openen |
| `Ctrl+?` | Sneltoetsen tonen |
| `F1` | Over |
| `F12` | Loguitvoer openen |

## Verder lezen

- [Zoekgids](search.md)
- [Machtigingen en backends](permissions-and-backends.md)
- [Gebruiksscenario's](use-cases.md)
