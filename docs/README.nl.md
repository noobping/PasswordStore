# Keycord-documentatie

Keycord is een grafische app voor standaard [`pass`](https://www.passwordstore.org/)-opslagen. Het behoudt dezelfde mapindeling op schijf, werkt met compatibele pass-hulpmiddelen en gebruikt een adaptieve GTK-interface voor toetsenbord, aanwijzer en aanraking op desktop- en mobiele Linux.

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

- open een of meer wachtwoordopslagen en zoek op naam, opslag, veld, reguliere expressie of gestructureerde `find`-query
- bewerk items met formuliervelden of ruwe pass-bestandstekst, genereer wachtwoorden en kopieer wachtwoorden, gebruikersnamen of eenmalige inlogcodes
- voeg bestaande opslagen toe, maak nieuwe opslagen, importeer wachtwoorden op ondersteunde Linux-systemen of herstel een opslag uit Git met de Host-backend
- beheer opslagontvangers, mapspecifieke `.gpg-id`-bestanden, privésleutels, FIDO2-beveiligingssleutels en OpenPGP-smartcards
- synchroniseer Git-opslagen, beheer remotes, onderteken wijzigingen en bekijk geschiedenis met commitverificatiedetails
- gebruik de adaptieve GTK-interface met toetsenbord, aanwijzer of aanraking op desktop- en mobiele Linux

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
