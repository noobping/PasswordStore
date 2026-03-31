# Het verhaal van geheimen

Dit is een codegerichte rondgang door hoe Keycord geheime gegevens verplaatst vanaf het maken van een opslag tot het kopieren van een wachtwoord.

## Verhaal 1: Een opslag wordt geboren

De opslagflow begint in [src/store/management.rs](../src/store/management.rs). Wanneer de gebruiker een map kiest, controleert Keycord of die leeg is.

Als de map niet leeg is, behandelt Keycord die als een bestaande opslag en opent het de editor voor opslagsleutels.

Als de map leeg is, opent Keycord via [src/store/recipients_page/mod.rs](../src/store/recipients_page/mod.rs) de versie van de pagina voor het maken van een nieuwe opslag. De modus voor maken zet meteen een autosave in de wachtrij, maar die opslag wordt pas echt zodra er minstens een ontvanger is.

De ontvangerspagina houdt een lijst van geselecteerde ontvangers in het geheugen. Voor het opslaan splitst [src/store/recipients.rs](../src/store/recipients.rs) die lijst in:

- standaardontvangers die in `.gpg-id` horen
- FIDO2-ontvangers die in `.fido-id` horen

Het echte opslagpad zit in [src/store/recipients_page/save.rs](../src/store/recipients_page/save.rs) en [src/backend/integrated/store.rs](../src/backend/integrated/store.rs):

1. Keycord verzamelt de huidige ontvangers en de huidige eis voor privésleutels.
2. `save_store_recipients` zorgt dat de opslagmap bestaat.
3. Eerst wordt elk bestaand item ontsleuteld.
4. Daarna worden de bijgewerkte ontvangerbestanden geschreven.
5. Vervolgens wordt elk item opnieuw versleuteld onder het nieuwe beleid.
6. Als de opslag splinternieuw is, kan Keycord ook Git initialiseren.

Twee details zijn hier belangrijk.

Ten eerste zijn ontvangerbestanden transactioneel. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) schrijft het nieuwe `.gpg-id` en het FIDO2-sidecarbestand, voert de closure voor herencryptie uit en zet de oude bestanden terug als de herencryptie mislukt.

Ten tweede worden ontvangers per pad geerfd. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) zoekt de ontvangers van een item door omhoog te lopen totdat het de dichtstbijzijnde `.gpg-id` vindt. Het "verhaal van een geheim" is dus eigenlijk "vind het dichtstbijzijnde ontvangerbestand en gebruik dan dat beleid".

## Verhaal 2: Een geheim wordt geschreven

Het dialoogvenster voor een nieuw item wordt gebouwd in [src/password/new_item.rs](../src/password/new_item.rs). Het kiest een opslagroot en een label voor het pass-bestand zoals `team/service`.

Wanneer de editor opent in [src/password/page/mod.rs](../src/password/page/mod.rs), vult Keycord het nieuwe bestand eerst met het "sjabloon voor nieuwe wachtwoorden" uit Voorkeuren. [src/password/file/compose.rs](../src/password/file/compose.rs) zet dat sjabloon om in initiële platte tekst waarbij:

- de eerste regel het wachtwoordvak is
- latere regels gestructureerde velden zijn zoals `username:` of `url:`

Terwijl de gebruiker bewerkt, bouwen [src/password/page/editor.rs](../src/password/page/editor.rs) en [src/password/file/compose.rs](../src/password/file/compose.rs) de tekst van het pass-bestand steeds opnieuw in het geheugen op. Keycord versleutelt niet veld voor veld. Het stelt altijd eerst een volledig pass-bestand in platte tekst samen en versleutelt daarna het geheel.

Bij opslaan roept [src/password/page/mod.rs](../src/password/page/mod.rs) code aan in [src/backend/mod.rs](../src/backend/mod.rs), dat doorstuurt naar de actieve backend. Het geïntegreerde opslagpad zit in [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs).

Dat opslagpad doet vier belangrijke dingen:

1. Het bepaalt het definitieve bestandspad voor het label.
2. Het laadt de cryptocontext uit de dichtstbijzijnde ontvangerbestanden.
3. Het versleutelt de platte tekst volgens het opslagbeleid.
4. Het schrijft de ciphertext naar schijf.

De bestandsextensie hoort bij dat beleid. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) en [src/password/entry_files.rs](../src/password/entry_files.rs) kiezen:

- `.gpg` voor opslagen met standaardontvangers
- `.keycord` voor opslagen met FIDO2-ontvangers

