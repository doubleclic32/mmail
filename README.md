# MMail

Client de messagerie IMAP pour Windows, Linux et Android. Il remplace Outlook
sur le poste dans le cadre de la sortie de Microsoft 365.

Noyau en **Rust**, interface en **Qt 6 / QML**, sous **GNU GPL v3**.

## Ce qu'il cherche à faire

**La boîte n'est pas une cloison.** Tous les clients existants, libres ou
payants, partent du compte : une arborescence par compte, et le déplacement d'un
message vers le dossier d'une autre boîte va du pénible à l'impossible. MMail
part du message.

Le cadrage complet — exigences, décisions actées (les « décisions » numérotées
citées dans le code), pistes écartées et leur motif — est tenu dans un dossier
de projet interne.

## État — 0.2.2

**Lecture, tri, rédaction et envoi.**

- **rédaction** dans une fenêtre à part (un volet sur téléphone) : nouveau
  message, réponse, réponse à tous, transfert — à la manière d'Outlook, avec
  « RE : », « TR : » et le message d'origine sous un bloc « De / Envoyé / À /
  Objet » ; Cc et Cci ; pièces jointes par « Joindre… » ou glisser-déposer,
  celles d'un message transféré reprises d'office ; une capture d'écran se
  colle dans le corps (Ctrl+V) et une image glissée sur le corps s'y insère,
  intégrée au message à l'envoi ; envoi par la soumission
  SMTP (port 465) — celle du serveur de lecture, ou un **serveur d'envoi**
  distinct quand le fournisseur en a un (Orange, Free, SFR) —, dans un fil à part — le compte reste
  utilisable pendant un gros envoi —, copie dans « Éléments envoyés », original
  marqué « répondu » ou « transféré » ; brouillons enregistrés sur le serveur et
  repris d'un double clic ;
- **mise en forme** à la rédaction — gras, italique, souligné, listes à
  puces ou numérotées, liens — envoyée en HTML avec sa version en texte brut ;
  « Mise en forme » décochée, le message part en texte brut ;
- **nom affiché et signature** par compte (menu « Comptes », « Nom et
  signature… »), ajoutée aux nouveaux messages, aux réponses, ou sur demande ;
- **adresses proposées** en tapant un destinataire : celles à qui l'on a
  écrit, puis les expéditeurs connus ;
- **options d'envoi** : importance haute ou basse (`Importance`,
  `X-Priority`), accusé de réception demandé au serveur (DSN, RFC 3461),
  confirmation de lecture demandée au destinataire (`Disposition-Notification-To`) ;
- **envoi différé** : le message attend sur le serveur, dans un dossier
  « Envoi différé » créé au besoin, et part à l'heure dite si MMail est ouvert
  — sur ce poste ou un autre ; sinon à la prochaine ouverture. Le premier MMail
  qui le réserve (CONDSTORE) l'envoie, les autres le laissent : il ne part
  qu'une fois ;
- **drapeau de suivi** (`\Flagged`) posé ou retiré d'un clic en bout de ligne,
  par le menu ou la touche Insertion ; importance des messages reçus signalée
  dans la liste (« ! », « ↓ ») et dans l'en-tête ;
- **répondu et transféré** signalés par une pastille devant l'objet, aux
  préfixes d'Outlook (« RE », « TR »), que la réponse ou le transfert vienne
  de MMail ou d'un autre logiciel — téléphone, webmail (`\Answered`,
  `$Forwarded`) ; « RE TOUS » pour une réponse à tous faite depuis MMail, qui
  pose en plus le mot-clé `$ReplyAll` (aucun drapeau standard ne distingue une
  réponse à tous : faite ailleurs, elle reste « RE ») ;
- **confirmation de lecture** demandée par un expéditeur : proposée, jamais
  envoyée d'office ; la réponse — envoi ou refus — n'est demandée qu'une fois
  (`$MDNSent`) ;
- **nouveau courrier** : le nombre de non-lus des boîtes de réception de tous
  les comptes s'affiche en pastille sur l'icône de MMail dans la barre des
  tâches (Windows ; compteur du lanceur sous Linux) ; la boîte de réception
  d'un compte qu'on ne regarde pas est surveillée en continu (IDLE) ;
