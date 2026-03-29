# Machtigingen en backends

Deze pagina beschrijft functies die afhangen van de Integrated-backend, de Host-backend, Flatpak-hosttoegang, smartcardtoegang of host-opdrachtfuncties die alleen op Linux beschikbaar zijn.

## Overzicht van backends

### Integrated-backend

De Integrated-backend leest en schrijft de opslag direct. Dit is de standaard.

Gebruik deze wanneer je het volgende wilt:

- directe opslagtoegang zonder afhankelijk te zijn van een host-`pass`-opdracht,
- door de app beheerde privésleutels,
- gestructureerd bewerken, zoeken, OTP en hulpmiddelen,
- door Keycord beheerde updates van opslagontvangers.

### Host-backend

De Host-backend voert je geconfigureerde `pass`-opdracht uit. Deze is alleen beschikbaar op Linux.

Gebruik deze wanneer je het volgende nodig hebt:

- een aangepaste `pass`-opdracht,
- herstellen vanuit Git in de UI,
- `pass import`,
- compatibiliteit met bestaande `pass`-extensies of wrappers aan hostzijde.

## Mogelijkhedenmatrix

| Mogelijkheid | Integrated | Host | Extra vereiste |
| --- | --- | --- | --- |
| Bladeren, zoeken, kopiëren, bewerken, OTP, hulpmiddelen | Ja | Ja | Geen |
| Een aangepaste `pass`-opdracht gebruiken | Nee | Ja | Alleen Linux; configureer de opdracht bij Voorkeuren |
| Lokale opslagen maken of koppelen in de UI | Ja | Ja | Minstens één ontvanger is vereist voor een nieuwe opslag |
| Een opslag vanuit Git herstellen in de UI | Nee | Ja | Alleen Linux; hosttoegang plus `git` |
| `pass import`-integratie | Nee | Ja | Alleen Linux; `pass import` moet beschikbaar zijn in de geconfigureerde opdracht |
| Git-remotes beheren | Ja | Ja | Alleen Linux in de UI; hosttoegang vereist voor netwerkbewerkingen op remotes |
| Git-synchronisatie op afstand | Ja | Ja | Alleen Linux; hosttoegang, schone repo, uitgecheckte branch en remotes |
| Smartcard- / YubiKey-workflows | Ja | Ja | Smartcardtoegang in Flatpak |
| Keycord-sleutels synchroniseren met host-GPG | Ja | Ja | Alleen Linux en hosttoegang vereist |

## Flatpak-machtigingen

### Hosttoegang

Zonder hosttoegang zijn Linux-functies die door de host worden aangestuurd beperkt.

Wat hosttoegang ontgrendelt:

- Host-backendgedrag,
- hostprogramma's zoals `gpg`,
- `pass import`,
- herstellen vanuit Git,
- Git op afstand ophalen, mergen en pushen.

Keycord toont deze opdracht wanneer hosttoegang ontbreekt:

```sh
flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord
```

### Smartcardtoegang

Acties met hardwaresleutels hebben PC/SC-toegang nodig in Flatpak-builds.

Keycord toont deze opdracht wanneer smartcardtoegang ontbreekt:

```sh
flatpak override --user --socket=pcsc io.github.noobping.keycord
```

De app vraagt om een herstart nadat smartcardtoegang is ingeschakeld.

## Opmerkingen bij de Host-backend

### Aangepaste host-opdracht

De opdracht voor de Host-backend is configureerbaar op Linux. Keycord splitst die zoals een shell-opdrachtregel, en het geconfigureerde programma moet zich nog steeds gedragen als `pass`, omdat Keycord normale bewerkingen toevoegt zoals show, insert, move, remove, init en import.

Voorbeelden:

```text
pass
/usr/bin/pass
/path/to/custom-pass-wrapper
```

### `pass import`

Op Linux wordt de importpagina gevuld vanuit:

```sh
pass import --list
```

via je geconfigureerde host-opdracht.

Als er geen importeurs worden gedetecteerd, blijft de import-UI ontoegankelijk.

### Herstellen vanuit Git

De actie **Wachtwoordopslag herstellen** is een Host-functie die alleen op Linux beschikbaar is, omdat deze `git clone` uitvoert in de map die je kiest.

## Git-gedrag

### Afhandeling van lokale repositories

Op Linux initialiseert Keycord een Git-repository voor die opslag wanneer Keycord een nieuwe opslag maakt door ontvangers op te slaan in een map die nog geen `.gpg-id` of `.git` heeft.

