# Teams, werkgroepen en organisaties

Gebruik gedeelde opslagen voor gedeelde referenties. Bewaar persoonlijke geheimen in persoonlijke opslagen.

## Model voor gedeelde opslagen

Standaardmodel:

- gebruik één of meer speciale gedeelde opslagen
- houd elke gedeelde opslag in Git
- zet elk lid op de lijst met ontvangers
- houd paden en velden consistent

Voorbeeldlabels:

```text
shared/github/admin
shared/vpn/helpdesk
infra/aws/prod/root
infra/k8s/staging/admin
oncall/pagerduty
```

## Opslagindeling

Gebruik één gedeelde opslag wanneer:

- de meeste leden dezelfde geheimen nodig hebben
- het team klein is
- de lijst met ontvangers stabiel is

Gebruik meerdere opslagen wanneer:

- lijsten met ontvangers verschillen
- toegang tot productie beperkter is dan toegang tot staging of interne systemen
- teams onafhankelijk worden onboarded en offboarded

Voorbeeldverdeling:

```text
~/stores/engineering
~/stores/support
~/stores/finance
~/stores/production-breakglass
```

## Keuze van backend

- gebruik op Linux `Host` om een opslag vanuit een Git-URL te herstellen of om `pass import` te gebruiken
- gebruik `Integrated` voor normaal bewerken en workflows met door de app beheerde sleutels
- in Linux Flatpak hebben Git-synchronisatie op afstand en andere host-gestuurde functies nog steeds hosttoegang nodig

Zie [Machtigingen en backends](permissions-and-backends.md) voor de volledige matrix.

## Nieuwe gedeelde opslag

### Een opslag maken

1. kies een lege map in **Wachtwoordopslagen**
2. open **Opslagsleutels**
3. voeg de eerste ontvangers toe
4. sla de ontvangers op
5. bevestig dat leden kunnen ontsleutelen
6. voeg een Git-remote toe als de opslag via Git wordt gedeeld

### Een bestaande opslag herstellen

1. schakel op Linux indien nodig over naar `Host`
2. gebruik **Wachtwoordopslag herstellen**
3. kies de doelmap
4. voer de repository-URL in
5. open **Opslagsleutels**
6. controleer de ontvangers

## Tijdelijke bootstrap-sleutel

Gebruik dit wanneer je een nieuwe gedeelde opslag maakt en niemand anders daar nog een sleutel in heeft.

Vereisten:

- een met wachtwoord beveiligde softwaresleutel
- een veilige manier om de ge-armorde privésleutel en het bijbehorende wachtwoord te delen

Stappen:

1. maak de gedeelde opslag
2. genereer op **Opslagsleutels** een tijdelijke, met wachtwoord beveiligde privésleutel
3. laat die sleutel geselecteerd en sla de opslag op
4. synchroniseer de opslag als die op Git is gebaseerd
5. kopieer de ge-armorde privésleutel uit de sleutellijst
6. deel de sleutel en het wachtwoord via een veilig kanaal
7. elk lid kloont of herstelt de opslag
8. elk lid importeert de tijdelijke sleutel met **Privésleutel importeren vanuit klembord** of **Privésleutel importeren**
9. elk lid bevestigt dat ontsleuteling werkt
10. elk lid genereert of voegt een eigen langetermijnsleutel toe
11. elk lid selecteert zijn eigen sleutel en slaat de opslag op
12. nadat alle leden werkende langetermijnsleutels hebben, verwijder je de tijdelijke sleutel uit de lijst met ontvangers en synchroniseer je
13. verwijder het tijdelijke sleutelbestand uit Keycord

Beperkingen:

- gebruik dit alleen voor met wachtwoord beveiligde softwaresleutels
- de gekopieerde export is nog steeds een privésleutel
- verwijder de tijdelijke sleutel niet voordat alle leden toegang hebben bevestigd

## Dagelijkse workflow

Voor het bewerken:

1. open de gedeelde opslag
2. controleer de Git-status
3. synchroniseer eerst als er remotes zijn geconfigureerd

Tijdens het bewerken:

- houd labels consistent
- houd veldnamen consistent
- gebruik waar mogelijk gestructureerde velden

Voorgestelde velden:

```text
username:
email:
url:
owner:
environment:
notes:
```

Na het bewerken:

1. sla het item op
2. synchroniseer vanuit een schone repo
3. als de repo niet schoon is of losgekoppeld is, repareer dit dan eerst op de host

## Onboarding

Bij het toevoegen van een lid:

1. kies het sleuteltype: bestaande sleutel, nieuwe met wachtwoord beveiligde sleutel of hardwaresleutel
2. voeg de ontvanger toe bij **Opslagsleutels**
3. sla de opslagontvangers op
4. laat het lid de opslag herstellen of openen
5. bevestig dat ontsleuteling werkt

Tijdens de eerste bootstrapfase kan een lid eerst de tijdelijke bootstrap-sleutel importeren en daarna een eigen langetermijnsleutel toevoegen.

## Offboarding

Bij het verwijderen van een lid:

1. verwijder diens ontvanger
2. sla de opslagontvangers op
3. synchroniseer de opslag
4. roteer gevoelige referenties indien nodig

Het verwijderen van een ontvanger maakt geheimen die die persoon al kent niet ongeldig. Behandel tijdelijke bootstrap-sleutels op dezelfde manier.

## Conventies

### Paden

Kies één patroon en houd dat stabiel:

```text
team/service/account
environment/service/account
department/tool/role
```

### Velden

Houd veldnamen stabiel:

- `username`
- `email`
- `url`
- `owner`
- `environment`
- `notes`

Keycord normaliseert `user`, `login` en `username` naar hetzelfde zoekveld. Andere velden worden gebruikt zoals geschreven.

### Opslaggrenzen

Splits opslagen wanneer:

- lijsten met ontvangers verschillen
- gevoeligheid verschilt
- regels voor review of goedkeuring verschillen

## Reviews en audits

Handige zoekopdrachten:

```text
find weak password
find otp
find url contains admin
find email is $username
reg:(?i)^prod/.+root$
```

Handige hulpmiddelen:

- **Zwakke wachtwoorden vinden**
- **Veldwaarden bekijken**

## Opslagen met hoog vertrouwen

Keycord kan vereisen dat alle geselecteerde beheerde sleutels voor een opslag nodig zijn.

Gebruik dit alleen als je de afweging accepteert:

- het is Keycord-specifiek
- andere `pass`-apps kunnen die items niet lezen

## Linux en Flatpak

Voor Flatpak:

- op Linux Flatpak is hosttoegang nodig voor restore-from-Git en Git-synchronisatie op afstand
- smartcardtoegang is nodig voor hardwaresleutels
- de Integrated-backend werkt nog steeds zonder die machtigingen voor lokale bewerkingen

Op Linux is synchronisatie van host-privésleutels ook beschikbaar. Lees eerst de risico's in [Machtigingen en backends](permissions-and-backends.md).

## Uitrol

1. begin met één gedeelde niet-productieopslag
2. spreek pad- en veldconventies af
3. bevestig dat elk lid kan ontsleutelen en synchroniseren
4. voeg controles op zwakke wachtwoorden en OTP toe
5. splits pas op in meer opslagen wanneer grenzen van ontvangers of gevoeligheid dat vereisen

## Gerelateerd leesvoer

- [Gebruiksscenario's](use-cases.md)
- [Machtigingen en backends](permissions-and-backends.md)
- [Werkstromen](workflows.md)