- **gros dossiers** : la liste s'affiche dès sa première page (une vingtaine
  de millisecondes pour 40 000 messages), le reste suit par paquets sans figer
  la fenêtre ; de même en sortant d'une recherche ;
- **« Toutes les BàL »** : les boîtes de réception de tous les comptes en une
  seule liste (les 1 000 messages les plus récents), chacun avec son compte ;
  un message s'y lit, s'y répond, s'y déplace comme ailleurs ;
- **mise à jour assistée** : une nouvelle version s'installe maintenant (un
  redémarrage de MMail), à la fermeture, ou jamais — téléchargée et contrôlée
  par MMail, installée sans question (cf. « Données sur le poste ») ;
- **plusieurs comptes dans une seule arborescence**, repliables, avec la
  rubrique **Favoris** au-dessus : on l'alimente en y **glissant un dossier**,
  et l'on réordonne ses favoris de la même façon ; un blanc et un trait
  séparent chaque compte de ce qui le précède ;
- **nouveau sous-dossier** par clic droit sur un dossier : créé sur le
  serveur et abonné, pour que le webmail et Outlook le montrent aussi ; un nom
  vide, déjà pris ou portant le séparateur du serveur est refusé dans le
  dialogue même ;
- **menu « Comptes »** : ajouter un compte — le serveur est trouvé d'après
  l'adresse quand le domaine publie sa configuration —, en ajouter un ou
  plusieurs par un **lien de configuration**, ou en retirer un (voir
  « Configuration d'un compte ») ;
- **menu « ? »** : aide (F1) et « À propos » — versions de MMail, du noyau et
  de Qt ;
- **ascenseurs toujours visibles** dès qu'une colonne a de quoi défiler ;
- **trombone** dans la liste pour un message à pièces jointes : supposé
  d'après les en-têtes (`multipart/mixed`, ou corps entier qui n'est pas du
  texte ; mot-clé `$HasAttachment` du serveur s'il existe), puis constaté à
  l'ouverture du message ;
- **menus contextuels et « Déplacer vers… » au zoom de la colonne** d'où ils
  sont ouverts ;
- **déplacement par glisser-déposer ou par clic droit**, vers un dossier de la
  même boîte ou de n'importe quelle autre boîte connectée ; dialogue
  « Déplacer vers… » avec filtre (Ctrl+Maj+V) ;
- **recherche** (Ctrl+E), comme dans Outlook : le champ propose les dernières
  recherches (dix visibles, ascenseur au-delà), Entrée lance la recherche ; la
  liste des messages ne contient alors que les résultats — dans le dossier
  actif, objet et expéditeur d'après l'index puis le texte entier des messages
  par le serveur ; dans toutes les boîtes (index du poste), chaque résultat avec
  son dossier, un seul choisi à la fois, et l'on revient au dossier de départ
  en sortant de la recherche (Échap). Les résultats se **trient** — plus
  récents, plus anciens, expéditeur, objet (sans ses « RE : »), plus gros,
  dossier dans toutes les boîtes — sur le poste : le tri n'est jamais demandé
  au serveur, pour qui il coûte cher sur un gros dossier ;
- **Supprimer** envoie à la corbeille de la boîte (touche Suppr) ;
- **dossiers de rôle** — brouillons, éléments envoyés, corbeille — désignés
  par le serveur (SPECIAL-USE) ; à défaut (OVH), reconnus à leur nom usuel,
  « Drafts » ou « Brouillons », « Sent » ou « Éléments envoyés »… : le moins
  profond d'abord, puis le nom standard avant sa traduction ;
- **vider les corbeilles et indésirables** de tous les comptes : seuls les
  dossiers que le serveur désigne comme tels (`\Trash`, `\Junk`) sont purgés,
  jamais un dossier sur la foi de son nom ;
- marquage lu / non lu (Ctrl+Q, Ctrl+U) ; un message affiché est marqué lu ;
- **masquage local** des dossiers peu utilisés, réaffichage en un clic ;
- affichage du **message brut** (bouton « Source ») ;
- **copie automatique de la sélection**, comme dans MMdedit : un texte
  sélectionné dans un message part au presse-papier, sauf ce qu'un autre
  logiciel vient d'y déposer, protégé une minute ; clic droit sur le message :
  « Copier », « Tout sélectionner » ; sur une ligne de la liste : copier
  l'adresse de l'expéditeur ou l'objet. Sous Android, ni copie automatique ni
  menu : l'appui long choisit le mot et ouvre la barre d'Android (Copier, Tout
  sélectionner), qu'un toucher ou le défilement referme ;
- **barre d'information** : compte et dossier ouverts, nombre de messages et de
  non-lus, sélection, état de la synchronisation, déplacements en attente ;
- **pièces jointes** mises en valeur sous l'en-tête du message, une ligne par
  pièce : type en couleur (PDF, XLSX, JPG…), nom, taille, « Ouvrir » et
  « Enregistrer sous… » ; clic droit, « Ouvrir avec… » un logiciel choisi, pour
  cette fois ou pour tous les fichiers du même type — retenu sur ce poste
  seulement, revu par « Logiciels d'ouverture… » (menu « Affichage ») ; sans
  choix, le logiciel du système. Sous Windows, un type que le système ne sait
  pas ouvrir fait choisir le logiciel par MMail, qui le retient d'emblée :
  choisi dans la fenêtre de Windows, il lui échappait. Un programme ou un
  script (`.exe`, `.js`, `.bat`…) ne s'ouvre pas depuis MMail, il s'enregistre ;
- **zoom propre à chaque colonne** : Ctrl + molette sur la colonne, ou Ctrl +,
  Ctrl − et Ctrl 0 sur la dernière colonne survolée ; pincement au doigt ;
- **quatre apparences**, celles de MMdedit : classique (Windows 9x), moderne
  (M-Media), système, et MMEDIA, aux couleurs du site mmedia.fr — en-tête en
  bandeau nuit, boutons en pastilles, non-lus en pastilles rouges, dossier et
  message choisis sur un cyan pâle, marqués d'un filet — menu « Affichage » ;
- mot de passe confié au **coffre du système**, jamais écrit par MMail ;
- **synchronisation incrémentale** (QRESYNC) : seul ce qui a changé depuis la
  dernière visite est relu ; le dossier ouvert suit le serveur en temps réel
  (IDLE) — un message arrivé apparaît en quelques secondes —, les compteurs des
  autres dossiers se mettent à jour toutes les deux minutes, ou sur F5. Une
  liste de dizaines de milliers de messages ne se met à jour que des lignes
  qui changent.

### L'agenda

**Agenda** (bouton « Agenda », Ctrl+2 ; Ctrl+1 ramène au courrier) :
les agendas CalDAV des boîtes connectées — SOGo pour une boîte Mailcow, ou tout
serveur CalDAV qui accepte les identifiants de la boîte —, trouvés sans
réglage sur le serveur de la boîte (`/.well-known/caldav`, RFC 6764). Vues jour, semaine et mois à la
manière d'Outlook ; un agenda se coche ou se décoche ; un clic sur un
événement en montre la fiche (horaire, lieu, description, agenda). Les
répétitions, leurs exceptions et les occurrences déplacées sont calculées sur
le poste, dans le fuseau du poste ; les fuseaux nommés à la manière de Windows
(« Romance Standard Time ») sont reconnus. L'agenda reste consultable hors
connexion ; il se synchronise au démarrage, tous les quarts d'heure, et sur
« Actualiser ».

**Créer, modifier, supprimer** : « Nouvel événement », ou un double clic sur un
créneau de la semaine, du jour ou sur une case du mois ; titre, lieu, horaires
ou journée entière, agenda, répétition (chaque jour, jour ouvré, semaine, mois,
année, jusqu'à une date), rappel, description. Dans une série, la fiche
demande s'il s'agit de cette occurrence ou de toute la série. Un événement
modifié ailleurs entre-temps n'est pas écrasé : l'agenda est relu et la
modification est à refaire. Les propriétés que MMail ne gère pas (celles d'un
autre logiciel) sont conservées.

**Invitations** : un courriel d'invitation affiche un bandeau — objet, horaire,
lieu, organisateur — avec « Accepter », « Provisoire », « Refuser » ; la réunion
entre dans l'agenda (un refus l'en retire) et l'organisateur reçoit la réponse.
Une réponse reçue s'inscrit d'elle-même dans l'agenda de l'organisateur, une
annulation propose de retirer la réunion. Dans l'agenda, une réunion où l'on
est invité se répond depuis sa fiche ; « Participants » fait d'un événement une
réunion. C'est le serveur d'agenda qui envoie les courriels d'invitation, de
mise à jour, d'annulation et de réponse (SOGo le fait ; un serveur qui ne le
fait pas laisse ces boutons inactifs).

