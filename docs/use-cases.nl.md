# Gebruiksscenario's

Praktische voorbeelden en korte handleidingen.

## Persoonlijke opslag

Setup:

- backend: `Integrated`
- aantal opslagen: `1`
- typische opslag: `~/.password-store`

Veelgebruikte workflow:

1. voeg de opslag toe of bevestig die in Voorkeuren
2. maak items zoals `personal/github` of `personal/bank`
3. houd het standaardsjabloon met gebruikersnaam, e-mail en URL
4. voeg waar nodig OTP toe
5. controleer met zoeken en hulpmiddelen

Veelgebruikte zoekopdrachten:

```text
find url contains github
find otp
find weak password
```

## Persoonlijke en werkopslagen

Setup:

- backend: `Integrated` voor normaal gebruik
- schakel op Linux alleen over naar `Host` voor herstellen vanuit Git of `pass import`
- aantal opslagen: `2+`

Voorbeeld:

```text
~/.password-store
~/work-password-store
```

Veelgebruikte workflow:

1. voeg beide opslagen toe
2. houd labels consistent
3. zoek over beide opslagen heen
4. gebruik de browser voor veldwaarden voor terugkerende gebruikersnamen, domeinen of URL's

Voorbeeldlabels:

```text
personal/github
work/github
work/vpn
```

Veelgebruikte zoekopdrachten:

```text
github
find user alice
reg:(?i)^work/.+vpn$
```

## Gedeelde teamopslag

Zie voor een uitgebreidere handleiding, inclusief het patroon met een tijdelijke bootstrap-sleutel, [Teams, werkgroepen en organisaties](teams-and-organizations.md).

Setup:

- backend: `Host` op Linux voor het eerste herstel; daarna kan elke backend worden gebruikt
- aantal opslagen: meestal `1` gedeelde opslag
- één ontvanger per teamlid

Veelgebruikte workflow:

1. herstel de opslag vanuit Git
2. open **Opslagsleutels** en controleer de ontvangers
3. voeg ontvangers toe of verwijder ze wanneer het team verandert
4. controleer de Git-status voor synchronisatie
5. synchroniseer alleen vanuit een schone repo

Veelgebruikte zoekopdrachten:

```text
find url contains admin
find weak password
find otp AND url contains company.com
```

## Opslag met hoog vertrouwen

Setup:

- backend: `Integrated`
- een speciale opslag of subopslag
- meerdere geselecteerde beheerde sleutels

Veelgebruikte workflow:

1. open **Opslagsleutels**
2. voeg de vereiste ontvangers toe
3. schakel **Alle geselecteerde sleutels vereisen** in
4. sla de opslagontvangers op

Beperking:

- dit is Keycord-specifiek
- andere `pass`-apps kunnen die items niet lezen

Typisch gebruik:

- break-glass-referenties
- rootreferenties voor productie
- toegangsbeheer met meerdere partijen

## DevOps- en beheerwerk

Setup:

- backend: `Integrated` voor normaal gebruik
- `Host` waar herstellen of importeren nodig is
- meerdere opslagen of strikte padconventies

Voorbeeldlabels:

```text
prod/aws/root
prod/k8s/admin
staging/vpn
shared/oncall/github
```

Veelgebruikte workflow:

1. gebruik gestructureerde labels en velden
2. voer controles op zwakke wachtwoorden en OTP uit
3. roteer ontvangers via **Opslagsleutels**
4. herstel opslagen vanuit Git wanneer werkstations opnieuw worden opgebouwd
5. genereer of importeer langetermijnsleutels voor elke beheerder

Veelgebruikte zoekopdrachten:

```text
find weak password
find url contains github
find email is $username
reg:(?i)^prod/.+vpn$
find url contains internal.company
find otp AND user admin
```

## Vuistregel

- één persoonlijke opslag: begin met `Integrated`
- scheiding tussen persoonlijk en werk: gebruik meerdere opslagen
- gedeelde teamopslag: focus op ontvangers en Git-status
- opslag met hoog vertrouwen: gebruik gelaagde versleuteling alleen als je het compatibiliteitsverlies accepteert
- beheerzware opstelling: gebruik padconventies, gestructureerde velden en de pagina met hulpmiddelen

## Verder lezen

- [Aan de slag](getting-started.md)
- [Zoekgids](search.md)
- [Machtigingen en backends](permissions-and-backends.md)
- [Teams, werkgroepen en organisaties](teams-and-organizations.md)