Bestaande verouderde bestanden worden nog steeds gerespecteerd, dus een item met FIDO2-ondersteuning kan een ouder `.gpg`-bestand blijven lezen totdat het wordt herschreven.

## Verhaal 3: Met wachtwoord beveiligde sleutel

Dit is het normale pad voor een beheerde sleutel.

De UI voor het genereren van de sleutel zit in [src/store/recipients_page/generate.rs](../src/store/recipients_page/generate.rs). De echte sleutelaanmaak gebeurt in [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs):

1. `generate_ripasso_private_key` maakt een Sequoia-certificaat met een verplichte passphrase.
2. Het serialiseert het geheime sleutelmateriaal.
3. Het importeert dat materiaal meteen terug in de opslag voor beheerde sleutels van Keycord.

Import gebruikt dezelfde opslagmodule. De belangrijke regel wordt afgedwongen in [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs): Keycord weigert een onbeveiligde softwarematige privésleutel te bewaren. Geimporteerde softwaresleutels moeten al met een wachtwoord zijn beveiligd.

Ontgrendelen is sessiegebaseerd. [src/private_key/unlock.rs](../src/private_key/unlock.rs) verzamelt de passphrase, waarna [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs) de opgeslagen sleutel ontsleutelt en het ontgrendelde certificaat cachet in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs).

Wanneer een item wordt gelezen, bouwt [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs) via [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs) een kandidatenlijst op:

- ontvangers voor het item
- de geselecteerde "eigen" vingerafdruk, als die is geconfigureerd
- elke geimporteerde beheerde sleutel

Als de benodigde sleutel nog vergrendeld is, faalt het lezen met een locked-key-fout. De copy- en open-flow vangen die fout af en leiden terug naar de ontgrendeldialoog via [src/clipboard.rs](../src/clipboard.rs) of [src/private_key/unlock.rs](../src/private_key/unlock.rs).

Voor versleuteling bouwt [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) een normale lijst met OpenPGP-ontvangers en versleutelt het het hele pass-bestand in een keer.

## Verhaal 4: Alle sleutels verplichten

Deze optie begint in de UI voor opslagsleutels. [src/store/recipients_page/list.rs](../src/store/recipients_page/list.rs) toont de schakelaar "alle sleutels vereisen" wanneer de opslag normale beheerde sleutels gebruikt.

Het opslaan van die optie maakt geen nieuw bestand. Het voegt metadata toe aan `.gpg-id`. [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs) schrijft:

```text
# keycord-private-key-requirement=all
```

Die ene comment verandert het hele lees- en schrijfpak.

Bij schrijven schakelt [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) over van "elke geselecteerde sleutel mag dit openen" naar gelaagde versleuteling:

1. Versleutel de platte tekst voor de binnenste vereiste ontvanger.
2. Wikkel die ciphertext in een laag `keycord-require-all-private-keys-v1`.
3. Versleutel die ingepakte waarde voor de volgende ontvanger.
4. Herhaal dit totdat elke vereiste sleutel een laag heeft toegevoegd.

Bij lezen draait dezelfde module dat proces voor elke ontvanger in omgekeerde volgorde terug. Als ook maar een vereiste sleutel ontbreekt, incompatibel is of nog vergrendeld is, gaat het geheim niet open.

Er zit nog een extra regel verstopt in [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs): een opslag met alleen FIDO2 en meer dan een FIDO2-ontvanger wordt behandeld als `AllManagedKeys`, zelfs als de comment ontbreekt. Met andere woorden: "alle sleutels vereist" is expliciet voor normale sleutels en impliciet voor FIDO2-opslagen met meerdere sleutels.

## Verhaal 5: FIDO2-beveiligingssleutel

De flow voor het toevoegen van FIDO2 leeft in [src/store/recipients_page/import.rs](../src/store/recipients_page/import.rs), maar het echte werk gebeurt in [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs).

Wanneer de gebruiker een FIDO2-beveiligingssleutel toevoegt:

1. registreert Keycord een `hmac-secret`-credential tegen de Keycord RP ID
2. leidt het een stabiele ontvanger-id af uit de credential-id
3. slaat het tijdelijk een inschrijvingsrecord in het geheugen op
4. geeft het een ontvangerstring terug zoals `keycord-fido2-recipient-v1=...`

Het formaat van die ontvangerstring staat in [src/fido2_recipient.rs](../src/fido2_recipient.rs). De ontvanger zelf wordt opgeslagen in `.fido-id`, niet in `.gpg-id`.

