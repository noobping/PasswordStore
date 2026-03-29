# Zoekgids

Keycord ondersteunt drie zoekmodi:

- gewoon zoeken op label,
- regex-zoeken met `reg`,
- gestructureerd, veldbewust zoeken met `find`.

Ongeldige `find`- of `reg`-zoekopdrachten vallen niet terug op gewoon zoeken.

## Snel overzicht

| Modus | Waarin wordt gezocht | Voorbeeld |
| --- | --- | --- |
| Platte tekst | Alleen itemlabels | `github` |
| `reg` | Labels plus geïndexeerde veldcorpus | `reg:(?i)^work/.+github$` |
| `find` | Gestructureerde velden en zoekpredicaten | `find url contains github` |

## Wat telt als doorzoekbare gegevens

### Gewoon zoeken

Gewoon zoeken controleert alleen het zichtbare label, zoals:

```text
work/alice/github
```

Er wordt niet in veldwaarden gezocht.

### Regex-zoeken

`reg`-zoekopdrachten gebruiken reguliere expressies en matchen:

- labels,
- plus een doorzoekbare veldcorpus die is opgebouwd uit geïndexeerde gestructureerde velden.

Dat betekent dat een regex `email: alice@example.com` kan matchen, zelfs als het label zelf die waarde niet bevat.

### Gestructureerd zoeken met `find`

`find`-zoekopdrachten werken op gestructureerde zoekvelden:

- `username` plus de aliassen `user` en `login`,
- elk ander `key: value`-veld in het pass-bestand,
- het predicaat `otp`,
- het predicaat `weak password`.

Belangrijke beperkingen:

- `otpauth` wordt niet behandeld als een normaal doorzoekbaar veld.
- Gebruik `find otp` in plaats van direct op `otpauth` te proberen zoeken.
- Regels zonder gestructureerde vorm `key: value` zijn niet als veld doorzoekbaar.

## Gewoon zoeken

Gebruik gewoon zoeken wanneer je alleen op pad of naam wilt zoeken:

```text
github
vpn
work/alice
```

Dit werkt als een labelfilter.

## Regex-zoeken met `reg`

Regex-zoeken begint met `reg:` of `reg `.

Voorbeelden:

```text
reg:(?i)^work/.+github$
reg team/.+service
reg:(?i)email:\s+alice@example\.com
```

Opmerkingen:

- `(?i)` werkt voor hoofdletterongevoelige regex.
- Een ongeldige regex zoals `reg:[` is ongeldig en geeft geen resultaten terug.

## Gestructureerd zoeken met `find`

Gestructureerd zoeken begint met `find:` of `find `.

Voorbeelden:

```text
find user alice
find url contains github
find email is $username
```

### Namen van zoekvelden

Keycord normaliseert deze aliassen voor gebruikersnamen naar hetzelfde veld:

- `username`
- `user`
- `login`

Alles daarbuiten gebruikt, hoofdletterongevoelig, de veldsleutel van het pass-bestand:

```text
find email contains example.com
find url is https://example.com
find "security question" is "first pet"
```

## Operatoren

### Bevat

Deze vormen zijn gelijkwaardig:

```text
find username=noob
find username~=noob
find username contains noob
find user noob
```

### Bevat niet

```text
find url!~gitlab
find url does not contain gitlab
```

### Exacte overeenkomst

```text
find username==alice
find username is alice
```

### Exact niet gelijk

```text
find username!=alice
find username is not alice
```

### Regex-match binnen `find`

```text
find user matches '^Alice$'
find user regex '^Alice$'
```

### Regex komt niet overeen binnen `find`

```text
find user does not match '^Alice$'
find url not regex 'gitlab|github'
```

## Veldreferenties

Je kunt één veld met een ander vergelijken met `$field_name`, maar alleen voor exacte vergelijkingen.

Geldig:

```text
find email is $username
find email is not $user
find "backup email" == $"security question"
```

Ongeldig:

```text
find email contains $username
find email ~= $username
find user regex $email
```

## Booleaanse logica en prioriteit

Keycord ondersteunt:

- `NOT` of `!`
- `AND`, `&&` of `WITH`
- `OR` of `||`
- haakjes

De prioriteit is:

1. `NOT`
2. `AND` / `WITH`
3. `OR`

Voorbeelden:

```text
find username=noob AND url=gitlab OR email==alice@example.com
find (username=noob OR url=gitlab) AND email==alice@example.com
find !username~=alice
find not email is $username
```

## Speciale predicaten

### OTP-predicaat

Matcht items die OTP-gegevens bevatten:

```text
find otp
find otp AND user alice
```

Zoek `otpauth` niet als normaal veld. Dat is ongeldig:

```text
find otpauth contains totp
```

### Predicaat voor zwak wachtwoord

Matcht items waarvan de eerste wachtwoordregel niet voldoet aan de basiscontroles van Keycord:

```text
find weak password
find weak
find weak password AND username==alice
find not weak password
```

## Citeren en escapen

Zet waarden of veldnamen tussen aanhalingstekens wanneer ze spaties of gereserveerde woorden bevatten:

```text
find "security question" is "first pet"
find notes matches 'Personal (OR|AND) Work'
find "matches" is "keyword field"
```

Binnen gequote waarden:

- escape `"` als `\\"` binnen dubbele aanhalingstekens,
- escape `'` als `\\'` binnen enkele aanhalingstekens,
- escape `\` als `\\`.

Voorbeelden:

```text
find:notes=="Personal OR Work \"vault\""
find:notes=='Personal OR Work \'vault\''
```

## Voorbeelden van eenvoudig naar geavanceerd

### Eenvoudige labelzoekopdrachten

```text
github
personal/bank
vpn
```

### Eenvoudige gestructureerde zoekopdrachten

```text
find user alice
find email contains example.com
find url is https://github.com/login
```

### Zoekopdrachten met meerdere voorwaarden

```text
find user alice AND url contains github
find weak password AND url contains gitlab
find otp AND email is $username
```

### Regex-zware zoekopdrachten

```text
reg:(?i)^work/.+github$
find user matches '^(alice|bob)$'
find notes not regex 'deprecated|legacy'
```

### Exacte veldvergelijking

```text
find email is $username
find email is not $username
```

### Zoekopdrachten in auditstijl

```text
find weak password OR otp
find (user alice OR user bob) AND url contains admin
find url contains github AND email contains company.com
```

## Veelvoorkomende ongeldige zoekopdrachten

Deze geven geen resultaten terug totdat ze zijn gecorrigeerd:

```text
find user
find username=
find url does not
find user matches
find email contains $username
find otpauth contains totp
reg:[
```

## Tips

- Gebruik gewoon zoeken voor namen en paden.
- Gebruik `find` voor veldbewuste zoekopdrachten.
- Gebruik `reg` voor regex over labels en geïndexeerde velden.
- Zet veldnamen met spaties tussen aanhalingstekens.
- Gebruik vergelijkingen met `$username` om consistentie tussen velden te controleren.

## Verder lezen

- [Aan de slag](getting-started.md)
- [Werkstromen](workflows.md)
- [Gebruiksscenario's](use-cases.md)
