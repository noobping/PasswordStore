# Keycord-documentatie

Keycord is een GUI voor standaard [`pass`](https://www.passwordstore.org/) -opslagen. Het behoudt de opslagindeling op schijf en voegt zoeken, bewerken, OTP-codes, sleutelbeheer, Git-workflows en ondersteuning voor softwaresleutels, FIDO-beveiligingssleutels en OpenPGP-smartcards toe.

## Handleidingen

- [Aan de slag](getting-started.md): setup, opslagen, eerste items en eerste zoekopdrachten
- [Zoekgids](search.md): gewoon zoeken, `reg` en `find`
- [Werkstromen](workflows.md): bewerken, OTP, hulpmiddelen, sneltoetsen en onderhoud
- [Machtigingen en backends](permissions-and-backends.md): Integrated vs Host, Flatpak-machtigingen, Git en sleutelsynchronisatie
- [Het verhaal van geheimen](story-of-secrets.md): codegerichte rondgang door het maken van opslagen, versleuteling van items, ontgrendelpaden en kopieren naar het klembord
- [Teams, werkgroepen en organisaties](teams-and-organizations.md): gedeelde opslagen, ontvangers, onboarding, offboarding en bootstrap-patronen
- [Gebruiksscenario's](use-cases.md): veelvoorkomende opstellingen, van persoonlijk gebruik tot gedeelde opslagen en beheerwerk

## Standaardindeling

Keycord leest en schrijft gewone `pass`-opslagen:

- een opslagmap zoals `~/.password-store`
- één geheim per bestand
- de eerste regel als wachtwoord
- latere `key: value`-regels als gestructureerde velden
- `.gpg-id` voor opslagontvangers

## Keycord functies

- bladeren, zoeken en bewerken over meer dan een opslag
- eenvoudige veldeneditor plus editor voor ruwe pass-bestanden
- ingebouwde wachtwoordgenerator en live eenmalige inlogcodes
- controles op zwakke wachtwoorden en hulpmiddelen voor terugkerende gebruikersnamen, e-mailadressen en URL's
- bestaande opslagen toevoegen, nieuwe opslagen maken, wachtwoorden importeren en opslagen herstellen vanuit Git
- sleutels voor een opslag kiezen, maken en importeren, inclusief FIDO-beveiligingssleutels en OpenPGP-smartcards
- optionele Git-synchronisatie, beheer van remotes en ondertekening van commits
- voor extra gevoelige opslagen kun je meer dan een sleutel verplicht maken

## Backendmatrix

| Mogelijkheid | Integrated | Host | Opmerkingen |
| --- | --- | --- | --- |
| Standaard-`pass`-opslagen bekijken en bewerken | Ja | Ja | Beide gebruiken de standaard opslagindeling. |
| Een aangepaste `pass`-opdracht gebruiken | Nee | Ja | Alleen Linux; stel de opdracht in bij Voorkeuren. |
| Zoeken, OTP, veldwaardebrowser, hulpmiddel voor zwakke wachtwoorden | Ja | Ja | Zoekgedrag is hetzelfde. |
| Opslagontvangers en door de app beheerde privésleutels beheren | Ja | Ja | Host-GPG-inspectie hangt af van hosttoegang. |
| Een opslag herstellen vanuit een Git-URL in de UI | Nee | Ja | Alleen Linux; hosttoegang vereist. |
| `pass import`-integratie | Nee | Ja | Alleen Linux; vereist de extensie `pass import`. |
| Git op afstand ophalen, mergen en pushen | Ja | Ja | Alleen Linux; vereist hosttoegang en een opslag met Git-backend. |
| Smartcard- / YubiKey-workflows | Ja | Ja | Alleen Linux; Flatpak heeft smartcardtoegang nodig. |
| Keycord-privésleutels synchroniseren met host-GPG | Ja | Ja | Alleen Linux en hosttoegang vereist. |

## Beperkingen

- Flatpak zonder hosttoegang:
  - Functies die alleen in Host beschikbaar zijn, zoals herstellen vanuit Git en `pass import`, blijven uitgeschakeld.
  - Als Host is geselecteerd zonder hosttoegang, valt Keycord terug op de Integrated-backend.
- Niet-Linux-builds:
  - Functies die alleen in Host beschikbaar zijn, zoals een aangepaste `pass`, herstellen vanuit Git en `pass import`, blijven verborgen.
  - workflows met hardwaresleutels blijven verborgen.
- Flatpak-smartcardondersteuning:
  - acties met hardwaresleutels hebben PC/SC-toegang nodig
  - met wachtwoord beveiligde softwaresleutels niet
- Gelaagde versleuteling:
  - dit is Keycord-specifiek
  - andere `pass`-apps kunnen die items niet lezen

## Begin

1. Lees [Aan de slag](getting-started.md).
2. Houd [Zoekgids](search.md) open terwijl je zoekopdrachten opbouwt.
3. Gebruik [Machtigingen en backends](permissions-and-backends.md) als een functie is uitgeschakeld.