### Status en synchronisatie van Git op afstand

Op Linux kan Keycord op Git gebaseerde opslagen inspecteren en remotes beheren. Synchronisatie op afstand vereist:

- een Git-repository,
- minstens één remote,
- een uitgecheckte branch,
- geen lokale wijzigingen zonder commit.

Wanneer synchronisatie draait, doet Keycord het volgende:

1. haalt elke remote op met `--prune`,
2. voegt de huidige branch vanuit elke remote samen,
3. pusht `HEAD` terug naar elke remote.

Als de repo niet schoon is, losgekoppeld is of geen eerste commit heeft, stopt Keycord en vertelt het je wat je moet repareren.

### Git-ondertekening en ontgrendeling van privésleutels

Op Linux kunnen workflows van de Integrated-backend vereisen dat een beheerde privésleutel is ontgrendeld voordat Keycord een Git-commit kan ondertekenen die hoort bij een wijziging van een item of ontvanger.

Als de ontgrendelingsprompt wordt gesloten, kan het opslaan doorgaan zonder Git-handtekening.

## Opslagontvangers en gelaagde versleuteling

### Normale afhandeling van ontvangers

Opslagen gebruiken `.gpg-id` voor ontvangers. Keycord accepteert ontvangerwaarden zoals:

- vingerafdrukken,
- sleutelhandles,
- gebruikers-ID's zoals `Alice Example <alice@example.com>`.

### Alle geselecteerde sleutels vereisen

Keycord kan een opslag zo markeren dat elke geselecteerde beheerde sleutel moet zijn ontgrendeld. Dit gebruikt gelaagde versleuteling en voegt Keycord-specifieke metadata toe aan `.gpg-id`.

Gebruik dit alleen wanneer je expliciet Keycord-only gedrag wilt.

Belangrijke waarschuwing:

- andere `pass`-apps kunnen die items niet lezen.

## Door de app beheerde privésleutels

Keycord kan het volgende beheren:

- met wachtwoord beveiligde privésleutels die door de app worden opgeslagen,
- Linux OpenPGP-sleutels met hardwareondersteuning,
- imports van publieke sleutels die op Linux worden gekoppeld aan verbonden hardwaresleutels.

De UI voor opslagsleutels ondersteunt:

- privésleutel genereren,
- hardwaresleutel toevoegen op Linux,
- publieke hardwaresleutel importeren op Linux,
- importeren vanaf het klembord,
- importeren vanuit een bestand.

## Privésleutels synchroniseren met host-GPG

Deze functie is alleen beschikbaar op Linux.

Wanneer deze is ingeschakeld, brengt Keycord eerst zijn lijst met privésleutels in lijn met de privésleutels in host-GPG en blijft ze daarna synchroon houden.

Belangrijke beperkingen:

- hosttoegang moet beschikbaar zijn,
- elke gesynchroniseerde hostsleutel moet met een wachtwoord zijn beveiligd voordat Keycord die kan opslaan,
- de eerste synchronisatie van host naar app kan met wachtwoord beveiligde app-only sleutels verwijderen die ontbreken in de host-keyring,
- latere synchronisatie van app naar host kan sleutels importeren of verwijderen zodat de host overeenkomt met de door de app beheerde set.

Gebruik dit als je één beheerde set softwaresleutels wilt in zowel Keycord als de host-keyring.

## Hardwaresleutels

Keycord ondersteunt verbonden OpenPGP-smartcards en YubiKeys op Linux.

Gebruiksscenario's:

- een verbonden hardwaresleutel direct toevoegen,
- een passend bestand met een publieke hardwaresleutel importeren,
- een export van een publieke sleutel koppelen aan het momenteel verbonden token.

Flatpak-builds hebben smartcardtoegang nodig voor deze workflows.

## Checklist voor probleemoplossing

Als een functie is uitgeschakeld:

1. Controleer of deze alleen voor Host is.
2. Controleer in Flatpak hosttoegang of smartcardtoegang.
3. Controleer of de huidige opslag Git-metadata en remotes heeft.
4. Controleer of de repo niet schoon is of losgekoppeld is.
5. Controleer of vereiste privésleutels aanwezig en ontgrendeld zijn.

## Verder lezen

- [Aan de slag](getting-started.md)
- [Werkstromen](workflows.md)
- [Gebruiksscenario's](use-cases.md)