**Rappels** : ceux des agendas affichés, posés depuis MMail ou ailleurs,
s'affichent à l'heure dite tant que MMail est ouvert, avec « Ignorer » et
« Répéter dans… » ; la fenêtre clignote dans la barre des tâches.

### Le déplacement entre boîtes

Aucun protocole ne déplace un message d'une boîte vers une autre. MMail le lit
en entier à la source, le **copie sur disque**, le dépose dans la cible avec ses
drapeaux et sa date de réception d'origine, et **ne le retire de la source
qu'une fois le dépôt accepté**. Une coupure au milieu ne perd rien : le
déplacement reprend à la connexion suivante, et la cible est d'abord interrogée
par `Message-ID` pour ne pas créer de doublon.

## Configuration d'un compte

### Configuration automatique

À l'ajout d'un compte, dès que l'adresse est saisie, MMail cherche le serveur
IMAP dans le document de configuration que publie le domaine, au format
« autoconfig » de Thunderbird. Il interroge, dans l'ordre :

1. `https://autoconfig.<domaine>/mail/config-v1.1.xml?emailaddress=<adresse>`
2. `https://<domaine>/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=<adresse>`

Il retient le premier serveur `imap` en `SSL` sur le port 993 dont
l'identifiant est l'adresse elle-même. Aucun annuaire tiers n'est interrogé.
Mailcow sert ce document : il suffit que `autoconfig.<domaine>` désigne le
serveur. Un serveur saisi à la main n'est jamais remplacé.