De tijdelijke cache voor inschrijvingen in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs) is belangrijk. Daardoor kan de eerste save meteen het zojuist gemaakte FIDO2-geheimmateriaal gebruiken zonder de gebruiker te dwingen het direct opnieuw van het apparaat af te leiden. Na een succesvolle save van opslagontvangers wist [src/backend/integrated/store.rs](../src/backend/integrated/store.rs) die pending enrollment-status.

FIDO2-versleuteling voor items werkt anders dan standaard OpenPGP-versleuteling voor items.

Voor het gebruikelijke any-key-pad maken [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) en [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) een any-managed-bundel:

- een willekeurige data-encryptiesleutel versleutelt de payload van het pass-bestand een keer
- elke FIDO2-ontvanger krijgt zijn eigen ingepakte kopie van die sleutel
- standaard OpenPGP-ontvangers kunnen ook een ingepakte kopie van dezelfde sleutel krijgen

Dat betekent dat de payload een keer wordt versleuteld, maar dat meerdere ontvangerwrappers ernaar verwijzen.

Voor herschrijvingen probeert [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) bestaande ingepakte ontvangers waar mogelijk te behouden. Daarom dwingt het toevoegen of verwijderen van een FIDO2-sleutel niet altijd een volledige herbouw van elke FIDO2-wrapper af.

Voor het pad waarbij alle sleutels vereist zijn, gebruikt FIDO2 directe vereiste lagen in plaats van de any-managed-bundel.

Ontgrendelen is ook sessiegebaseerd. [src/private_key/unlock.rs](../src/private_key/unlock.rs) kan om een FIDO2-PIN vragen, waarna [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) het apparaat valideert en de PIN cachet in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs).

De extra begeleidingsdialoog in [src/store/recipients_page/guide.rs](../src/store/recipients_page/guide.rs) bestaat om een echte reden: wanneer je nog een FIDO2-ontvanger toevoegt aan een bestaande FIDO2-opslag, moet Keycord de oude items nog steeds ontsleutelen voordat het ze opnieuw kan inpakken voor de nieuwe set sleutels. Daarom kan het vragen om een beveiligingssleutel die al met de opslag werkt.

## Verhaal 6: Een geheim wordt geopend

Het openen van een wachtwoordinvoer begint in [src/password/page/mod.rs](../src/password/page/mod.rs). De pagina toont een laadstatus en roept daarna `read_password_entry_with_progress` aan.

Het geïntegreerde leespad in [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs) splitst op basis van de eis voor privésleutels:

- `AnyManagedKey`: probeer kandidaten totdat er een ontsleutelt
- `AllManagedKeys`: vereis elke geselecteerde sleutel in volgorde

De cryptocontext komt uit [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs). De kandidatenlijst en ontvangermetadata komen uit [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs).

Als het item opent, gaat het pass-bestand in platte tekst terug naar de gestructureerde editor.

Als de sleutel vergrendeld is, geeft Keycord een getypeerde fout door vanuit [src/backend/errors.rs](../src/backend/errors.rs), zodat de UI de ontbrekende ontgrendelstap kan vragen in plaats van alleen te falen.

## Verhaal 7: Het wachtwoord kopieren

De kopieerknop op elke wachtwoordrij wordt gekoppeld in [src/password/list/row.rs](../src/password/list/row.rs). Die roept [src/clipboard.rs](../src/clipboard.rs) aan.

Vanaf daar is het verhaal kort:

1. Als de geïntegreerde backend actief is, leest Keycord alleen de eerste regel van het item via `read_password_line`.
2. Als dat lezen mislukt omdat de sleutel vergrendeld is, zoekt Keycord de voorkeursleutel op en toont het de ontgrendeldialoog.
3. Als het lezen lukt, schrijft Keycord de eerste regel naar het klembord van het systeem en toont het knopfeedback.

Het belangrijke detail is dat kopieren nog steeds een ontsleuteloperatie is. Het wachtwoord wordt nergens anders in de app als kant-en-klare platte tekst voor kopieren gecachet. Keycord gaat opnieuw door hetzelfde leespad, neemt de eerste regel en geeft die tekst aan het klembord.

Als de Host-backend actief is, neemt [src/clipboard.rs](../src/clipboard.rs) een andere route en roept het `pass -c` aan. De rest van deze handleiding volgt het geïntegreerde pad, omdat daar het beheer van opslagsleutels, gelaagde versleuteling en FIDO2-gedrag leeft.