### Serveur d'envoi

L'envoi passe par défaut par le serveur de lecture, sur le port 465. Quand le
fournisseur reçoit l'envoi ailleurs — Orange lit sur `imap.orange.fr` et
reçoit sur `smtp.orange.fr` —, le compte porte un **serveur d'envoi** : champ
« Serveur d'envoi » à l'ajout d'un compte, ou « Modifier le compte… » pour un
compte déjà présent. Même port (465, TLS d'emblée), mêmes
identifiants que la lecture ; laissé vide, c'est de nouveau le serveur de
lecture. Il vaut dès l'envoi suivant, sans reconnexion, et suit la sauvegarde
de la configuration.

### Modifier un compte

« Comptes › Modifier un compte », ou « Modifier ce compte… » au clic droit sur
le compte, rouvre la fenêtre de l'ajout pour un compte présent : serveur de
lecture, serveur d'envoi, mot de passe (laissé vide, il ne change pas), et de
là « Nom et signature… ». L'adresse ne change pas : c'est elle qui désigne le
compte, son index et ses réglages. Un nouveau serveur de lecture reprend le
mot de passe mémorisé, qui change de clé dans le coffre avec lui ; s'il est
injoignable, la fenêtre revient avec le motif.

### Comptes Microsoft (Office 365, Outlook.com)

Office 365 n'accepte plus de mot de passe en IMAP : « Comptes › Ajouter un
compte Microsoft (Office 365, Outlook.com)… » ouvre la page de connexion de
Microsoft dans le navigateur. La personne s'y connecte — mot de passe, double
authentification, tout se passe chez Microsoft — et consent ; MMail reçoit le
retour sur une écoute locale (`http://localhost`, flux « code
d'autorisation » avec PKCE) et ne voit jamais le mot de passe. Il garde, dans
le coffre du système, le **jeton de rafraîchissement** que Microsoft lui
délivre, à la place du mot de passe ; les jetons d'accès, valables une heure,
ne vivent qu'en mémoire.

- **Une seule application pour tous** : MMail est une application publique
  (sans secret), ouverte à toutes les organisations et aux comptes
  personnels. Rien n'est à inscrire chez le client.
- **Consentement** : l'utilisateur consent lui-même si son organisation le
  permet. Sinon, MMail affiche un lien de consentement à transmettre à
  l'administrateur du Microsoft 365 de l'organisation
  (`https://login.microsoftonline.com/organizations/adminconsent?client_id=…`) :
  ouvert une fois, il autorise MMail pour tous les utilisateurs.
- **Lecture** en IMAPS sur `outlook.office365.com` (`AUTHENTICATE XOAUTH2`) ;
  **envoi par Microsoft Graph** (`Mail.Send`) et non par SMTP, coupé par
  défaut dans Office 365. Microsoft range lui-même la copie dans « Éléments
  envoyés ». Un message trop lourd pour une seule requête Graph (4 Mo) passe
  par un brouillon (`Mail.ReadWrite`) : MMail le crée sans ses plus grosses
  pièces jointes, les y ajoute une à une — par morceaux au-delà de 3 Mo —,
  puis l'envoie. La limite est alors celle de la boîte : 35 Mo par message
  par défaut dans Office 365, 150 Mo par pièce jointe au plus.
- « Modifier le compte… » n'y propose ni serveur ni mot de passe, mais « Se
  reconnecter à Microsoft… ».

Limites : IMAP doit être permis pour la boîte (c'est le réglage par défaut
d'Office 365) ; le texte et les images du corps d'un message tiennent en
3 Mo — les pièces jointes, à part, n'y comptent pas ; pas d'accusé de remise ; pas d'agenda (Office 365 ne parle pas
CalDAV) ; la sauvegarde de la configuration garde le compte, mais pas son
jeton — il se reconnecte à Microsoft sur le poste où elle est remise — et ne
se chiffre que par les comptes à mot de passe.

### Lien de configuration

Un administrateur peut préparer un ou plusieurs comptes, mot de passe compris,
derrière une adresse HTTPS **à usage unique**. La personne la colle dans
« Comptes › Ajouter avec un lien de configuration… », sans rien saisir
d'autre ; les mots de passe vont au coffre du système sans être affichés.

Ce que MMail attend du serveur qui sert le lien :

- MMail l'interroge en **`POST`**, sans corps, avec `Accept: application/json`,
  et ne suit aucune redirection ;
- `200` : le document ci-dessous, que le serveur **détruit en le remettant** ;
  `404` ou `410` : lien inconnu, déjà utilisé ou expiré ;
- un `GET` — la même adresse ouverte dans un navigateur — ne doit pas le
  consommer ; le serveur peut y afficher le mode d'emploi.

```json
{
  "mmail": 1,
  "comptes": [
    { "adresse": "nom@exemple.fr", "hote": "mail.exemple.fr", "motDePasse": "…" },
    { "adresse": "nom@orange.fr", "hote": "imap.orange.fr", "hoteEnvoi": "smtp.orange.fr", "motDePasse": "…" }
  ]
}
```

`port`, facultatif, ne peut valoir que 993. `hoteEnvoi`, facultatif, désigne le
serveur d'envoi (port 465) ; absent, celui d'un compte déjà présent est
conservé, celui d'un compte nouveau est le serveur de lecture. Un compte incomplet est écarté, et
signalé ; les autres sont ajoutés. Comme pour une saisie à la main, un mot de
passe n'est confié au coffre qu'une fois accepté par le serveur.

### Sauvegarde de la configuration

MMail sauvegarde sa configuration hors du poste : les comptes et leurs mots de
passe, les noms et signatures avec leurs images, les dossiers favoris, masqués
ou repliés, les agendas affichés, l'apparence et les réglages. Restent sur le
poste la position de la fenêtre et les logiciels choisis pour les pièces
jointes. La sauvegarde se refait d'elle-même, chaque minute où la
configuration a changé ; « Comptes › Sauvegarder la configuration » la fait
sur-le-champ.

Elle est **chiffrée sur le poste**, par une clé tirée de l'adresse et du mot de
passe de chaque compte (PBKDF2-HMAC-SHA256, 600 000 tours) : chaque compte en
porte une copie (AES-256-GCM), que le service qui la range ne peut pas lire.
D'où la restauration sans code à garder : sur un poste neuf, dès que le premier
compte est ajouté — à la main ou par un lien de configuration —, MMail propose
de reprendre la configuration sauvegardée. « Comptes › Restaurer une
configuration sauvegardée… » le fait à tout moment. Un changement de mot de
passe rend illisibles les copies antérieures ; la suivante se fait avec le
nouveau. Si les mots de passe ne sont pas gardés sur le poste, ils ne sont pas
sauvegardés non plus.

Chaque poste range sa copie sous son propre numéro : deux postes ne
s'écrasent pas, et la restauration propose le poste à reprendre.

Ce que MMail attend du service (`https://agent.mmedia.fr/mmail/sauvegardes`,
celui de M-Media) — `<identifiant>` : 64 chiffres hexadécimaux tirés de la
clé, qui n'en livrent rien ; `<poste>` : 32 chiffres hexadécimaux :

- `PUT <service>/<identifiant>/<poste>` : dépose une copie (16 Mo au plus) ;
- `GET <service>/<identifiant>` : `[{"poste", "date", "taille"}]`, la dernière
  copie de chaque poste, `date` en secondes depuis 1970 ;
- `GET <service>/<identifiant>/<poste>` : la dernière copie du poste.

Une copie : `MMS1`, 12 octets d'aléa, le document JSON chiffré et son sceau ;
l'identifiant est authentifié avec. Le module `sauvegarde` du noyau décrit le
document.

## Données sur le poste

Le serveur reste la référence : MMail garde sur le poste un **index** (dossiers,
en-têtes des messages, favoris, ordre des comptes, événements des agendas) et,
pour lire hors connexion, les **messages des 31 derniers jours**, entiers, en
fichiers `.eml`.
La boîte de réception et chaque dossier ouvert se préchargent en arrière-plan ;
au-delà de la fenêtre, un message est relu sur le serveur.

Un message en cours de rédaction est gardé sur le poste toutes les 10 secondes
et à la fermeture de MMail, et enregistré en brouillon sur le serveur 30
secondes après la première frappe, puis chaque minute tant qu'il change : le
brouillon paraît dans son dossier pendant la rédaction. Si MMail se ferme avant son envoi (mise à
jour, arrêt du poste), il est proposé à la reprise au démarrage suivant.

| | Windows | Linux |
|---|---|---|
| Profil (index, messages gardés, rédactions en cours, images des signatures) | `%APPDATA%\M-Media\MMail` | `~/.local/share/M-Media/MMail` |
| Réglages (comptes, signatures, apparence) | `HKCU\Software\M-Media\MMail` | `~/.config/M-Media/MMail.conf` |
| Mots de passe | Gestionnaire d'identification | Secret Service |

**Présentation au serveur.** Après la connexion, MMail se présente au serveur
IMAP qui l'accepte (commande `ID`, RFC 2971) : `name` (MMail), `version`, `os`
(Windows, Linux, Android, macOS), `os-version` (le système et sa version, tels
que Qt les nomme) et `x-poste` (le nom de l'ordinateur ; « Android » sur un
téléphone). Le serveur peut l'inscrire à son journal : c'est ce qui permet de
compter les postes où MMail fonctionne.

**Serveur partagé (RDS).** Deux réglages valent pour toute la machine, dans
`HKLM\Software\M-Media\MMail` (ou `/etc/xdg/M-Media/MMail.conf`), ou par
variable d'environnement :

| Valeur | Variable | Effet |
|---|---|---|
| `DossierProfil` | `MMAIL_DOSSIER_PROFIL` | emplacement du profil, variables développées — par exemple `D:\MMail\%USERNAME%`. Un profil itinérant ou redirigé partirait sur le réseau, où l'index SQLite se comporte mal |
| `JoursCache` | `MMAIL_JOURS_CACHE` | jours de messages gardés sur le poste (31 par défaut ; 0 : aucun) |
| `AvisVersion` | `MMAIL_AVIS_VERSION` | 0 : ne pas signaler les nouvelles versions (parc dont l'administrateur déploie les mises à jour) |

Un seul MMail s'ouvre par profil : un second lancement ramène au premier plan
la fenêtre déjà ouverte, puis s'arrête.

La fenêtre retrouve sa position, sa taille et son état agrandi. Une position
tombée hors de tout écran (écran débranché, autre bureau) est remise au centre
de l'écran principal au démarrage ; à tout moment, le clic droit sur l'icône de
MMail dans la barre des tâches propose **« Ramener la fenêtre »** (`mmail
--ramener`).

MMail signale une nouvelle version par un bandeau, après avoir demandé à
l'API de GitHub la dernière publication du dépôt — au démarrage, puis une fois
par jour. Rien d'autre n'est envoyé ; `AvisVersion` à 0 le coupe.

**Mise à jour assistée.** Le bandeau propose de l'installer **maintenant**, **à
la fermeture** de MMail, ou **jamais** :

- *Maintenant* : le paquet se télécharge, son empreinte est contrôlée contre le
  fichier `SHA256SUMS` de la publication, puis le bandeau invite à
  **redémarrer MMail** — un clic : MMail se ferme, la mise à jour s'installe
  sans question et MMail se rouvre. Sans ce clic, elle s'installe à la
  fermeture.
- *À la fermeture* : même téléchargement, en silence ; l'installation suit la
  fermeture de MMail. Pas pendant un arrêt ou une déconnexion de la session :
  elle attend alors la fermeture suivante.
- *Jamais* : cette version n'est plus proposée ; son lien reste dans « À propos
  de MMail ». Une version plus récente se propose de nouveau.

Sous Windows, l'installeur est lancé sans question (`/VERYSILENT`, ou
`/SILENT` avec sa barre d'avancement quand MMail doit se rouvrir) et attend la
fin de MMail avant de remplacer ses fichiers. Une installation pour tous les
utilisateurs (dans Program Files) demande l'autorisation d'un administrateur :
elle n'est proposée qu'à un compte qui peut la donner — sinon, comme sous
Android ou pour un programme compilé sur place, le bandeau ouvre la page de la
version. En AppImage, le fichier est remplacé sur place, sous son nom : ses
raccourcis restent bons. Les rédactions ouvertes au redémarrage sont gardées et
reproposées.

## Limites connues

- Connexion en **IMAPS (port 993)** seulement ; pas de STARTTLS ni d'OAuth2.
- Le rendu HTML est celui du texte riche de Qt : pas de mise en page par
  feuille de style élaborée. Les tableaux de mise en page à une colonne des
  lettres d'information sont ramenés à des blocs, et une image plus large que
  la colonne est réduite ; les images SVG ne s'affichent pas.
- Sous Android, les pièces jointes s'ouvrent mais ne s'enregistrent pas
  ailleurs ; leur ouverture n'a pas été éprouvée sur un téléphone.
- Hors connexion, un message ouvert reste non lu sur le serveur ; pas de
  filtres.
- Agenda : une écriture exige la connexion ; les rappels
  ne s'affichent que MMail ouvert (pas de notification du système, ni sous
  Android en arrière-plan). Les tâches (VTODO) ne s'affichent pas.
- L'envoi se fait sur le port 465 seulement (TLS d'emblée), avec les
  identifiants de la lecture ; pas de soumission en STARTTLS sur le port 587.
- Sous Android, les autorités de certification sont celles de Mozilla,
  embarquées dans l'application — et non celles du téléphone.

## Construction

```sh
cmake -S . -B _build -DCMAKE_BUILD_TYPE=Release
cmake --build _build
QT_QPA_PLATFORM=offscreen _build/mmail --smoke
```

Qt 6.4 au minimum, Rust stable, `libsecret-1-dev` sous Linux. Les paquets —
installeur Windows, AppImage, APK — sont produits par l'intégration continue
(`.github/workflows/socle.yml`).

Épreuves contre un vrai serveur, ignorées par défaut (identifiants fournis par
l'environnement, cf. `core/src/essais_serveur.rs`) :

```sh
cargo test --manifest-path core/Cargo.toml -- --ignored --test-threads=1
```

## Licence

GNU General Public License version 3 ou ultérieure — texte intégral dans
[`LICENSE`](LICENSE).
