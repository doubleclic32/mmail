// SPDX-License-Identifier: GPL-3.0-or-later
//! L'objet que l'interface pilote : les comptes du profil, leur arborescence
//! commune, le dossier ouvert et ses messages, le tri entre dossiers et entre
//! boîtes.
//!
//! **Le réseau ne passe jamais par le fil de l'interface.** Chaque compte
//! connecté a son propre fil de travail, qui possède sa connexion IMAP et sa
//! propre connexion à l'index local. L'interface lui adresse des commandes et
//! reçoit les résultats par signaux, remis sur le fil Qt par
//! `CxxQtThread::queue`. Un serveur lent, un dossier volumineux ou une coupure
//! ne figent donc pas la fenêtre, et un compte injoignable ne retient pas les
//! autres.
//!
//! Trois règles tiennent l'interface à jour sans lui faire subir le passé :
//!
//! - **une commande devenue caduque avant d'être traitée est abandonnée** : si
//!   l'on clique trois dossiers pendant qu'un premier se charge, seul le dernier
//!   est lu ; un message demandé puis délaissé n'est pas téléchargé. Un
//!   déplacement ou un marquage, eux, ne sont jamais abandonnés ;
//! - **chaque session porte un numéro de génération** : les résultats d'une
//!   session fermée ou remplacée peuvent encore arriver, ils sont ignorés ;
//! - **un fil sans commande veille** : toutes les deux minutes, il relit les
//!   compteurs de ses dossiers et resynchronise le dossier ouvert.
//!
//! Les lectures de l'index (`arborescence`, `messages`) restent synchrones :
//! elles sont locales, et l'index est en WAL, qui laisse lire pendant que les
//! fils de travail écrivent.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::cache::Cache;
use crate::categories::{self, Catalogue, Categorie, Changement};
use crate::deplacement::{self, Rapport};
use crate::imap::{Attente, Client, Erreur};
use crate::magasin::{DossierLocal, Magasin, MessageLocal};
use crate::synchro::{self, assurer_selection, Echec};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Vrai tant qu'une opération demandée par l'utilisateur est en cours.
        #[qproperty(bool, occupe)]
        /// Dernier message d'erreur, vide s'il n'y en a pas eu.
        #[qproperty(QString, erreur)]
        /// Augmente à chaque changement de l'arborescence ou des compteurs :
        /// l'interface relit alors `arborescence()`.
        #[qproperty(i32, revision)]
        /// Compte et chemin du dossier ouvert (0 et vide s'il n'y en a pas).
        #[qproperty(i32, compte_courant, cxx_name = "compteCourant")]
        #[qproperty(QString, dossier_courant, cxx_name = "dossierCourant")]
        /// Déplacements entre boîtes inscrits et pas encore soldés.
        #[qproperty(i32, en_attente, cxx_name = "enAttente")]
        /// Vrai pendant une synchronisation des agendas.
        #[qproperty(bool, agenda_occupe, cxx_name = "agendaOccupe")]
        /// Marquage « lu » d'un message affiché : 0 à l'affichage, N secondes
        /// après (`marquageLuDiffere`), -1 jamais — à la main seulement
        /// (0.6.9, demande de Gwenaëlle).
        #[qproperty(i32, delai_lu, cxx_name = "delaiLu")]
        /// Augmente à chaque changement d'un catalogue de catégories (0.6.15) :
        /// l'interface relit alors `categories()`.
        #[qproperty(i32, revision_categories, cxx_name = "revisionCategories")]
        type Boite = super::BoiteRust;

        /// Ouvre l'index local du profil. Local et immédiat ; à appeler avant
        /// tout le reste.
        #[qinvokable]
        #[cxx_name = "ouvrirProfil"]
        fn ouvrir_profil(self: Pin<&mut Boite>, chemin: &QString) -> bool;

        /// Comptes du profil, en JSON : `[{id, adresse, hote, hoteEnvoi, auth,
        /// etat}]` — `hoteEnvoi` vide : l'envoi passe par le serveur de
        /// lecture ; `auth` : `microsoft` pour un compte Microsoft, `google`
        /// pour un compte Google, vide pour un mot de passe.
        #[qinvokable]
        #[cxx_name = "comptes"]
        fn comptes(&self) -> QString;

        /// Connecte un compte — nouveau ou déjà connu — en arrière-plan.
        /// Issue : `connecte`, ou `echec` avec l'étape « connexion » ou
        /// « identifiants ». Un compte nouveau dont la première connexion
        /// échoue n'est pas conservé.
        #[qinvokable]
        #[cxx_name = "connecter"]
        fn connecter(
            self: Pin<&mut Boite>,
            hote: &QString,
            adresse: &QString,
            mot_de_passe: &QString,
        ) -> bool;

        /// Pose le serveur d'envoi (SMTP, port 465) d'un compte déjà
        /// enregistré ; vide, c'est le serveur de lecture. Pris en compte dès
        /// le prochain envoi, sans reconnexion.
        #[qinvokable]
        #[cxx_name = "poserServeurEnvoi"]
        fn poser_serveur_envoi(self: Pin<&mut Boite>, adresse: &QString, hote: &QString) -> bool;

        /// Vrai si MMail a un identifiant d'application Microsoft : sans lui,
        /// pas de compte Microsoft.
        #[qinvokable]
        #[cxx_name = "microsoftDisponible"]
        fn microsoft_disponible(&self) -> bool;

        /// Lien que l'administrateur d'une organisation ouvre une fois pour
        /// autoriser MMail à tous ses utilisateurs.
        #[qinvokable]
        #[cxx_name = "consentementMicrosoft"]
        fn consentement_microsoft(&self) -> QString;

        /// Connexion d'un compte Microsoft (Office 365, Outlook.com) : rend
        /// l'adresse de la page de connexion de Microsoft, à ouvrir dans le
        /// navigateur, et attend son retour en arrière-plan ; vide si rien n'a
        /// pu se préparer (cf. `erreur`). `indice` : l'adresse d'un compte à
        /// reconnecter, vide pour un nouveau. Issue : `microsoftConnecte`, ou
        /// `microsoftRefuse`.
        #[qinvokable]
        #[cxx_name = "connecterMicrosoft"]
        fn connecter_microsoft(self: Pin<&mut Boite>, indice: &QString) -> QString;

        /// Sous Android : l'adresse de retour que la page de connexion intégrée
        /// a interceptée après la connexion Microsoft. Rend faux si elle n'est
        /// pas celle attendue. Issue : `microsoftConnecte`, ou `microsoftRefuse`.
        #[qinvokable]
        #[cxx_name = "terminerMicrosoft"]
        fn terminer_microsoft(self: Pin<&mut Boite>, url: &QString) -> bool;

        /// Vrai si MMail a un client Google : sans lui, pas de compte Google.
        #[qinvokable]
        #[cxx_name = "googleDisponible"]
        fn google_disponible(&self) -> bool;

        /// Vrai si `url` (`file:///…`) désigne un dossier présent : le
        /// dernier dossier d'enregistrement n'est proposé que s'il existe.
        #[qinvokable]
        #[cxx_name = "dossierExiste"]
        fn dossier_existe(&self, url: &QString) -> bool;

        /// Vrai si l'ajout d'un compte Google s'affiche : l'application Google
        /// de MMail est publiée, ou `MMAIL_GOOGLE_ESSAI` est posée.
        #[qinvokable]
        #[cxx_name = "googleAjoutOuvert"]
        fn google_ajout_ouvert(&self) -> bool;

        /// Connexion d'un compte Google (Gmail) : comme `connecterMicrosoft`.
        /// Issue : `googleConnecte`, ou `googleRefuse`.
        #[qinvokable]
        #[cxx_name = "connecterGoogle"]
        fn connecter_google(self: Pin<&mut Boite>, indice: &QString) -> QString;

        /// Ferme la session d'un compte ; son index reste consultable.
        #[qinvokable]
        #[cxx_name = "deconnecter"]
        fn deconnecter(self: Pin<&mut Boite>, compte: i32);

        /// Retire un compte du profil, avec son index local. Rien n'est touché
        /// sur le serveur.
        #[qinvokable]
        #[cxx_name = "retirerCompte"]
        fn retirer_compte(self: Pin<&mut Boite>, compte: i32);

        /// Arborescence commune, en JSON : la rubrique Favoris, puis chaque
        /// compte suivi de ses dossiers. Les dossiers masqués n'y figurent que
        /// si `masques` est vrai.
        #[qinvokable]
        #[cxx_name = "arborescence"]
        fn arborescence(&self, masques: bool) -> QString;

        /// Dossiers pouvant recevoir un message, tous comptes connectés
        /// confondus, en JSON : la liste du dialogue « Déplacer vers… ».
        #[qinvokable]
        #[cxx_name = "dossiersCibles"]
        fn dossiers_cibles(&self) -> QString;

        #[qinvokable]
        #[cxx_name = "replierCompte"]
        fn replier_compte(self: Pin<&mut Boite>, compte: i32, replie: bool);

        /// Place un compte juste avant un autre dans l'arborescence, ou à la
        /// fin si `avant` vaut 0. L'ordre est local au poste.
        #[qinvokable]
        #[cxx_name = "placerCompte"]
        fn placer_compte(self: Pin<&mut Boite>, compte: i32, avant: i32);

        /// Monte (`sens` négatif) ou descend un compte d'une place.
        #[qinvokable]
        #[cxx_name = "decalerCompte"]
        fn decaler_compte(self: Pin<&mut Boite>, compte: i32, sens: i32);

        /// Masque un dossier dans l'arborescence, ou le réaffiche (décision 4).
        #[qinvokable]
        #[cxx_name = "masquerDossier"]
        fn masquer_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString, masque: bool);

        /// Replie ou déplie les sous-dossiers d'un dossier ; réglage local.
        #[qinvokable]
        #[cxx_name = "replierDossier"]
        fn replier_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString, replie: bool);

        /// Épingle un dossier dans la rubrique Favoris, ou l'en retire
        /// (décision 3).
        #[qinvokable]
        #[cxx_name = "epinglerDossier"]
        fn epingler_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString, favori: bool);

        /// Place un dossier dans la rubrique Favoris, avant le favori désigné
        /// par `compte_avant` et `chemin_avant`, ou à la fin si `chemin_avant`
        /// est vide. C'est ce que fait le glisser-déposer d'un dossier.
        #[qinvokable]
        #[cxx_name = "placerFavori"]
        fn placer_favori(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            compte_avant: i32,
            chemin_avant: &QString,
        );

        /// Ouvre un dossier : l'index s'affiche tout de suite, la
        /// synchronisation suit. Issue : `dossierOuvert`.
        #[qinvokable]
        #[cxx_name = "ouvrirDossier"]
        fn ouvrir_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString) -> bool;

        /// Messages du dossier ouvert, en JSON, du plus récent au plus ancien.
        #[qinvokable]
        #[cxx_name = "messages"]
        fn messages(&self) -> QString;

        /// Ce qui a changé dans la liste du dossier ouvert depuis le dernier
        /// appel : `{"retires":[{uid,h}…],"ajoutes":[…],"modifies":[…]}`, dans
        /// la partie déjà envoyée. Rend `{"complet":[…],"suite":…}`, la liste
        /// entière, à la première lecture d'un dossier, si `complet`, ou s'il y
        /// a trop de changements — sa première page seulement, `suite` vrai
        /// s'il en reste.
        #[qinvokable]
        #[cxx_name = "changementsListe"]
        fn changements_liste(self: Pin<&mut Boite>, complet: bool) -> QString;

        /// Page suivante de la liste envoyée : `{"lignes":[…],"suite":…}`,
        /// `nombre` messages au plus, relus dans l'index.
        #[qinvokable]
        #[cxx_name = "suiteListe"]
        fn suite_liste(self: Pin<&mut Boite>, nombre: i32) -> QString;

        /// Messages dont chaque mot de `texte` figure dans l'objet, le nom ou
        /// l'adresse de l'expéditeur, d'après l'index : du dossier ouvert, ou
        /// de tous les comptes si `partout` — chaque ligne porte alors son
        /// compte, son chemin et le nom de son dossier. JSON, du plus récent au
        /// plus ancien, 500 au plus.
        #[qinvokable]
        #[cxx_name = "chercher"]
        fn chercher(&self, texte: &QString, partout: bool) -> QString;

        /// Cherche aussi dans le texte entier des messages du dossier ouvert,
        /// sur le serveur, par une connexion à part (0.6.16). Issue :
        /// `resultatsServeur`, précédé de `rechercheInachevee` si le serveur
        /// n'a pas fini.
        #[qinvokable]
        #[cxx_name = "chercherSurLeServeur"]
        fn chercher_sur_le_serveur(self: Pin<&mut Boite>, texte: &QString) -> bool;

        /// Cherche aussi sur le serveur, dans chaque dossier de chaque compte
        /// connecté, par une connexion à part pour chaque compte (0.6.16) : un
        /// dossier lent passe après les autres. Rend le nombre de dossiers à
        /// interroger ; un texte vide arrête la recherche en cours. Issue :
        /// `resultatsPartout`, par dossier, après `rechercheInachevee` si le
        /// serveur n'a pas fini d'y chercher.
        #[qinvokable]
        #[cxx_name = "chercherPartoutSurLeServeur"]
        fn chercher_partout_sur_le_serveur(self: Pin<&mut Boite>, texte: &QString) -> i32;

        /// Lignes d'un dossier quelconque pour des UID (« 3,5,7 »), comme
        /// `chercher(texte, true)` : compte, chemin et dossier compris.
        #[qinvokable]
        #[cxx_name = "trouvesParUid"]
        fn trouves_par_uid(&self, compte: i32, chemin: &QString, uids: &QString) -> QString;

        /// Lignes du dossier ouvert pour des UID (« 3,5,7 »), en JSON.
        #[qinvokable]
        #[cxx_name = "messagesParUid"]
        fn messages_par_uid(&self, uids: &QString) -> QString;

        /// Demande le corps affichable d'un message du dossier ouvert, et le
        /// marque comme lu. Issue : `corpsRecu`.
        #[qinvokable]
        #[cxx_name = "demanderCorps"]
        fn demander_corps(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Réaffiche un message HTML avec ses images distantes, téléchargées
        /// à cette demande seulement. Issue : `corpsRecu`.
        #[qinvokable]
        #[cxx_name = "afficherImages"]
        fn afficher_images(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Demande le message brut, tel que le serveur le conserve (décision 6).
        /// Issue : `corpsRecu` avec `brut` à vrai.
        #[qinvokable]
        #[cxx_name = "demanderSource"]
        fn demander_source(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Fenêtre d'un message (0.6.16) : son corps (`brut` : sa source), lu
        /// dans son dossier quel que soit le dossier ouvert, sans toucher au
        /// volet de lecture ni au marquage « lu ». Issue : `corpsRecu`.
        #[qinvokable]
        #[cxx_name = "demanderDans"]
        fn demander_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uid: i32, brut: bool) -> bool;

        /// Comme `ouvrirPiece` et `enregistrerPiece`, pour un message de
        /// n'importe quel dossier (fenêtre d'un message, 0.6.16).
        #[qinvokable]
        #[cxx_name = "ouvrirPieceDans"]
        fn ouvrir_piece_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uid: i32, indice: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "enregistrerPieceDans"]
        fn enregistrer_piece_dans(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            uid: i32,
            indice: i32,
            url: &QString,
        ) -> bool;

        /// Comme `marquerLu`, pour des messages de n'importe quel dossier.
        #[qinvokable]
        #[cxx_name = "marquerLuDans"]
        fn marquer_lu_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uids: &QString, lu: bool) -> bool;

        /// Comme `deplacer`, depuis n'importe quel dossier.
        #[qinvokable]
        #[cxx_name = "deplacerDans"]
        fn deplacer_dans(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            uids: &QString,
            compte_cible: i32,
            chemin_cible: &QString,
        ) -> bool;

        /// Comme `preparer`, pour un message de n'importe quel dossier.
        #[qinvokable]
        #[cxx_name = "preparerDans"]
        fn preparer_dans(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            uid: i32,
            mode: &QString,
            jeton: &QString,
            adresses: &QString,
        ) -> bool;

        /// Ouvre une pièce jointe d'un message du dossier ouvert avec le
        /// logiciel du système : elle est d'abord écrite dans un dossier de
        /// travail du profil. Refusé pour un type exécutable. Issue :
        /// `pieceEcrite` avec `ouvrir` à vrai.
        #[qinvokable]
        #[cxx_name = "ouvrirPiece"]
        fn ouvrir_piece(self: Pin<&mut Boite>, uid: i32, indice: i32) -> bool;

        /// Enregistre une pièce jointe à l'emplacement choisi (URL rendue par
        /// le dialogue d'enregistrement). Issue : `pieceEcrite`.
        #[qinvokable]
        #[cxx_name = "enregistrerPiece"]
        fn enregistrer_piece(self: Pin<&mut Boite>, uid: i32, indice: i32, url: &QString) -> bool;

        /// Écrit une pièce jointe pour la joindre à une rédaction — un
        /// programme aussi, puisqu'il ne s'ouvre pas. Issue : `pieceAJoindre`.
        #[qinvokable]
        #[cxx_name = "joindrePiece"]
        fn joindre_piece(self: Pin<&mut Boite>, uid: i32, indice: i32) -> bool;

        /// Marque des messages du dossier ouvert — UID séparés par des
        /// virgules — comme lus ou non lus.
        #[qinvokable]
        #[cxx_name = "marquerLu"]
        fn marquer_lu(self: Pin<&mut Boite>, uids: &QString, lu: bool) -> bool;

        /// Fin du délai annoncé par `marquageLuDiffere` : marque « lu » le
        /// message `uid` s'il est encore celui de l'affichage en attente, et si
        /// l'utilisateur ne l'a pas marqué lui-même entre-temps.
        #[qinvokable]
        #[cxx_name = "marquerLuDiffere"]
        fn marquer_lu_differe(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Pose ou retire le drapeau de suivi sur des messages du dossier
        /// ouvert — UID séparés par des virgules.
        #[qinvokable]
        #[cxx_name = "marquerSuivi"]
        fn marquer_suivi(self: Pin<&mut Boite>, uids: &QString, suivi: bool) -> bool;

        /// Comme `marquerSuivi`, pour des messages d'un dossier donné de
        /// n'importe quel compte (« Toutes les BàL », recherche globale).
        #[qinvokable]
        #[cxx_name = "marquerSuiviDans"]
        fn marquer_suivi_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uids: &QString, suivi: bool) -> bool;

        /// Catégories proposées pour un dossier (0.6.15) — `compte` 0 : le
        /// dossier ouvert —, en JSON : `{compte, boite, serveur, refusees,
        /// creees, max, liste: [{motCle, nom, couleur, origine}]}`. `boite` :
        /// la boîte de réception dont le catalogue vaut pour ce dossier ;
        /// `serveur` : catalogue gardé sur le serveur, sinon sur ce poste ;
        /// `refusees` : le serveur refuse les catégories dans ce dossier.
        #[qinvokable]
        #[cxx_name = "categories"]
        fn categories(&self, compte: i32, chemin: &QString) -> QString;

        /// Pose ou retire une catégorie (mot-clé) sur des messages du dossier
        /// ouvert — UID séparés par des virgules.
        #[qinvokable]
        #[cxx_name = "marquerCategorie"]
        fn marquer_categorie(self: Pin<&mut Boite>, uids: &QString, mot_cle: &QString, present: bool) -> bool;

        /// Comme `marquerCategorie`, pour un dossier donné de n'importe quel
        /// compte (« Toutes les BàL », recherche globale).
        #[qinvokable]
        #[cxx_name = "marquerCategorieDans"]
        fn marquer_categorie_dans(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            uids: &QString,
            mot_cle: &QString,
            present: bool,
        ) -> bool;

        /// Crée (`mot_cle` vide) ou modifie une catégorie du catalogue qui vaut
        /// pour ce dossier. Rend son mot-clé ; vide en cas de refus, motif dans
        /// `erreur`.
        #[qinvokable]
        #[cxx_name = "poserCategorie"]
        fn poser_categorie(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            mot_cle: &QString,
            nom: &QString,
            couleur: &QString,
        ) -> QString;

        /// Retire une catégorie du catalogue qui vaut pour ce dossier ; une
        /// catégorie d'origine reprend son nom et sa couleur. Les messages
        /// gardent leur mot-clé.
        #[qinvokable]
        #[cxx_name = "retirerCategorie"]
        fn retirer_categorie(self: Pin<&mut Boite>, compte: i32, chemin: &QString, mot_cle: &QString) -> bool;

        /// Répond à une demande de confirmation de lecture : l'envoie, ou
        /// l'ignore ; dans les deux cas, la demande ne sera plus posée.
        #[qinvokable]
        #[cxx_name = "repondreConfirmation"]
        fn repondre_confirmation(self: Pin<&mut Boite>, uid: i32, envoyer: bool) -> bool;

        /// Envoie, sur chaque compte connecté, les messages différés arrivés à
        /// échéance. Issue : `differesEnvoyes`, s'il y en avait.
        #[qinvokable]
        #[cxx_name = "envoyerDifferes"]
        fn envoyer_differes(self: Pin<&mut Boite>);

        /// Déplace des messages du dossier ouvert vers un dossier de n'importe
        /// quel compte connecté. Issue : `deplacementTermine`.
        #[qinvokable]
        #[cxx_name = "deplacer"]
        fn deplacer(
            self: Pin<&mut Boite>,
            uids: &QString,
            compte_cible: i32,
            chemin_cible: &QString,
        ) -> bool;

        /// Envoie des messages du dossier ouvert à la corbeille de leur compte.
        #[qinvokable]
        #[cxx_name = "supprimer"]
        fn supprimer(self: Pin<&mut Boite>, uids: &QString) -> bool;

        /// Envoie des messages d'un dossier donné à la corbeille de son
        /// compte, sans en faire le dossier ouvert : le bouton corbeille d'une
        /// ligne, dans un dossier comme dans « Toutes les BàL ». Issue :
        /// `deplacementTermine`.
        #[qinvokable]
        #[cxx_name = "supprimerDans"]
        fn supprimer_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uids: &QString) -> bool;

        /// Signale des messages du dossier ouvert comme indésirables : ils vont
        /// dans le dossier d'indésirables de leur compte, et le filtre du
        /// serveur l'apprend (Mailcow : Rspamd, par IMAPSieve). Depuis ce
        /// dossier, l'inverse : retour en boîte de réception, appris comme
        /// légitime.
        #[qinvokable]
        #[cxx_name = "signalerIndesirable"]
        fn signaler_indesirable(self: Pin<&mut Boite>, uids: &QString) -> bool;

        /// Comme `signalerIndesirable`, pour des messages d'un dossier donné de
        /// n'importe quel compte (le glisser d'une ligne, « Toutes les BàL »
        /// comprise) : vers les indésirables de leur compte, ou, depuis ce
        /// dossier, vers la boîte de réception. Issue : `deplacementTermine`.
        #[qinvokable]
        #[cxx_name = "signalerIndesirableDans"]
        fn signaler_indesirable_dans(self: Pin<&mut Boite>, compte: i32, chemin: &QString, uids: &QString) -> bool;

        /// Filtres du courrier (0.6.0, cf. `filtres`), en JSON : une liste
        /// d'objets `{nom, actif, compte, tout, conditions, lu, suivi,
        /// destination, dossier, arreter}`.
        #[qinvokable]
        fn filtres(&self) -> QString;

        /// Contacts des carnets d'adresses (0.6.1, cf. `carddav`) dont le nom,
        /// l'organisation ou une adresse contient `filtre`, en JSON :
        /// `[{nom, organisation, courriels, telephones, carnet, compte}]`.
        #[qinvokable]
        fn carnet(&self, filtre: &QString) -> QString;

        /// Enregistre la liste des filtres. Rend faux, `erreur` posée, si un
        /// filtre est incomplet : rien n'est alors changé.
        #[qinvokable]
        #[cxx_name = "poserFiltres"]
        fn poser_filtres(self: Pin<&mut Boite>, json: &QString) -> bool;

        /// Passe aux filtres toute la boîte de réception du compte, et non
        /// les seules arrivées. Rend le nombre de messages touchés ; issue :
        /// `filtresAppliques`, puis `deplacementTermine` par déplacement.
        #[qinvokable]
        #[cxx_name = "appliquerFiltres"]
        fn appliquer_filtres(self: Pin<&mut Boite>, compte: i32) -> i32;

        /// Vide, sur TOUS les comptes connectés, la corbeille et les dossiers
        /// d'indésirables (rôles SPECIAL-USE `Trash` et `Junk`, ou dont le nom
        /// évoque le pourriel). Purge définitive côté serveur. Issue par compte :
        /// `corbeillesVidees` avec le nombre de messages effacés.
        #[qinvokable]
        #[cxx_name = "viderCorbeilles"]
        fn vider_corbeilles(self: Pin<&mut Boite>);

        /// Vide entièrement un dossier précis d'un compte (clic droit « Vider »).
        /// Purge définitive côté serveur. Issue : `corbeillesVidees`.
        #[qinvokable]
        #[cxx_name = "viderDossier"]
        fn vider_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString);

        /// Crée, sur le serveur, le sous-dossier `nom` du dossier `parent`
        /// (clic droit « Nouveau sous-dossier… »). Rend le motif d'un refus
        /// immédiat — nom vide ou interdit, dossier existant, compte hors
        /// ligne —, vide si la demande est partie. Issue : `dossierCree`, ou
        /// `echec`.
        #[qinvokable]
        #[cxx_name = "creerSousDossier"]
        fn creer_sous_dossier(self: Pin<&mut Boite>, compte: i32, parent: &QString, nom: &QString) -> QString;

        /// Relit compteurs et dossier ouvert de chaque compte ; reconnecte les
        /// comptes dont la session est tombée.
        #[qinvokable]
        #[cxx_name = "actualiser"]
        fn actualiser(self: Pin<&mut Boite>);

        /// Prépare une rédaction à partir d'un message du dossier ouvert.
        /// `mode` : « repondre », « repondre_tous », « transferer » ou
        /// « brouillon » ; `adresses` : les autres adresses d'envoi du compte
        /// (JSON, liste), reconnues comme siennes. Issue : `preparation`, ou
        /// `echecRedaction`.
        #[qinvokable]
        #[cxx_name = "preparer"]
        fn preparer(self: Pin<&mut Boite>, uid: i32, mode: &QString, jeton: &QString, adresses: &QString) -> bool;

        /// Envoie un message rédigé (JSON, cf. `redaction::Redaction`) depuis
        /// le compte `compte`. Issue : `envoye`, ou `echecRedaction`.
        #[qinvokable]
        #[cxx_name = "envoyerMessage"]
        fn envoyer_message(self: Pin<&mut Boite>, compte: i32, redaction: &QString) -> bool;

        /// Enregistre un brouillon dans le dossier des brouillons du compte.
        /// Issue : `brouillonEnregistre`, ou `echecRedaction`.
        #[qinvokable]
        #[cxx_name = "enregistrerBrouillon"]
        fn enregistrer_brouillon(self: Pin<&mut Boite>, compte: i32, redaction: &QString) -> bool;

        /// Importe une signature HTML — celle d'Outlook, fichier `.htm` — pour
        /// le compte `adresse` : HTML assaini, images copiées dans le profil.
        /// Rend le HTML, ou une chaîne vide (motif dans `erreur`).
        #[qinvokable]
        #[cxx_name = "importerSignature"]
        fn importer_signature(self: Pin<&mut Boite>, adresse: &QString, url: &QString) -> QString;

        /// Copie une image dans le dossier des signatures du compte `adresse`,
        /// et rend l'adresse `file:` de la copie (vide en cas d'échec).
        #[qinvokable]
        #[cxx_name = "imageSignature"]
        fn image_signature(self: Pin<&mut Boite>, adresse: &QString, url: &QString) -> QString;

        /// Garde sur le poste l'état d'une rédaction en cours (JSON), sous
        /// l'identifiant `id` ; faux en cas d'échec (motif dans `erreur`).
        #[qinvokable]
        #[cxx_name = "garderRedaction"]
        fn garder_redaction(self: Pin<&mut Boite>, id: &QString, etat: &QString) -> bool;

        /// Retire une rédaction gardée : elle a trouvé une issue.
        #[qinvokable]
        #[cxx_name = "oublierRedactionGardee"]
        fn oublier_redaction_gardee(&self, id: &QString);

        /// Rédactions gardées par une session précédente, en JSON : tableau
        /// d'états, chacun avec son `id` et `modifie` (secondes Unix).
        #[qinvokable]
        #[cxx_name = "redactionsGardees"]
        fn redactions_gardees(&self) -> QString;

        /// Fichier à joindre, désigné par son URL (`file:…`) ou son chemin :
        /// `{chemin, nom, taille, risquee}` en JSON, ou une chaîne vide s'il
        /// n'existe pas.
        #[qinvokable]
        #[cxx_name = "decrireFichier"]
        fn decrire_fichier(&self, url: &QString) -> QString;

        /// Boîte de réception unifiée : les messages des boîtes de réception
        /// de tous les comptes, les plus récents d'abord, en JSON — chacun avec
        /// son compte et son dossier, comme une recherche dans toutes les boîtes.
        #[qinvokable]
        #[cxx_name = "boitesDeReception"]
        fn boites_de_reception(&self) -> QString;

        /// Messages non lus des boîtes de réception de tous les comptes.
        #[qinvokable]
        #[cxx_name = "nonLusReceptions"]
        fn non_lus_receptions(&self) -> i32;

        /// Dossier où ranger une image collée dans le corps d'un message.
        #[qinvokable]
        #[cxx_name = "dossierImagesCollees"]
        fn dossier_images_collees(&self) -> QString;

        /// Copie une image du poste, glissée ou collée dans le corps d'un
        /// message, parmi les images collées ; rend son adresse `file:`, vide
        /// si ce n'est pas une image de 10 Mo au plus.
        #[qinvokable]
        #[cxx_name = "importerImage"]
        fn importer_image(&self, url: &QString) -> QString;

        /// Adresses proposées à la saisie d'un destinataire, en JSON
        /// `[{nom, adresse}]` : celles à qui l'on a écrit, puis les expéditeurs.
        #[qinvokable]
        #[cxx_name = "adressesConnues"]
        fn adresses_connues(&self, filtre: &QString) -> QString;

        /// Rôle SPECIAL-USE du dossier ouvert (« Drafts », « Sent »…), vide
        /// s'il n'en a pas.
        #[qinvokable]
        #[cxx_name = "roleCourant"]
        fn role_courant(&self) -> QString;

        /// Agendas de tous les comptes, en JSON : `[{id, compte, boite, nom,
        /// couleur, affiche}]`.
        #[qinvokable]
        #[cxx_name = "agendas"]
        fn agendas(&self) -> QString;

        /// Coche ou décoche un agenda : ses événements paraissent ou non.
        #[qinvokable]
        #[cxx_name = "afficherAgenda"]
        fn afficher_agenda(&self, agenda: i32, affiche: bool);

        /// Vue de l'agenda — `jour`, `semaine` ou `mois` — autour de la date
        /// `aaaa-mm-jj`, en JSON (cf. `agenda::vue`). Locale et immédiate.
        #[qinvokable]
        #[cxx_name = "vueAgenda"]
        fn vue_agenda(&self, genre: &QString, date: &QString) -> QString;

        /// Synchronise dans un fil à part les agendas des comptes connectés.
        /// `insister` : à la demande de l'utilisateur, une boîte sans agenda
        /// connu est réinterrogée sans attendre. Sans effet pendant une
        /// synchronisation. Issue : `agendasSynchronises`.
        #[qinvokable]
        #[cxx_name = "synchroniserAgendas"]
        fn synchroniser_agendas(self: Pin<&mut Boite>, insister: bool);

        /// Formulaire prérempli d'un événement de l'index (cf.
        /// `saisie::Saisie`, en JSON) : toute la série si `serie`, sinon
        /// l'occurrence dont `occurrence` est le début d'origine. Vide si
        /// l'événement ne se lit pas.
        #[qinvokable]
        #[cxx_name = "saisieEvenement"]
        fn saisie_evenement(&self, objet: i32, occurrence: &QString, serie: bool) -> QString;

        /// Enregistre une saisie (JSON) sur le serveur : nouvel événement, ou
        /// modification de la série ou d'une occurrence. Issue :
        /// `evenementEnregistre`, puis l'agenda relu.
        #[qinvokable]
        #[cxx_name = "enregistrerEvenement"]
        fn enregistrer_evenement(self: Pin<&mut Boite>, saisie: &QString);

        /// Supprime un événement — toute la série si `serie`, sinon
        /// l'occurrence désignée. Issue : `evenementEnregistre`.
        #[qinvokable]
        #[cxx_name = "supprimerEvenement"]
        fn supprimer_evenement(self: Pin<&mut Boite>, objet: i32, occurrence: &QString, serie: bool);

        /// Rappels échus à montrer, en JSON (cf. `agenda::Rappel`).
        #[qinvokable]
        #[cxx_name = "rappelsEchus"]
        fn rappels_echus(&self) -> QString;

        /// Un rappel vu ne revient pas.
        #[qinvokable]
        #[cxx_name = "rappelVu"]
        fn rappel_vu(&self, cle: &QString);

        /// Un rappel repoussé revient dans `minutes` minutes.
        #[qinvokable]
        #[cxx_name = "repousserRappel"]
        fn repousser_rappel(&self, cle: &QString, minutes: i32);

        /// Répond à l'invitation reçue dans la boîte `compte` (`ical` : sa
        /// partie calendrier) : `ACCEPTED`, `TENTATIVE` ou `DECLINED`. La
        /// réunion est écrite dans l'agenda de la boîte — retirée après un
        /// refus —, et le serveur en envoie la réponse à l'organisateur.
        /// Issue : `evenementEnregistre`.
        #[qinvokable]
        #[cxx_name = "repondreInvitation"]
        fn repondre_invitation(self: Pin<&mut Boite>, compte: i32, ical: &QString, reponse: &QString);

        /// Retire de l'agenda de la boîte `compte` la réunion — ou l'occurrence
        /// — qu'annule ce courriel. Issue : `evenementEnregistre`.
        #[qinvokable]
        #[cxx_name = "retirerAnnulee"]
        fn retirer_annulee(self: Pin<&mut Boite>, compte: i32, ical: &QString);

        /// Répond, depuis l'agenda, à une réunion où la boîte est invitée.
        #[qinvokable]
        #[cxx_name = "repondreEvenement"]
        fn repondre_evenement(self: Pin<&mut Boite>, objet: i32, reponse: &QString);
    }

    impl cxx_qt::Threading for Boite {}

    unsafe extern "RustQt" {
        /// La session d'un compte est ouverte et son arborescence est dans
        /// l'index.
        #[qsignal]
        #[cxx_name = "connecte"]
        fn connecte(self: Pin<&mut Boite>, compte: i32, adresse: &QString);

        /// Un dossier est synchronisé ; `veille` est vrai si c'est la veille
        /// périodique qui l'a relu, et non l'utilisateur qui l'a ouvert.
        #[qsignal]
        #[cxx_name = "dossierOuvert"]
        fn dossier_ouvert(self: Pin<&mut Boite>, compte: i32, chemin: &QString, veille: bool);

        /// Corps d'un message : affichable, ou brut si `brut` est vrai.
        /// `html` : `texte` est du HTML assaini, à afficher en texte riche ;
        /// `bloquees` : nombre d'images distantes non téléchargées.
        /// `pieces` : ses pièces jointes, en JSON (`[{indice, nom, type,
        /// taille, risquee}]`). `entete` : De, À, Cc et date (JSON, cf.
        /// `index::entete_affichable`) ; `compte` et `chemin` : son dossier —
        /// un UID ne désigne un message que dans son dossier (0.6.16).
        #[qsignal]
        #[cxx_name = "corpsRecu"]
        fn corps_recu(
            self: Pin<&mut Boite>,
            uid: i32,
            texte: &QString,
            brut: bool,
            html: bool,
            bloquees: i32,
            pieces: &QString,
            confirmation: &QString,
            entete: &QString,
            compte: i32,
            chemin: &QString,
        );

        /// Une pièce jointe est écrite sur le disque : `url` la désigne pour
        /// l'ouvrir, `chemin` pour l'afficher.
        #[qsignal]
        #[cxx_name = "pieceEcrite"]
        fn piece_ecrite(self: Pin<&mut Boite>, url: &QString, chemin: &QString, ouvrir: bool);

        /// Une pièce jointe est écrite pour être jointe à une rédaction.
        #[qsignal]
        #[cxx_name = "pieceAJoindre"]
        fn piece_a_joindre(self: Pin<&mut Boite>, url: &QString);

        /// Messages du dossier ouvert marqués lus ou non lus : l'interface met
        /// à jour leurs seules lignes, sans relire la liste. `uids` : « 3,5,7 ».
        #[qsignal]
        #[cxx_name = "lusModifies"]
        fn lus_modifies(self: Pin<&mut Boite>, uids: &QString, lu: bool);

        /// Le message `uid` vient de s'afficher non lu, et `delaiLu` en
        /// diffère le marquage : l'interface rappelle `marquerLuDiffere` après
        /// `secondes`.
        #[qsignal]
        #[cxx_name = "marquageLuDiffere"]
        fn marquage_lu_differe(self: Pin<&mut Boite>, uid: i32, secondes: i32);

        /// Des drapeaux ont changé dans le dossier ouvert.
        #[qsignal]
        #[cxx_name = "drapeauxModifies"]
        fn drapeaux_modifies(self: Pin<&mut Boite>);

        /// Un déplacement est terminé : `nombre` messages déplacés, `erreurs`
        /// vide si tout s'est bien passé.
        #[qsignal]
        #[cxx_name = "deplacementTermine"]
        fn deplacement_termine(self: Pin<&mut Boite>, nombre: i32, erreurs: &QString);

        /// Des filtres ont agi sur la boîte de réception d'un compte :
        /// `touches` messages, `deplacements` déplacements lancés — chacun
        /// rendra `deplacementTermine`.
        #[qsignal]
        #[cxx_name = "filtresAppliques"]
        fn filtres_appliques(self: Pin<&mut Boite>, compte: i32, touches: i32, deplacements: i32);

        /// Du nouveau courrier dans la boîte de réception d'un compte (0.6.2) :
        /// `nombre` messages non lus, que les filtres n'ont ni marqués lus ni
        /// rangés ; l'expéditeur et l'objet du dernier.
        #[qsignal]
        #[cxx_name = "nouveauCourrier"]
        fn nouveau_courrier(self: Pin<&mut Boite>, compte: i32, nombre: i32, expediteur: &QString, sujet: &QString);

        /// Corbeille et indésirables d'un compte vidés : `nombre` messages
        /// effacés définitivement. Émis une fois par compte concerné.
        #[qsignal]
        #[cxx_name = "corbeillesVidees"]
        fn corbeilles_videes(self: Pin<&mut Boite>, nombre: i32);

        /// Sous-dossier créé : il figure dans l'arborescence, son parent
        /// déplié.
        #[qsignal]
        #[cxx_name = "dossierCree"]
        fn dossier_cree(self: Pin<&mut Boite>, compte: i32, chemin: &QString, nom: &QString);

        /// Une opération a échoué. `etape` : « connexion », « identifiants »,
        /// « dossier », « message », « tri » ou « reseau » (la session du
        /// compte est alors perdue).
        #[qsignal]
        #[cxx_name = "echec"]
        fn echec(self: Pin<&mut Boite>, compte: i32, etape: &QString, message: &QString);

        /// Compte Microsoft autorisé, enregistré et en cours de connexion.
        /// `jeton` — le jeton de rafraîchissement — va au coffre sous la clé
        /// du compte, une fois la connexion acceptée.
        #[qsignal]
        #[cxx_name = "microsoftConnecte"]
        fn microsoft_connecte(self: Pin<&mut Boite>, adresse: &QString, hote: &QString, jeton: &QString);

        /// Connexion Microsoft non aboutie. `consentement` : l'organisation
        /// veut que son administrateur autorise MMail (`consentementMicrosoft`).
        #[qsignal]
        #[cxx_name = "microsoftRefuse"]
        fn microsoft_refuse(self: Pin<&mut Boite>, message: &QString, consentement: bool);

        /// Compte Google autorisé, enregistré et en cours de connexion : comme
        /// `microsoftConnecte`.
        #[qsignal]
        #[cxx_name = "googleConnecte"]
        fn google_connecte(self: Pin<&mut Boite>, adresse: &QString, hote: &QString, jeton: &QString);

        /// Connexion Google non aboutie : le motif, à montrer tel quel.
        #[qsignal]
        #[cxx_name = "googleRefuse"]
        fn google_refuse(self: Pin<&mut Boite>, message: &QString);

        /// Microsoft a renouvelé le jeton de rafraîchissement d'un compte : il
        /// remplace l'ancien au coffre.
        #[qsignal]
        #[cxx_name = "jetonRenouvele"]
        fn jeton_renouvele(self: Pin<&mut Boite>, adresse: &QString, hote: &QString, jeton: &QString);

        /// Rédaction préparée (JSON, cf. `redaction::Preparation`) pour la
        /// fenêtre désignée par `jeton`.
        #[qsignal]
        #[cxx_name = "preparation"]
        fn preparation(self: Pin<&mut Boite>, jeton: &QString, contenu: &QString);

        /// Message envoyé ; `avertissement` non vide si une suite a échoué
        /// (copie dans « Éléments envoyés », marquage de l'original).
        #[qsignal]
        #[cxx_name = "envoye"]
        fn envoye(self: Pin<&mut Boite>, jeton: &QString, avertissement: &QString);

        /// Brouillon enregistré sous l'UID `uid` du dossier des brouillons.
        #[qsignal]
        #[cxx_name = "brouillonEnregistre"]
        fn brouillon_enregistre(self: Pin<&mut Boite>, jeton: &QString, uid: i32);

        /// Message rangé dans « Envoi différé », pour partir à `echeance`
        /// (secondes Unix, en texte : QML n'a pas d'entier sur 64 bits).
        #[qsignal]
        #[cxx_name = "programme"]
        fn programme(self: Pin<&mut Boite>, jeton: &QString, echeance: &QString);

        /// UID des messages du dossier `chemin` dont le texte contient chacun
        /// des mots de `texte`, d'après le serveur (« 3,5,7 »).
        #[qsignal]
        #[cxx_name = "resultatsServeur"]
        fn resultats_serveur(self: Pin<&mut Boite>, compte: i32, chemin: &QString, texte: &QString, uids: &QString);

        /// Recherche sur toutes les boîtes : UID trouvés par le serveur dans
        /// un dossier (« 3,5,7 », déjà rangés dans l'index), et nombre de
        /// dossiers qui restent à interroger pour ce compte.
        #[qsignal]
        #[cxx_name = "resultatsPartout"]
        fn resultats_partout(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            texte: &QString,
            uids: &QString,
            restants: i32,
        );

        /// Recherche sur le serveur (0.6.16) : le serveur n'a pas fini de
        /// chercher dans le dossier `chemin` — `dossier` : son nom lisible,
        /// `motif` : pourquoi. Ce dossier ne rend que les résultats de l'index ;
        /// `resultatsServeur` suit ce signal, `resultatsPartout` le précède.
        /// Le compte, lui, n'est pas touché.
        #[qsignal]
        #[cxx_name = "rechercheInachevee"]
        fn recherche_inachevee(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            dossier: &QString,
            texte: &QString,
            motif: &QString,
        );

        /// Messages différés partis ; `erreurs` non vide si certains ont échoué.
        #[qsignal]
        #[cxx_name = "differesEnvoyes"]
        fn differes_envoyes(self: Pin<&mut Boite>, nombre: i32, erreurs: &QString);

        /// Préparation, envoi ou enregistrement impossible.
        #[qsignal]
        #[cxx_name = "echecRedaction"]
        fn echec_redaction(self: Pin<&mut Boite>, jeton: &QString, message: &QString);

        /// Synchronisation des agendas terminée : `change` si la vue est à
        /// relire ; `erreurs` : une ligne par boîte en échec.
        #[qsignal]
        #[cxx_name = "agendasSynchronises"]
        fn agendas_synchronises(self: Pin<&mut Boite>, change: bool, erreurs: &QString);

        /// Écriture dans l'agenda terminée : `message` dit pourquoi elle a
        /// échoué.
        #[qsignal]
        #[cxx_name = "evenementEnregistre"]
        fn evenement_enregistre(self: Pin<&mut Boite>, ok: bool, message: &QString);

        /// Le message affiché porte une invitation, une réponse ou une
        /// annulation : de quoi en montrer le bandeau, en JSON (cf.
        /// `decrire_invitation`) ; vide sinon.
        #[qsignal]
        #[cxx_name = "invitationRecue"]
        fn invitation_recue(self: Pin<&mut Boite>, uid: i32, invitation: &QString);

        /// Sauvegarde la configuration du profil chez le service de M-Media
        /// (cf. `sauvegarde`) : une copie par compte connecté, chiffrée par
        /// sa clé, rangée sous le numéro `poste`. `reglages` : les réglages
        /// de l'interface, en JSON ; `empreinte` : celle de la dernière
        /// sauvegarde faite — si rien n'a changé depuis, rien ne part ;
        /// `mots_de_passe` : faux si l'utilisateur ne garde pas les siens.
        /// Rend faux si rien n'est lancé. Issue : `configurationSauvegardee`
        /// ou `sauvegardeEchouee`.
        #[qinvokable]
        #[cxx_name = "sauvegarderConfiguration"]
        fn sauvegarder_configuration(
            self: Pin<&mut Boite>,
            reglages: &QString,
            poste: &QString,
            empreinte: &QString,
            mots_de_passe: bool,
        ) -> bool;

        /// Cherche les sauvegardes que la clé d'un compte connecté ouvre.
        /// Issue : `sauvegardesTrouvees`.
        #[qinvokable]
        #[cxx_name = "chercherSauvegardes"]
        fn chercher_sauvegardes(self: Pin<&mut Boite>, compte: i32);

        /// Restaure la sauvegarde d'un poste, trouvée par
        /// `chercherSauvegardes` : comptes, état local des dossiers et des
        /// agendas, images des signatures. Rend en JSON ce qui revient à
        /// l'interface — `{"reglages": {…}, "comptes": [{adresse, hote,
        /// motDePasse?}], "images": n}` —, vide en cas d'échec (`erreur`).
        #[qinvokable]
        #[cxx_name = "restaurerConfiguration"]
        fn restaurer_configuration(self: Pin<&mut Boite>, poste: &QString) -> QString;

        /// Numéro neuf pour un poste qui n'a pas encore sauvegardé.
        #[qinvokable]
        #[cxx_name = "nouveauPoste"]
        fn nouveau_poste(self: &Boite) -> QString;

        /// Sauvegarde déposée : `copies` comptes la portent.
        #[qsignal]
        #[cxx_name = "configurationSauvegardee"]
        fn configuration_sauvegardee(self: Pin<&mut Boite>, empreinte: &QString, copies: i32);

        /// Sauvegarde impossible : une ligne par compte en échec.
        #[qsignal]
        #[cxx_name = "sauvegardeEchouee"]
        fn sauvegarde_echouee(self: Pin<&mut Boite>, message: &QString);

        /// Sauvegardes trouvées pour un compte, en JSON : `[{poste, nom,
        /// memeNom, date, programme, comptes: [adresse]}]`, la plus récente
        /// d'abord. `erreur` : le service n'a pas pu répondre.
        #[qsignal]
        #[cxx_name = "sauvegardesTrouvees"]
        fn sauvegardes_trouvees(self: Pin<&mut Boite>, compte: i32, sauvegardes: &QString, erreur: &QString);
    }
}

use core::pin::Pin;
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};

use crate::redaction::{self, Fichier, Preparation, Redaction};
use crate::smtp;

/// Délai sans commande au bout duquel un fil de travail relit ses compteurs.
const VEILLE: Duration = Duration::from_secs(120);

/// Port IMAPS : le seul que vise le client (TLS implicite).
const PORT_IMAPS: u16 = 993;

/// Dossier des messages qui attendent leur heure d'envoi, créé au premier
/// envoi différé. Il vit sur le serveur : tout MMail ouvert sur le compte les
/// envoie à l'échéance.
const DOSSIER_DIFFERE: &str = "Envoi différé";

/// De quoi ouvrir une session sur un compte. Ne vit qu'en mémoire : le mot de
/// passe est au coffre du système, jamais dans l'index.
#[derive(Clone)]
struct Identifiants {
    hote: String,
    /// Serveur de soumission, s'il n'est pas celui de la lecture ; vide sinon.
    hote_envoi: String,
    utilisateur: String,
    /// Vide pour un compte à jetons (Microsoft, Google).
    mot_de_passe: String,
    /// Compte à jetons : ses jetons OAuth2, partagés par toutes ses
    /// sessions ; `None` pour un mot de passe.
    oauth: Option<crate::oauth::Partage>,
}

impl Identifiants {
    /// Serveur auquel remettre les messages.
    fn serveur_envoi(&self) -> &str {
        if self.hote_envoi.is_empty() { &self.hote } else { &self.hote_envoi }
    }
}

/// Ouvre une session IMAP authentifiée sur un compte.
fn ouvrir_imap(p: &Identifiants) -> Result<Client, Erreur> {
    let mut client = Client::connecter(&p.hote, PORT_IMAPS)?;
    authentifier(&mut client, p)?;
    Ok(client)
}

/// Authentifie une session : mot de passe, ou jeton OAuth2 d'un compte
/// Microsoft ou Google — redemandé une fois si le serveur le refuse (révoqué,
/// ou périmé avant l'heure annoncée).
fn authentifier(client: &mut Client, p: &Identifiants) -> Result<(), Erreur> {
    let Some(jetons) = &p.oauth else {
        return client.ouvrir_session(&p.utilisateur, &p.mot_de_passe);
    };
    let service = crate::oauth::fournisseur(jetons);
    let portee = service.portee_imap();
    let jeton = crate::oauth::jeton_acces(jetons, portee).map_err(|e| e.vers_erreur(service.nom()))?;
    match client.ouvrir_session_jeton(&p.utilisateur, &jeton) {
        Err(Erreur::Refuse(_)) => {
            crate::oauth::oublier(jetons, portee);
            let jeton = crate::oauth::jeton_acces(jetons, portee).map_err(|e| e.vers_erreur(service.nom()))?;
            client.ouvrir_session_jeton(&p.utilisateur, &jeton)
        }
        autre => autre,
    }
}

/// Remet un message : par SMTP, par Graph pour un compte Microsoft (le SMTP
/// authentifié y est coupé par défaut), par SMTP avec un jeton pour un compte
/// Google.
fn remettre_message(id: &Identifiants, de: &str, destinataires: &[String], octets: &[u8], accuse: bool) -> Result<(), Erreur> {
    use crate::microsoft::{self as ms, ErreurOAuth};
    use crate::oauth::Fournisseur;
    let Some(jetons) = &id.oauth else {
        return smtp::envoyer(id.serveur_envoi(), &id.utilisateur, &id.mot_de_passe, de, destinataires, octets, accuse);
    };
    if crate::oauth::fournisseur(jetons) == Fournisseur::Google {
        use crate::google::{HOTE_SMTP, PORTEE_MAIL};
        let jeton = crate::oauth::jeton_acces(jetons, PORTEE_MAIL).map_err(|e| e.vers_erreur("Google"))?;
        return match smtp::envoyer_jeton(HOTE_SMTP, &id.utilisateur, &jeton, de, destinataires, octets, accuse) {
            Err(Erreur::Refuse(m)) if m.starts_with("authentification") => {
                crate::oauth::oublier(jetons, PORTEE_MAIL);
                let jeton = crate::oauth::jeton_acces(jetons, PORTEE_MAIL).map_err(|e| e.vers_erreur("Google"))?;
                smtp::envoyer_jeton(HOTE_SMTP, &id.utilisateur, &jeton, de, destinataires, octets, accuse)
            }
            autre => autre,
        };
    }
    let jeton = ms::jeton_acces(jetons, ms::PORTEE_ENVOI)?;
    match ms::envoyer(&jeton, octets, destinataires) {
        Err(ErreurOAuth::Refus { code, .. }) if code == "InvalidAuthenticationToken" => {
            ms::oublier(jetons, ms::PORTEE_ENVOI);
            let jeton = ms::jeton_acces(jetons, ms::PORTEE_ENVOI)?;
            ms::envoyer(&jeton, octets, destinataires).map_err(Erreur::from)
        }
        autre => autre.map_err(Erreur::from),
    }
}

/// Ce que rend une connexion OAuth2 aboutie (Microsoft, Google) : l'adresse
/// du compte, ses jetons.
struct ConnexionOAuth {
    adresse: String,
    rafraichissement: String,
    acces: String,
    duree: u64,
}

/// Ce que le service doit rendre à l'échange du code : l'adresse du compte
/// (`id_token`) et un jeton de rafraîchissement.
fn connexion_aboutie(r: crate::oauth::ReponseJeton, nom: &str) -> Result<ConnexionOAuth, crate::oauth::ErreurOAuth> {
    use crate::oauth::ErreurOAuth;
    let refus = |code: &str, description: String| ErreurOAuth::Refus { code: code.into(), description };
    let adresse = r
        .id_token
        .as_deref()
        .and_then(crate::oauth::adresse_id_token)
        .ok_or_else(|| refus("adresse", format!("{nom} n'a pas donné l'adresse du compte")))?;
    let rafraichissement = r
        .rafraichissement
        .clone()
        .ok_or_else(|| refus("offline_access", format!("{nom} n'a pas rendu de jeton de rafraîchissement")))?;
    Ok(ConnexionOAuth { adresse, rafraichissement, acces: r.acces, duree: r.duree })
}

/// Dossier cible d'un déplacement.
#[derive(Clone)]
struct Cible {
    compte: i64,
    chemin: String,
    /// Identifiants de la boîte cible quand elle n'est pas la boîte source.
    identifiants: Option<Identifiants>,
}

/// Ce que l'interface demande au fil de travail d'un compte.
#[derive(Clone)]
enum Commande {
    /// Relire l'arborescence et les compteurs.
    Arborescence,
    OuvrirDossier(String),
    /// Lire un message : sa source si `brut`, sinon de quoi l'afficher, avec
    /// ses images distantes si `distantes`.
    Corps { chemin: String, uid: u32, brut: bool, distantes: bool, numero: u64 },
    /// Écrire une pièce jointe : dans `destination` si elle est donnée, sinon
    /// dans le dossier de travail, pour l'ouvrir.
    Piece { chemin: String, uid: u32, indice: usize, destination: Option<PathBuf>, joindre: bool },
    /// `discret` : marquage d'un message qu'on affiche ; l'interface a déjà
    /// mis sa ligne à jour et ne relit pas la liste.
    MarquerLu { chemin: String, uids: Vec<u32>, lu: bool, discret: bool },
    MarquerSuivi { chemin: String, uids: Vec<u32>, suivi: bool },
    /// Poser ou retirer une catégorie (0.6.15).
    MarquerCategorie { chemin: String, uids: Vec<u32>, mot_cle: String, present: bool },
    /// Changer le catalogue des catégories d'une boîte (chemin de sa boîte de
    /// réception), relu sur le serveur juste avant.
    Catalogue { boite: String, changement: Changement },
    /// Envoyer une confirmation de lecture (identifiants présents) ou
    /// l'ignorer (absents) ; le message reçoit `$MDNSent` dans les deux cas.
    Confirmer { chemin: String, uid: u32, identifiants: Option<Identifiants> },
    /// Envoyer les messages différés arrivés à échéance.
    EnvoyerDifferes { identifiants: Identifiants },
    Deplacer { source: String, uids: Vec<u32>, cible: Cible },
    /// Vider entièrement les dossiers désignés (corbeille, indésirables) de ce
    /// compte : purge définitive côté serveur.
    ViderDossiers { chemins: Vec<String> },
    /// Créer un dossier — le sous-dossier `nom` de `parent` —, et s'y abonner.
    CreerDossier { parent: String, chemin: String, nom: String },
    /// Reprendre les déplacements interrompus dont ce compte est la source.
    Reprendre(HashMap<i64, Identifiants>),
    /// Préparer une réponse, un transfert ou la reprise d'un brouillon.
    /// `autres` : les autres adresses d'envoi du compte (0.6.7).
    Preparer { chemin: String, uid: u32, mode: String, jeton: String, autres: Vec<String> },
    /// Envoyer : il faut le mot de passe, que le fil a oublié une fois la
    /// session IMAP ouverte — il revient avec la commande.
    Envoyer { redaction: Redaction, identifiants: Identifiants },
    /// Le message est parti — par son propre fil, le SMTP ne bloquant plus
    /// le compte (0.5.9) : sa copie dans « Éléments envoyés », le brouillon
    /// repris effacé, le message d'origine marqué.
    ApresEnvoi { redaction: Redaction, copie: Vec<u8> },
    Brouillon { redaction: Redaction },
    /// Veille périodique, émise par le fil lui-même.
    Veille,
    /// Le serveur a signalé, pendant `IDLE`, un changement du dossier ouvert :
    /// il est resynchronisé. Émise par le fil lui-même.
    Signale,
    /// Garder sur le poste les messages récents d'un dossier (décision 17),
    /// par petits lots, après tout ce que l'utilisateur demande.
    Precharger { chemin: String },
}

impl Commande {
    /// Vrai si `self`, arrivée après `anterieure`, la rend caduque.
    fn remplace(&self, anterieure: &Commande) -> bool {
        use Commande::*;
        if let (Precharger { chemin: a }, Precharger { chemin: b }) = (self, anterieure) {
            return a == b;
        }
        // Lecture pour la fenêtre d'un message (numéro 0, 0.6.16) : ni
        // remplacée par l'affichage d'un autre message, ni ne le remplace.
        if matches!(anterieure, Corps { numero: 0, .. })
            || (matches!(self, Corps { numero: 0, .. }) && matches!(anterieure, Corps { .. }))
        {
            return false;
        }
        matches!(
            (self, anterieure),
            (OuvrirDossier(_), OuvrirDossier(_))
                | (OuvrirDossier(_), Corps { .. })
                | (Corps { .. }, Corps { .. })
                | (Arborescence, Arborescence)
                | (EnvoyerDifferes { .. }, EnvoyerDifferes { .. })
                | (Signale, Signale)
        ) || (matches!(anterieure, Veille) && (self.comptee() || matches!(self, Veille)))
    }

    /// Ordre de passage dans la file : ce que l'utilisateur demande, puis le
    /// préchargement. La recherche sur le serveur a sa propre connexion
    /// (0.6.16, cf. `Recherche`).
    fn rang(&self) -> u8 {
        match self {
            Commande::Precharger { .. } => 1,
            _ => 0,
        }
    }

    /// Vrai si la commande vient de l'interface et compte dans `occupe`.
    fn comptee(&self) -> bool {
        // Les tâches de fond ne font pas tourner l'indicateur d'activité.
        !matches!(
            self,
            Commande::Veille | Commande::Signale | Commande::EnvoyerDifferes { .. } | Commande::Precharger { .. }
        )
    }
}

/// Retire de la file les commandes qu'une commande postérieure rend caduques,
/// en gardant l'ordre des autres.
fn regrouper(file: Vec<Commande>) -> (Vec<Commande>, usize) {
    let garder: Vec<bool> = (0..file.len())
        .map(|i| !file[i + 1..].iter().any(|suivante| suivante.remplace(&file[i])))
        .collect();
    let mut abandonnees = 0;
    let gardees = file
        .into_iter()
        .zip(garder)
        .filter_map(|(commande, garde)| {
            if !garde && commande.comptee() {
                abandonnees += 1;
            }
            garde.then_some(commande)
        })
        .collect();
    (gardees, abandonnees)
}

/// Ce que le fil de travail rend à l'interface.
enum Issue {
    Connecte,
    Arborescence,
    Dossier { chemin: String, veille: bool },
    /// `non_lu` : le message l'était à sa lecture, l'interface le marque
    /// « lu » (cf. `marquer_lu_a_l_affichage`).
    Corps { chemin: String, uid: u32, lu: Lu, non_lu: bool, numero: u64 },
    /// Un lot du préchargement est fait ; `reste` s'il en faut un autre. Reste
    /// dans le fil : l'interface n'en sait rien.
    Precharge { chemin: String, reste: bool },
    Piece { fichier: PathBuf, ouvrir: bool, joindre: bool },
    Marque { discret: bool },
    Deplace { cible: i64, chemin_cible: String, rapport: Rapport },
    /// Un déplacement n'aura pas lieu (échec, ou commande abandonnée avec la
    /// session) : l'interface remet les lignes qu'elle avait retirées.
    DeplacementEchoue { message: String },
    /// UID trouvés par le serveur pour une recherche ; `inacheve` : le
    /// serveur n'a pas fini, et pourquoi (0.6.16).
    Recherche { chemin: String, texte: String, uids: Vec<u32>, inacheve: Option<String> },
    /// UID trouvés par le serveur dans un dossier, pour une recherche sur
    /// toutes les boîtes ; `restants` : dossiers encore à interroger.
    RecherchePartout { chemin: String, texte: String, uids: Vec<u32>, restants: usize, inacheve: Option<String> },
    /// Un marquage n'a pas atteint le serveur : l'index du dossier a peut-être
    /// pris de l'avance, il sera réconcilié à la prochaine ouverture.
    MarquageAbandonne { chemin: String },
    /// Corbeille et indésirables de ce compte vidés : `nombre` messages effacés.
    Vide { nombre: u32 },
    DossierCree { parent: String, chemin: String, nom: String },
    /// Des déplacements interrompus ont été repris : ni demandés à l'instant,
    /// ni attendus par l'interface, qui n'a qu'à relire ses compteurs.
    Repris { rapport: Rapport },
    Echec { etape: &'static str, message: String, session_perdue: bool },
    Prepare { jeton: String, contenu: String },
    Envoye { jeton: String, avertissement: String },
    /// Le fil d'envoi a fini : parti, ou `erreur`. Remise même si la session
    /// du compte a changé entre-temps — un message parti doit avoir sa suite.
    Parti { redaction: Redaction, copie: Vec<u8>, erreur: Option<String> },
    BrouillonEnregistre { jeton: String, uid: u32 },
    Programme { jeton: String, echeance: i64 },
    DifferesEnvoyes { nombre: usize, erreurs: String },
    EchecRedaction { jeton: String, message: String },
}

/// Session d'un compte, vue de l'interface.
struct Session {
    envoi: Sender<Commande>,
    /// Commandes de l'interface en cours ou en attente, connexion comprise.
    attente: Arc<AtomicUsize>,
    generation: u64,
}

/// État de connexion d'un compte, tel que l'arborescence l'affiche.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Etat {
    HorsLigne,
    Connexion,
    Pret,
    Erreur,
}

impl Etat {
    fn code(self) -> &'static str {
        match self {
            Etat::HorsLigne => "horsligne",
            Etat::Connexion => "connexion",
            Etat::Pret => "pret",
            Etat::Erreur => "erreur",
        }
    }
}

pub struct BoiteRust {
    occupe: bool,
    erreur: QString,
    /// Connexion Microsoft lancée sous Android, en attente de l'adresse de
    /// retour : (client, state, vérificateur PKCE, adresse).
    attente_microsoft: Option<(String, String, String, String)>,
    revision: i32,
    compte_courant: i32,
    dossier_courant: QString,
    en_attente: i32,
    profil: String,
    magasin: Option<Magasin>,
    sessions: HashMap<i64, Session>,
    identites: HashMap<i64, Identifiants>,
    etats: HashMap<i64, Etat>,
    /// Motif de la dernière perte de session de chaque compte : l'interface
    /// le montre au survol de « erreur », quand le message passager s'est
    /// effacé.
    motifs: HashMap<i64, String>,
    /// Comptes créés par une connexion pas encore aboutie.
    nouveaux: HashSet<i64>,
    generation: u64,
    /// Dossier ouvert : compte, chemin, identifiant dans l'index.
    courant: Option<(i64, String, i64)>,
    /// Liste envoyée en dernier à l'interface (cf. `ListeEnvoyee`).
    liste: Option<ListeEnvoyee>,
    /// Marquages lu / non lu demandés par l'utilisateur : par message (dossier,
    /// UID), le numéro d'affichage (`AFFICHAGE`) du moment. Un affichage
    /// demandé avant ne marque plus le message « lu » (0.6.7).
    marquages: HashMap<(i64, u32), u64>,
    agenda_occupe: bool,
    delai_lu: i32,
    revision_categories: i32,
    /// Empreinte des catalogues rangés à la dernière relecture : un
    /// changement fait augmenter `revision_categories`.
    empreinte_catalogues: u64,
    /// Message affiché non lu dont `delai_lu` diffère le marquage ; oublié dès
    /// qu'un autre message est demandé.
    lu_differe: Option<LuDiffere>,
    /// Comptes dont le serveur a accepté le mot de passe pendant cette
    /// session : seule leur clé sert à la sauvegarde.
    authentifies: HashSet<i64>,
    sauvegarde_en_cours: bool,
    /// Sauvegardes lues par `chercher_sauvegardes`, par numéro de poste.
    sauvegardes_lues: HashMap<String, crate::sauvegarde::Document>,
    /// Numéro de la recherche sur le serveur en cours : une nouvelle, ou
    /// l'arrêt, l'augmente, et les fils de la précédente s'arrêtent (0.6.16).
    recherche: Arc<AtomicU64>,
}

/// Liste envoyée à l'interface : dossier, et pour chaque message son UID, sa
/// date et l'empreinte de ses champs — de quoi ne lui renvoyer que ce qui a
/// changé. Un gros dossier part par pages (0.5.9) : `fin` est la clé (date,
/// UID) du dernier message envoyé tant qu'il en reste ; les changements ne
/// portent que jusque-là, la suite arrive avec les pages. `version` : celle du
/// dossier dans l'index au moment de l'envoi — tant qu'elle n'a pas bougé,
/// rien n'est à relire (0.6.6).
struct ListeEnvoyee {
    dossier: i64,
    empreintes: Vec<(u32, i64, u64)>,
    fin: Option<(i64, u32)>,
    version: i64,
}

impl ListeEnvoyee {
    /// L'interface ne montre plus, pour ces messages, ce que dit la liste
    /// envoyée : leur empreinte ne correspond plus à rien, la prochaine
    /// différence les renvoie, et la version oubliée la fait calculer.
    fn oublier(&mut self, uids: &[u32]) {
        for e in self.empreintes.iter_mut().filter(|e| uids.contains(&e.0)) {
            e.2 = 0;
        }
        self.version = -1;
    }
}

/// Message affiché non lu, en attente de son marquage « lu » (cf. `delai_lu`).
struct LuDiffere {
    compte: i64,
    chemin: String,
    dossier: i64,
    uid: u32,
    numero: u64,
}

/// Vrai si l'utilisateur a marqué ce message (lu ou non lu) après la demande
/// d'affichage `numero` : le marquage automatique de cet affichage n'a plus
/// cours.
fn marque_depuis(marquages: &HashMap<(i64, u32), u64>, dossier: i64, uid: u32, numero: u64) -> bool {
    marquages.get(&(dossier, uid)).is_some_and(|&n| n >= numero)
}

/// Messages de la première page d'une liste : de quoi remplir l'écran le plus
/// grand, aussitôt.
const PREMIERE_PAGE: usize = 200;

impl Default for BoiteRust {
    fn default() -> Self {
        Self {
            occupe: false,
            erreur: QString::from(""),
            attente_microsoft: None,
            revision: 0,
            compte_courant: 0,
            dossier_courant: QString::from(""),
            en_attente: 0,
            profil: String::new(),
            magasin: None,
            sessions: HashMap::new(),
            identites: HashMap::new(),
            etats: HashMap::new(),
            motifs: HashMap::new(),
            nouveaux: HashSet::new(),
            generation: 0,
            courant: None,
            liste: None,
            marquages: HashMap::new(),
            agenda_occupe: false,
            delai_lu: 0,
            revision_categories: 0,
            empreinte_catalogues: 0,
            lu_differe: None,
            authentifies: HashSet::new(),
            sauvegarde_en_cours: false,
            sauvegardes_lues: HashMap::new(),
            recherche: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl qobject::Boite {
    pub fn ouvrir_profil(mut self: Pin<&mut Self>, chemin: &QString) -> bool {
        let chemin = chemin.to_string();
        if self.magasin.is_some() && self.profil == chemin {
            return true;
        }
        match Magasin::ouvrir(&chemin) {
            Ok(magasin) => {
                // Les pièces jointes ouvertes lors d'une session précédente
                // n'ont plus à traîner sur le disque.
                let _ = std::fs::remove_dir_all(dossier_pieces(&chemin));
                let _ = std::fs::remove_dir_all(dossier_affichage(&chemin));
                // Messages gardés sur le poste : ce qui est sorti de la fenêtre
                // d'un mois, puis ce que l'index ne connaît plus. Dans un fil à
                // part, par sa propre connexion à l'index : parcourir des
                // dizaines de milliers de fichiers retardait l'apparition de la
                // fenêtre.
                let profil = chemin.clone();
                let _ = thread::Builder::new().name("mmail-menage".into()).spawn(move || {
                    let Ok(magasin) = Magasin::ouvrir(&profil) else { return };
                    let cache = Cache::du_profil(&profil);
                    for id in magasin.gardes_perimes(crate::cache::limite(maintenant())).unwrap_or_default() {
                        cache.retirer(id);
                        let _ = magasin.poser_etat_corps(id, "headers");
                    }
                    if let Ok(gardes) = magasin.gardes() {
                        cache.ranger(&gardes);
                    }
                });
                let attente = magasin.nombre_operations().unwrap_or(0) as i32;
                {
                    let mut noyau = self.as_mut().rust_mut();
                    noyau.magasin = Some(magasin);
                    noyau.profil = chemin;
                }
                self.as_mut().set_en_attente(attente);
                self.as_mut().set_erreur(QString::from(""));
                self.as_mut().reviser();
                true
            }
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("index local : {e}")));
                false
            }
        }
    }

    pub fn agendas(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let comptes = magasin.comptes().unwrap_or_default();
        let liste: Vec<serde_json::Value> = magasin
            .agendas()
            .unwrap_or_default()
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "compte": a.compte,
                    "boite": comptes.iter().find(|c| c.id == a.compte).map(|c| c.adresse.as_str()).unwrap_or(""),
                    "nom": a.nom,
                    "couleur": crate::agenda::couleur(a),
                    "affiche": a.affiche,
                    "ecriture": a.ecriture,
                    "invitations": magasin.planification(a.compte).ok().flatten().unwrap_or(false),
                })
            })
            .collect();
        QString::from(&serde_json::Value::from(liste).to_string())
    }

    pub fn afficher_agenda(&self, agenda: i32, affiche: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.afficher_agenda(agenda as i64, affiche);
        }
    }

    pub fn vue_agenda(&self, genre: &QString, date: &QString) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("{}");
        };
        let date = chrono::NaiveDate::parse_from_str(&date.to_string(), "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive());
        QString::from(&crate::agenda::vue(magasin, &genre.to_string(), date))
    }

    pub fn synchroniser_agendas(mut self: Pin<&mut Self>, insister: bool) {
        if self.agenda_occupe {
            return;
        }
        let Some(magasin) = self.magasin.as_ref() else { return };
        // Les comptes dont on a le mot de passe : ceux qui sont connectés. Un
        // compte Microsoft ou Google n'a pas d'agenda CalDAV.
        let comptes: Vec<(i64, String, Identifiants)> = magasin
            .comptes()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| self.identites.get(&c.id).map(|id| (c.id, c.adresse, id.clone())))
            .filter(|(_, _, id)| id.oauth.is_none())
            .collect();
        if comptes.is_empty() {
            return;
        }
        let profil = self.profil.clone();
        let fil = self.qt_thread();
        self.as_mut().set_agenda_occupe(true);
        let lance = thread::Builder::new().name("mmail-agenda".into()).spawn(move || {
            let mut change = false;
            let mut erreurs = Vec::new();
            match Magasin::ouvrir(&profil) {
                Ok(magasin) => {
                    for (compte, adresse, id) in &comptes {
                        let acces = crate::caldav::Acces { utilisateur: &id.utilisateur, mot_de_passe: &id.mot_de_passe };
                        match crate::caldav::synchroniser(&magasin, *compte, &id.hote, &acces, maintenant(), insister) {
                            Ok(bilan) => change |= bilan.change,
                            Err(e) => erreurs.push(format!("{adresse} : {e}")),
                        }
                        // Les carnets d'adresses, au même serveur et au même
                        // rythme que les agendas.
                        if let Err(e) = crate::carddav::synchroniser(&magasin, *compte, &id.hote, &acces, maintenant(), insister) {
                            erreurs.push(format!("{adresse} (contacts) : {e}"));
                        }
                    }
                }
                Err(e) => erreurs.push(format!("index local : {e}")),
            }
            let _ = fil.queue(move |mut boite: Pin<&mut qobject::Boite>| {
                boite.as_mut().set_agenda_occupe(false);
                boite.as_mut().agendas_synchronises(change, &QString::from(&erreurs.join("\n")));
            });
        });
        if lance.is_err() {
            self.as_mut().set_agenda_occupe(false);
        }
    }

    pub fn sauvegarder_configuration(
        mut self: Pin<&mut Self>,
        reglages: &QString,
        poste: &QString,
        empreinte: &QString,
        mots_de_passe: bool,
    ) -> bool {
        let poste = poste.to_string();
        if self.sauvegarde_en_cours || !crate::sauvegarde::poste_valide(&poste) {
            return false;
        }
        let Some(magasin) = self.magasin.as_ref() else { return false };
        // La clé de chaque copie : un compte dont le serveur a accepté le mot
        // de passe.
        let comptes: Vec<(String, String)> = magasin
            .comptes()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| self.authentifies.contains(&c.id) && !c.oauth())
            .filter_map(|c| self.identites.get(&c.id).map(|i| (c.adresse, i.mot_de_passe.clone())))
            .collect();
        if comptes.is_empty() {
            return false;
        }
        let mots: HashMap<String, String> =
            if mots_de_passe { comptes.iter().cloned().collect() } else { HashMap::new() };
        let document = crate::sauvegarde::composer(
            magasin,
            &dossier_signatures(&self.profil),
            &mots,
            &reglages.to_string(),
            &crate::poste::nom(),
            env!("CARGO_PKG_VERSION"),
            maintenant(),
        );
        let document = match document {
            Ok(d) => d,
            Err(e) => {
                self.as_mut().sauvegarde_echouee(&QString::from(&e.to_string()));
                return false;
            }
        };
        let neuve = crate::sauvegarde::empreinte(&document);
        if neuve == empreinte.to_string() {
            return false;
        }
        let fil = self.qt_thread();
        self.as_mut().rust_mut().sauvegarde_en_cours = true;
        let lance = thread::Builder::new().name("mmail-sauvegarde".into()).spawn(move || {
            let mut copies = 0;
            let mut erreurs = Vec::new();
            for (adresse, mot_de_passe) in &comptes {
                let cles = crate::sauvegarde::cles(adresse, mot_de_passe);
                // La copie de ce compte ne porte que son mot de passe.
                let clair = serde_json::to_vec(&crate::sauvegarde::pour_compte(&document, adresse)).unwrap_or_default();
                match crate::sauvegarde::chiffrer(&cles, &clair)
                    .and_then(|copie| crate::sauvegarde::deposer(&cles, &poste, &copie))
                {
                    Ok(()) => copies += 1,
                    Err(e) => erreurs.push(format!("{adresse} : {e}")),
                }
            }
            let _ = fil.queue(move |mut boite: Pin<&mut qobject::Boite>| {
                boite.as_mut().rust_mut().sauvegarde_en_cours = false;
                if erreurs.is_empty() {
                    boite.as_mut().configuration_sauvegardee(&QString::from(&neuve), copies);
                } else {
                    boite.as_mut().sauvegarde_echouee(&QString::from(&erreurs.join("\n")));
                }
            });
        });
        if lance.is_err() {
            self.as_mut().rust_mut().sauvegarde_en_cours = false;
            return false;
        }
        true
    }

    pub fn chercher_sauvegardes(mut self: Pin<&mut Self>, compte: i32) {
        let id = compte as i64;
        let adresse = self.magasin.as_ref().and_then(|m| m.compte_par_id(id).ok().flatten()).map(|c| c.adresse);
        let (Some(adresse), Some(identifiants)) = (adresse, self.identites.get(&id).cloned()) else {
            self.as_mut().sauvegardes_trouvees(compte, &QString::from("[]"), &QString::from("compte non connecté"));
            return;
        };
        if let Some(jetons) = &identifiants.oauth {
            let motif = format!(
                "compte {} : une sauvegarde se retrouve par un compte à mot de passe",
                crate::oauth::fournisseur(jetons).nom()
            );
            self.as_mut().sauvegardes_trouvees(compte, &QString::from("[]"), &QString::from(&motif));
            return;
        }
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-sauvegarde".into()).spawn(move || {
            let cles = crate::sauvegarde::cles(&adresse, &identifiants.mot_de_passe);
            let mut lus = Vec::new();
            let erreur = match crate::sauvegarde::lister(&cles) {
                Ok(copies) => {
                    for copie in copies {
                        let document = crate::sauvegarde::lire(&cles, &copie.poste)
                            .ok()
                            .and_then(|brut| crate::sauvegarde::dechiffrer(&cles, &brut))
                            .and_then(|clair| crate::sauvegarde::lire_document(&clair));
                        if let Some(d) = document {
                            lus.push((copie.poste, d));
                        }
                    }
                    String::new()
                }
                Err(e) => e.to_string(),
            };
            let _ = fil.queue(move |mut boite: Pin<&mut qobject::Boite>| {
                let nom = crate::poste::nom();
                let resume: Vec<serde_json::Value> = lus
                    .iter()
                    .map(|(poste, d)| {
                        serde_json::json!({
                            "poste": poste,
                            "nom": d.poste,
                            "memeNom": d.poste.eq_ignore_ascii_case(&nom),
                            "date": d.date,
                            "programme": d.programme,
                            "comptes": d.comptes.iter().map(|c| c.adresse.clone()).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                boite.as_mut().rust_mut().sauvegardes_lues = lus.into_iter().collect();
                let resume = serde_json::Value::Array(resume).to_string();
                boite.as_mut().sauvegardes_trouvees(compte, &QString::from(&resume), &QString::from(&erreur));
            });
        });
        if lance.is_err() {
            self.as_mut().sauvegardes_trouvees(compte, &QString::from("[]"), &QString::from("fil de travail impossible"));
        }
    }

    pub fn restaurer_configuration(mut self: Pin<&mut Self>, poste: &QString) -> QString {
        let Some(document) = self.sauvegardes_lues.get(&poste.to_string()).cloned() else {
            self.as_mut().set_erreur(QString::from("sauvegarde introuvable : à chercher de nouveau"));
            return QString::from("");
        };
        let Some(magasin) = self.magasin.as_ref() else {
            self.as_mut().set_erreur(QString::from("le profil n'est pas ouvert"));
            return QString::from("");
        };
        if let Err(e) = crate::sauvegarde::appliquer(magasin, &document) {
            self.as_mut().set_erreur(QString::from(&format!("restauration : {e}")));
            return QString::from("");
        }
        let signatures = dossier_signatures(&self.profil);
        let images = match crate::sauvegarde::poser_images(&document, &signatures) {
            Ok(n) => n,
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("images des signatures : {e}")));
                0
            }
        };
        let reglages: serde_json::Value =
            serde_json::from_str(&crate::sauvegarde::reglages_pour(&document, &signatures)).unwrap_or_default();
        let comptes: Vec<serde_json::Value> = document
            .comptes
            .iter()
            .map(|c| serde_json::json!({ "adresse": c.adresse, "hote": c.hote, "motDePasse": c.mot_de_passe, "auth": c.auth }))
            .collect();
        self.as_mut().reviser();
        QString::from(&serde_json::json!({ "reglages": reglages, "comptes": comptes, "images": images }).to_string())
    }

    pub fn nouveau_poste(&self) -> QString {
        QString::from(&crate::sauvegarde::nouveau_poste())
    }

    pub fn saisie_evenement(&self, objet: i32, occurrence: &QString, serie: bool) -> QString {
        let Some(o) = self.magasin.as_ref().and_then(|m| m.objet(objet as i64).ok().flatten()) else {
            return QString::from("");
        };
        let occurrence = if serie { None } else { occurrence.to_string().parse::<i64>().ok() };
        match crate::saisie::saisie_de(&o.ical, occurrence, crate::saisie::fuseau_du_poste()) {
            Some(mut s) => {
                s.agenda = o.agenda;
                s.objet = o.id;
                QString::from(&serde_json::to_string(&s).unwrap_or_default())
            }
            None => QString::from(""),
        }
    }

    pub fn enregistrer_evenement(mut self: Pin<&mut Self>, saisie: &QString) {
        let saisie: crate::saisie::Saisie = match serde_json::from_str(&saisie.to_string()) {
            Ok(s) => s,
            Err(e) => {
                self.as_mut().evenement_enregistre(false, &QString::from(&format!("saisie illisible : {e}")));
                return;
            }
        };
        let mut saisie = saisie;
        let objet = if saisie.objet > 0 { self.magasin.as_ref().and_then(|m| m.objet(saisie.objet).ok().flatten()) } else { None };
        // L'organisateur d'une réunion nouvelle : la boîte de l'agenda.
        if saisie.organisateur.is_empty() {
            let agenda = objet.as_ref().map(|o| o.agenda).unwrap_or(saisie.agenda);
            if let Some(m) = self.magasin.as_ref() {
                let compte = m.agendas().unwrap_or_default().into_iter().find(|a| a.id == agenda).map(|a| a.compte).unwrap_or(0);
                saisie.organisateur = m.compte_par_id(compte).ok().flatten().map(|c| c.adresse).unwrap_or_default();
            }
        }
        if saisie.objet > 0 && objet.is_none() {
            self.as_mut().evenement_enregistre(false, &QString::from("événement introuvable dans l'index"));
            return;
        }
        let agenda = objet.as_ref().map(|o| o.agenda).unwrap_or(saisie.agenda);
        self.lancer_ecriture(
            agenda,
            Box::new(move |dav, agenda| {
                let (tz, maintenant) = (crate::saisie::fuseau_du_poste(), chrono::Utc::now());
                match objet {
                    None => {
                        let (uid, ical) = crate::saisie::creer(&saisie, tz, maintenant)?;
                        let url = format!("{}/{uid}.ics", agenda.adresse.trim_end_matches('/'));
                        crate::caldav::deposer(dav, &url, &ical, None).map(|_| ()).map_err(|e| e.to_string())
                    }
                    Some(o) => {
                        let ical = crate::saisie::modifier(&o.ical, &saisie, tz, maintenant)?;
                        let url = crate::caldav::adresse(&agenda.adresse, &o.href).ok_or("adresse de l'événement invalide")?;
                        crate::caldav::deposer(dav, &url, &ical, Some(&o.etag)).map(|_| ()).map_err(|e| e.to_string())
                    }
                }
            }),
        );
    }

    pub fn supprimer_evenement(mut self: Pin<&mut Self>, objet: i32, occurrence: &QString, serie: bool) {
        let Some(o) = self.magasin.as_ref().and_then(|m| m.objet(objet as i64).ok().flatten()) else {
            self.as_mut().evenement_enregistre(false, &QString::from("événement introuvable dans l'index"));
            return;
        };
        let occurrence = if serie { None } else { occurrence.to_string().parse::<i64>().ok() };
        let agenda = o.agenda;
        self.lancer_ecriture(
            agenda,
            Box::new(move |dav, agenda| {
                let url = crate::caldav::adresse(&agenda.adresse, &o.href).ok_or("adresse de l'événement invalide")?;
                match occurrence {
                    None => crate::caldav::retirer(dav, &agenda.adresse, &url, Some(&o.etag)).map_err(|e| e.to_string()),
                    Some(origine) => {
                        let ical = crate::saisie::retirer_occurrence(&o.ical, origine, chrono::Utc::now())?;
                        crate::caldav::deposer(dav, &url, &ical, Some(&o.etag)).map(|_| ()).map_err(|e| e.to_string())
                    }
                }
            }),
        );
    }

    /// Écrit dans un agenda depuis un fil à part, avec les identifiants de sa
    /// boîte ; relit ensuite l'agenda — réussite ou conflit — et rend compte.
    fn lancer_ecriture(
        mut self: Pin<&mut Self>,
        agenda: i64,
        travail: Box<dyn FnOnce(&crate::caldav::Acces, &crate::magasin::AgendaLocal) -> Result<(), String> + Send>,
    ) {
        let Some(magasin) = self.magasin.as_ref() else { return };
        let Some(local) = magasin.agendas().unwrap_or_default().into_iter().find(|a| a.id == agenda) else {
            self.as_mut().evenement_enregistre(false, &QString::from("agenda introuvable"));
            return;
        };
        let Some(id) = self.identites.get(&local.compte).cloned() else {
            self.as_mut().evenement_enregistre(false, &QString::from("la boîte de cet agenda n'est pas connectée"));
            return;
        };
        let profil = self.profil.clone();
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-agenda-ecriture".into()).spawn(move || {
            let acces = crate::caldav::Acces { utilisateur: &id.utilisateur, mot_de_passe: &id.mot_de_passe };
            let issue = travail(&acces, &local);
            if let Ok(magasin) = Magasin::ouvrir(&profil) {
                let _ = crate::caldav::synchroniser(&magasin, local.compte, &id.hote, &acces, maintenant(), false);
            }
            let _ = fil.queue(move |mut boite: Pin<&mut qobject::Boite>| {
                let message = issue.as_ref().err().cloned().unwrap_or_default();
                boite.as_mut().agendas_synchronises(true, &QString::from(""));
                boite.as_mut().evenement_enregistre(issue.is_ok(), &QString::from(&message));
            });
        });
        if lance.is_err() {
            self.as_mut().evenement_enregistre(false, &QString::from("impossible de lancer l'écriture"));
        }
    }

    /// Bandeau d'une invitation, en JSON : sa description (cf.
    /// `invitation::Invitation`), la boîte et son adresse, si le serveur
    /// distribue les invitations, la réunion telle que l'agenda la connaît, et
    /// la partie calendrier (`ical`), rendue telle quelle aux actions. Une
    /// réponse reçue par l'organisateur est inscrite dans sa copie au passage.
    fn decrire_invitation(mut self: Pin<&mut Self>, compte: i64, extrait: &crate::invitation::Extrait) -> String {
        let Some(magasin) = self.magasin.as_ref() else { return String::new() };
        let moi = magasin.compte_par_id(compte).ok().flatten().map(|c| c.adresse.to_lowercase()).unwrap_or_default();
        let tz = crate::saisie::fuseau_du_poste();
        let Some(description) = crate::invitation::decrire(extrait, &tz) else { return String::new() };
        let objet = magasin.objet_par_uid(compte, &description.uid).ok().flatten();
        // `null` : pas encore demandé au serveur (première synchronisation de
        // l'agenda à venir) — les boutons restent actifs.
        let planification = magasin.planification(compte).ok().flatten();
        let (sequence_agenda, ma_reponse) = objet
            .as_ref()
            .map(|o| (crate::invitation::identite(&o.ical).map(|(_, s)| s).unwrap_or(0), crate::invitation::reponse_de(&o.ical, &moi)))
            .unwrap_or((-1, String::new()));
        let mut inscrite = false;
        if extrait.methode == "REPLY" {
            // Seule la copie de l'organisateur — la boîte — reçoit les réponses.
            let organisateur = description.organisateur.as_ref().is_some_and(|p| p.adresse == moi);
            if let Some(o) = objet.as_ref().filter(|_| organisateur) {
                if let Ok(Some(ical)) = crate::invitation::inscrire_reponse(&o.ical, &extrait.ical) {
                    inscrite = true;
                    let (agenda, href, etag) = (o.agenda, o.href.clone(), o.etag.clone());
                    self.as_mut().lancer_ecriture(
                        agenda,
                        Box::new(move |dav, agenda| {
                            let url = crate::caldav::adresse(&agenda.adresse, &href).ok_or("adresse de l'événement invalide")?;
                            crate::caldav::deposer(dav, &url, &ical, Some(&etag)).map(|_| ()).map_err(|e| e.to_string())
                        }),
                    );
                }
            }
        }
        serde_json::json!({
            "invitation": description,
            "compte": compte,
            "moi": moi,
            "planification": planification,
            "dansAgenda": objet.is_some(),
            "sequenceAgenda": sequence_agenda,
            "maReponse": ma_reponse,
            "inscrite": inscrite,
            "ical": extrait.ical,
        })
        .to_string()
    }

    /// Agenda où écrire une réunion reçue par la boîte `compte` : celui qui
    /// la porte déjà, sinon l'agenda personnel, sinon le premier qui accepte
    /// l'écriture.
    fn agenda_de_reception(magasin: &Magasin, compte: i64, objet: Option<&crate::magasin::ObjetLocal>) -> Option<crate::magasin::AgendaLocal> {
        let agendas: Vec<_> = magasin.agendas().ok()?.into_iter().filter(|a| a.compte == compte).collect();
        if let Some(o) = objet {
            return agendas.into_iter().find(|a| a.id == o.agenda);
        }
        let modifiables: Vec<_> = agendas.into_iter().filter(|a| a.ecriture).collect();
        modifiables
            .iter()
            .find(|a| a.adresse.trim_end_matches('/').ends_with("/personal"))
            .or(modifiables.first())
            .cloned()
    }

    /// Écrit, depuis un fil à part, la réponse `reponse` de la boîte `compte`
    /// à la réunion `uid` — `ical` : l'invitation, ou la réunion telle que
    /// l'agenda la connaît. L'agenda est d'abord relu : la réunion a pu y
    /// arriver depuis (SOGo y dépose l'invitation d'un collègue), et l'écrire
    /// à côté en ferait un doublon. Le serveur avertit l'organisateur ; après
    /// un refus, la réunion est retirée, comme le fait Outlook.
    fn ecrire_reponse(mut self: Pin<&mut Self>, compte: i64, uid: String, ical: String, reponse: String) {
        let moi = self
            .magasin
            .as_ref()
            .and_then(|m| m.compte_par_id(compte).ok().flatten())
            .map(|c| c.adresse.to_lowercase())
            .unwrap_or_default();
        self.as_mut().lancer_pour_compte(
            compte,
            Box::new(move |dav, magasin| {
                let objet = magasin.objet_par_uid(compte, &uid).map_err(|e| e.to_string())?;
                let agenda = Self::agenda_de_reception(magasin, compte, objet.as_ref())
                    .ok_or("aucun agenda de cette boîte n'accepte l'écriture")?;
                let ecrit = crate::invitation::repondre(&ical, objet.as_ref().map(|o| o.ical.as_str()), &moi, &reponse)?;
                let url = match &objet {
                    Some(o) => crate::caldav::adresse(&agenda.adresse, &o.href).ok_or("adresse de l'événement invalide")?,
                    None => format!("{}/{}.ics", agenda.adresse.trim_end_matches('/'), nom_d_objet(&uid)),
                };
                let etag = crate::caldav::deposer(dav, &url, &ecrit, objet.as_ref().map(|o| o.etag.as_str())).map_err(|e| e.to_string())?;
                if reponse == "DECLINED" {
                    crate::caldav::retirer(dav, &agenda.adresse, &url, etag.as_deref()).map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        );
    }

    pub fn repondre_invitation(mut self: Pin<&mut Self>, compte: i32, ical: &QString, reponse: &QString) {
        let ical = ical.to_string();
        let Some((uid, _)) = crate::invitation::identite(&ical) else {
            self.as_mut().evenement_enregistre(false, &QString::from("invitation illisible"));
            return;
        };
        self.ecrire_reponse(compte as i64, uid, ical, reponse.to_string());
    }

    pub fn repondre_evenement(mut self: Pin<&mut Self>, objet: i32, reponse: &QString) {
        let Some(magasin) = self.magasin.as_ref() else { return };
        let Some(o) = magasin.objet(objet as i64).ok().flatten() else {
            self.as_mut().evenement_enregistre(false, &QString::from("événement introuvable dans l'index"));
            return;
        };
        let compte = magasin.agendas().unwrap_or_default().into_iter().find(|a| a.id == o.agenda).map(|a| a.compte).unwrap_or(0);
        let uid = crate::invitation::identite(&o.ical).map(|(u, _)| u).unwrap_or_default();
        self.ecrire_reponse(compte, uid, o.ical, reponse.to_string());
    }

    pub fn retirer_annulee(mut self: Pin<&mut Self>, compte: i32, ical: &QString) {
        let (compte, ical) = (compte as i64, ical.to_string());
        let Some((uid, _)) = crate::invitation::identite(&ical) else {
            self.as_mut().evenement_enregistre(false, &QString::from("annulation illisible"));
            return;
        };
        // Une occurrence annulée : une exception dans la série ; la réunion
        // entière : l'objet retiré.
        let occurrence = crate::agenda::analyser(&ical).and_then(|r| {
            r.enfants.iter().filter(|c| c.nom == "VEVENT").find_map(|c| crate::saisie::origine_de(&r, c))
        });
        self.as_mut().lancer_pour_compte(
            compte,
            Box::new(move |dav, magasin| {
                let Some(o) = magasin.objet_par_uid(compte, &uid).map_err(|e| e.to_string())? else { return Ok(()) };
                let agenda = Self::agenda_de_reception(magasin, compte, Some(&o)).ok_or("agenda introuvable")?;
                let url = crate::caldav::adresse(&agenda.adresse, &o.href).ok_or("adresse de l'événement invalide")?;
                match occurrence {
                    Some(origine) => {
                        let ical = crate::saisie::retirer_occurrence(&o.ical, origine, chrono::Utc::now())?;
                        crate::caldav::deposer(dav, &url, &ical, Some(&o.etag)).map(|_| ()).map_err(|e| e.to_string())
                    }
                    None => crate::caldav::retirer(dav, &agenda.adresse, &url, Some(&o.etag)).map_err(|e| e.to_string()),
                }
            }),
        );
    }

    /// Comme `lancer_ecriture`, pour une écriture dont l'agenda ne se décide
    /// qu'après avoir relu ceux de la boîte `compte` : le travail reçoit
    /// l'index à jour.
    fn lancer_pour_compte(
        mut self: Pin<&mut Self>,
        compte: i64,
        travail: Box<dyn FnOnce(&crate::caldav::Acces, &Magasin) -> Result<(), String> + Send>,
    ) {
        let Some(id) = self.identites.get(&compte).cloned() else {
            self.as_mut().evenement_enregistre(false, &QString::from("cette boîte n'est pas connectée"));
            return;
        };
        if let Some(jetons) = &id.oauth {
            let motif = format!("pas d'agenda pour un compte {} dans MMail", crate::oauth::fournisseur(jetons).nom());
            self.as_mut().evenement_enregistre(false, &QString::from(&motif));
            return;
        }
        let profil = self.profil.clone();
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-agenda-ecriture".into()).spawn(move || {
            let acces = crate::caldav::Acces { utilisateur: &id.utilisateur, mot_de_passe: &id.mot_de_passe };
            let issue = match Magasin::ouvrir(&profil) {
                Ok(magasin) => {
                    let _ = crate::caldav::synchroniser(&magasin, compte, &id.hote, &acces, maintenant(), true);
                    let issue = travail(&acces, &magasin);
                    let _ = crate::caldav::synchroniser(&magasin, compte, &id.hote, &acces, maintenant(), false);
                    issue
                }
                Err(e) => Err(format!("index local : {e}")),
            };
            let _ = fil.queue(move |mut boite: Pin<&mut qobject::Boite>| {
                let message = issue.as_ref().err().cloned().unwrap_or_default();
                boite.as_mut().agendas_synchronises(true, &QString::from(""));
                boite.as_mut().evenement_enregistre(issue.is_ok(), &QString::from(&message));
            });
        });
        if lance.is_err() {
            self.as_mut().evenement_enregistre(false, &QString::from("impossible de lancer l'écriture"));
        }
    }

    pub fn rappels_echus(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let rappels = crate::agenda::rappels_echus(magasin, chrono::Utc::now());
        QString::from(&serde_json::to_string(&rappels).unwrap_or_else(|_| "[]".into()))
    }

    pub fn rappel_vu(&self, cle: &QString) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.poser_rappel(&cle.to_string(), true, 0, maintenant());
        }
    }

    pub fn repousser_rappel(&self, cle: &QString, minutes: i32) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.poser_rappel(&cle.to_string(), false, maintenant() + minutes.max(1) as i64 * 60, maintenant());
        }
    }

    pub fn comptes(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let comptes = magasin.comptes().unwrap_or_default();
        let corps: Vec<String> = comptes
            .iter()
            .map(|c| {
                format!(
                    r#"{{"id":{},"adresse":{},"hote":{},"hoteEnvoi":{},"auth":{},"etat":{}}}"#,
                    c.id,
                    texte_json(&c.adresse),
                    texte_json(&c.hote),
                    texte_json(&c.hote_envoi),
                    texte_json(&c.auth),
                    texte_json(self.etat(c.id).code())
                )
            })
            .collect();
        QString::from(&format!("[{}]", corps.join(",")))
    }

    pub fn connecter(
        mut self: Pin<&mut Self>,
        hote: &QString,
        adresse: &QString,
        mot_de_passe: &QString,
    ) -> bool {
        let (hote, adresse) = (hote.to_string().trim().to_string(), adresse.to_string().trim().to_string());
        if hote.is_empty() || adresse.is_empty() {
            self.as_mut().set_erreur(QString::from("serveur et adresse sont obligatoires"));
            return false;
        }
        let compte = {
            let Some(magasin) = self.magasin.as_ref() else {
                self.as_mut().set_erreur(QString::from("le profil n'est pas ouvert"));
                return false;
            };
            let nouveau = matches!(magasin.compte_connu(&adresse), Ok(None));
            match magasin.compte(&adresse, &hote, PORT_IMAPS, &adresse) {
                Ok(id) => {
                    let present = magasin.compte_par_id(id).ok().flatten();
                    let hote_envoi = present.as_ref().map(|c| c.hote_envoi.clone()).unwrap_or_default();
                    let service = present.and_then(|c| c.fournisseur());
                    (id, nouveau, hote_envoi, service)
                }
                Err(e) => {
                    let message = QString::from(&format!("index local : {e}"));
                    self.as_mut().set_erreur(message);
                    return false;
                }
            }
        };
        let (id, nouveau, hote_envoi, service) = compte;
        // Compte à jetons (Microsoft, Google) : ce que l'interface tient du
        // coffre est le jeton de rafraîchissement, pas un mot de passe.
        let (mot_de_passe, oauth) = match service {
            Some(f) => (String::new(), Some(crate::oauth::partager(f, &mot_de_passe.to_string()))),
            None => (mot_de_passe.to_string(), None),
        };
        {
            let mut noyau = self.as_mut().rust_mut();
            if nouveau {
                noyau.nouveaux.insert(id);
            }
            noyau.identites.insert(id, Identifiants { hote, hote_envoi, utilisateur: adresse, mot_de_passe, oauth });
            noyau.authentifies.remove(&id);
        }
        self.as_mut().lancer(id, Vec::new())
    }

    pub fn microsoft_disponible(&self) -> bool {
        crate::microsoft::disponible()
    }

    pub fn consentement_microsoft(&self) -> QString {
        QString::from(&crate::microsoft::url_consentement(&crate::microsoft::identifiant_client()))
    }

    pub fn connecter_microsoft(mut self: Pin<&mut Self>, indice: &QString) -> QString {
        use crate::microsoft as ms;
        let client = ms::identifiant_client();
        if client.is_empty() {
            self.as_mut().set_erreur(QString::from(
                "connexion Microsoft indisponible : MMail n'a pas d'identifiant d'application Microsoft",
            ));
            return QString::default();
        }
        let indice = indice.to_string();
        // Sous Android, MMail est gelé en arrière-plan pendant que le navigateur
        // montre la connexion : l'écoute locale ne répond pas (retour de Manu
        // du 18/10). La connexion se fait dans une page intégrée à MMail, qui
        // intercepte le retour et le remet à terminer_microsoft.
        let (pkce, etat) = match crate::oauth::pkce_et_etat() {
            Ok(v) => v,
            Err(x) => {
                self.as_mut().set_erreur(QString::from(&x.to_string()));
                return QString::default();
            }
        };
        if cfg!(target_os = "android") {
            let url = ms::url_autorisation(&client, ms::REDIRECTION_ANDROID, &pkce.defi, &etat, &indice);
            self.as_mut().rust_mut().attente_microsoft = Some((client, etat, pkce.verificateur, indice));
            self.as_mut().set_erreur(QString::from(""));
            return QString::from(&url);
        }
        let ecoute = match ms::ecouter() {
            Ok(e) => e,
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("écoute locale : {e}")));
                return QString::default();
            }
        };
        let url = ms::url_autorisation(&client, &ecoute.redirection, &pkce.defi, &etat, &indice);
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-microsoft".into()).spawn(move || {
            let issue = ms::attendre_code(&ecoute, &etat, ms::DELAI_CONNEXION)
                .and_then(|code| ms::echanger_code(&client, &code, &pkce.verificateur, &ecoute.redirection, &indice))
                .and_then(|r| connexion_aboutie(r, "Microsoft"));
            let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| boite.accueillir_microsoft(issue));
        });
        if let Err(e) = lance {
            self.as_mut().set_erreur(QString::from(&format!("connexion Microsoft : {e}")));
            return QString::default();
        }
        self.as_mut().set_erreur(QString::from(""));
        QString::from(&url)
    }

    pub fn terminer_microsoft(mut self: Pin<&mut Self>, url: &QString) -> bool {
        let url = url.to_string();
        let Some((client, etat, verificateur, adresse)) = self.attente_microsoft.clone() else {
            return false;
        };
        let code = match crate::oauth::lire_retour(&url, &etat) {
            None => return false,
            Some(Ok(code)) => code,
            Some(Err(e)) => {
                self.as_mut().rust_mut().attente_microsoft = None;
                self.as_mut().accueillir_microsoft(Err(e));
                return true;
            }
        };
        self.as_mut().rust_mut().attente_microsoft = None;
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-microsoft".into()).spawn(move || {
            let issue = crate::microsoft::echanger_code(&client, &code, &verificateur, crate::microsoft::REDIRECTION_ANDROID, &adresse)
                .and_then(|r| connexion_aboutie(r, "Microsoft"));
            let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| boite.accueillir_microsoft(issue));
        });
        lance.is_ok()
    }

    /// Issue de la connexion Microsoft, sur le fil Qt : le compte est
    /// enregistré — ou converti, s'il existait avec un mot de passe — puis
    /// connecté.
    fn accueillir_microsoft(mut self: Pin<&mut Self>, issue: Result<ConnexionOAuth, crate::microsoft::ErreurOAuth>) {
        let c = match issue {
            Ok(c) => c,
            Err(e) => {
                self.as_mut().microsoft_refuse(&QString::from(&e.to_string()), e.consentement());
                return;
            }
        };
        match self.as_mut().enregistrer_oauth(crate::oauth::Fournisseur::Microsoft, &c) {
            Ok(()) => self.as_mut().microsoft_connecte(
                &QString::from(&c.adresse),
                &QString::from(crate::microsoft::HOTE_IMAP),
                &QString::from(&c.rafraichissement),
            ),
            Err(message) => self.as_mut().microsoft_refuse(&QString::from(&message), false),
        }
    }

    pub fn google_disponible(&self) -> bool {
        crate::google::disponible()
    }

    pub fn dossier_existe(&self, url: &QString) -> bool {
        let url = url.to_string();
        let chemin = match crate::rendu::chemin_de_url(&url) {
            Some(chemin) => chemin,
            None => {
                // « file://serveur/partage/… » : un partage réseau, sous Windows
                // « \\serveur\partage\… ».
                let partage = url
                    .get(..7)
                    .filter(|p| p.eq_ignore_ascii_case("file://"))
                    .and_then(|_| crate::rendu::decoder_pourcent(&url[7..]))
                    .filter(|reste| !reste.is_empty() && !reste.starts_with('/'));
                match partage {
                    Some(reste) if cfg!(windows) => PathBuf::from(format!(r"\\{}", reste.replace('/', r"\"))),
                    _ => return false,
                }
            }
        };
        dossier_present(chemin)
    }

    pub fn google_ajout_ouvert(&self) -> bool {
        crate::google::ajout_ouvert()
    }

    pub fn connecter_google(mut self: Pin<&mut Self>, indice: &QString) -> QString {
        use crate::google as g;
        let (client, secret) = (g::identifiant_client(), g::secret_client());
        if client.is_empty() || secret.is_empty() {
            self.as_mut().set_erreur(QString::from("connexion Google indisponible : MMail n'a pas de client Google"));
            return QString::default();
        }
        let ecoute = match g::ecouter() {
            Ok(e) => e,
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("écoute locale : {e}")));
                return QString::default();
            }
        };
        let (pkce, etat) = match crate::oauth::pkce_et_etat() {
            Ok(v) => v,
            Err(x) => {
                self.as_mut().set_erreur(QString::from(&x.texte("Google")));
                return QString::default();
            }
        };
        let url = g::url_autorisation(&client, &ecoute.redirection, &pkce.defi, &etat, &indice.to_string());
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-google".into()).spawn(move || {
            let issue = g::attendre_code(&ecoute, &etat, crate::oauth::DELAI_CONNEXION)
                .and_then(|code| g::echanger_code(&client, &secret, &code, &pkce.verificateur, &ecoute.redirection))
                .and_then(g::verifier_portee)
                .and_then(|r| connexion_aboutie(r, "Google"));
            let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| boite.accueillir_google(issue));
        });
        if let Err(e) = lance {
            self.as_mut().set_erreur(QString::from(&format!("connexion Google : {e}")));
            return QString::default();
        }
        self.as_mut().set_erreur(QString::from(""));
        QString::from(&url)
    }

    /// Issue de la connexion Google, sur le fil Qt : comme pour Microsoft.
    fn accueillir_google(mut self: Pin<&mut Self>, issue: Result<ConnexionOAuth, crate::oauth::ErreurOAuth>) {
        let c = match issue {
            Ok(c) => c,
            Err(e) => {
                self.as_mut().google_refuse(&QString::from(&crate::google::message_refus(&e)));
                return;
            }
        };
        match self.as_mut().enregistrer_oauth(crate::oauth::Fournisseur::Google, &c) {
            Ok(()) => self.as_mut().google_connecte(
                &QString::from(&c.adresse),
                &QString::from(crate::google::HOTE_IMAP),
                &QString::from(&c.rafraichissement),
            ),
            Err(message) => self.as_mut().google_refuse(&QString::from(&message)),
        }
    }

    /// Enregistre le compte d'une connexion OAuth2 aboutie — ou le convertit,
    /// s'il existait avec un mot de passe —, garde ses jetons et le connecte.
    fn enregistrer_oauth(mut self: Pin<&mut Self>, service: crate::oauth::Fournisseur, c: &ConnexionOAuth) -> Result<(), String> {
        let hote = service.hote_imap();
        let (id, nouveau) = match self.magasin.as_ref() {
            None => return Err("le profil n'est pas ouvert".to_string()),
            Some(magasin) => {
                let nouveau = matches!(magasin.compte_connu(&c.adresse), Ok(None));
                magasin
                    .compte(&c.adresse, hote, PORT_IMAPS, &c.adresse)
                    .and_then(|id| magasin.poser_auth(&c.adresse, service.auth()).map(|_| id))
                    .map(|id| (id, nouveau))
                    .map_err(|e| format!("index local : {e}"))?
            }
        };
        let jetons = crate::oauth::partager(service, &c.rafraichissement);
        crate::oauth::retenir(&jetons, service.portee_imap(), &c.acces, c.duree);
        {
            let mut noyau = self.as_mut().rust_mut();
            if nouveau {
                noyau.nouveaux.insert(id);
            }
            noyau.identites.insert(
                id,
                Identifiants {
                    hote: hote.into(),
                    hote_envoi: String::new(),
                    utilisateur: c.adresse.clone(),
                    mot_de_passe: String::new(),
                    oauth: Some(jetons),
                },
            );
            noyau.authentifies.remove(&id);
        }
        self.as_mut().lancer(id, Vec::new());
        Ok(())
    }

    /// Jetons de rafraîchissement que Microsoft a renouvelés pendant le
    /// travail des fils : l'interface les confie au coffre.
    fn conserver_jetons(mut self: Pin<&mut Self>) {
        let renouveles: Vec<(String, String, String)> = self
            .identites
            .values()
            .filter_map(|i| {
                let jeton = crate::oauth::a_conserver(i.oauth.as_ref()?)?;
                Some((i.utilisateur.clone(), i.hote.clone(), jeton))
            })
            .collect();
        for (adresse, hote, jeton) in renouveles {
            self.as_mut().jeton_renouvele(&QString::from(&adresse), &QString::from(&hote), &QString::from(&jeton));
        }
    }

    pub fn poser_serveur_envoi(mut self: Pin<&mut Self>, adresse: &QString, hote: &QString) -> bool {
        let (adresse, hote) = (adresse.to_string().trim().to_string(), hote.to_string().trim().to_ascii_lowercase());
        let valide = hote.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if !valide {
            self.as_mut().set_erreur(QString::from(&format!("serveur d'envoi invalide : {hote}")));
            return false;
        }
        let pose = match self.magasin.as_ref() {
            Some(magasin) => magasin.poser_serveur_envoi(&adresse, &hote),
            None => {
                self.as_mut().set_erreur(QString::from("le profil n'est pas ouvert"));
                return false;
            }
        };
        match pose {
            Ok(true) => {
                let mut noyau = self.as_mut().rust_mut();
                for id in noyau.identites.values_mut().filter(|i| i.utilisateur == adresse) {
                    id.hote_envoi = hote.clone();
                }
                true
            }
            Ok(false) => {
                self.as_mut().set_erreur(QString::from(&format!("compte inconnu : {adresse}")));
                false
            }
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("index local : {e}")));
                false
            }
        }
    }

    pub fn deconnecter(mut self: Pin<&mut Self>, compte: i32) {
        let compte = compte as i64;
        self.as_mut().fermer_session(compte);
        self.as_mut().rust_mut().etats.insert(compte, Etat::HorsLigne);
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
    }

    pub fn retirer_compte(mut self: Pin<&mut Self>, compte: i32) {
        let id = compte as i64;
        // Compte Google connecté : son jeton est révoqué chez Google, qui ne
        // garde pas un accès que plus rien n'utilise. (Microsoft n'offre pas
        // de révocation d'un jeton à une application publique.)
        let a_revoquer = self
            .identites
            .get(&id)
            .and_then(|i| i.oauth.clone())
            .filter(|p| crate::oauth::fournisseur(p) == crate::oauth::Fournisseur::Google)
            .map(|p| crate::oauth::verrouiller(&p).rafraichissement.clone())
            .filter(|j| !j.is_empty());
        if let Some(jeton) = a_revoquer {
            let _ = thread::Builder::new().name("mmail-revocation".into()).spawn(move || {
                if let Err(e) = crate::google::revoquer(&jeton) {
                    eprintln!("MMail : révocation du jeton Google : {}", e.texte("Google"));
                }
            });
        }
        self.as_mut().fermer_session(id);
        {
            let mut noyau = self.as_mut().rust_mut();
            noyau.identites.remove(&id);
            noyau.etats.remove(&id);
            noyau.nouveaux.remove(&id);
            noyau.authentifies.remove(&id);
        }
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.retirer_compte(id);
        }
        if self.courant.as_ref().map(|c| c.0) == Some(id) {
            self.as_mut().poser_courant(None);
        }
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
    }

    pub fn arborescence(&self, masques: bool) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let comptes = magasin.comptes().unwrap_or_default();
        let adresses: HashMap<i64, String> =
            comptes.iter().map(|c| (c.id, c.adresse.clone())).collect();
        let mut lignes: Vec<String> = Vec::new();

        // La rubrique Favoris est toujours là, même vide : c'est une cible de
        // dépôt pour les dossiers qu'on y glisse (décision 3).
        let favoris = magasin.favoris().unwrap_or_default();
        lignes.push(r#"{"genre":"rubrique","nom":"Favoris"}"#.to_string());
        if favoris.is_empty() {
            lignes.push(r#"{"genre":"favori-vide"}"#.to_string());
        }
        for d in &favoris {
            let adresse = adresses.get(&d.compte_id).cloned().unwrap_or_default();
            lignes.push(json_dossier("favori", d, &adresse, 0, false));
        }
        for c in &comptes {
            let motif = if self.etat(c.id).code() == "erreur" {
                self.motifs.get(&c.id).cloned().unwrap_or_default()
            } else {
                String::new()
            };
            let partagees: Vec<String> = c.partagees.iter().map(|a| texte_json(a)).collect();
            lignes.push(format!(
                r#"{{"genre":"compte","compte":{},"adresse":{},"hote":{},"etat":{},"replie":{},"motif":{},"partagees":[{}]}}"#,
                c.id,
                texte_json(&c.adresse),
                texte_json(&c.hote),
                texte_json(self.etat(c.id).code()),
                c.replie,
                texte_json(&motif),
                partagees.join(",")
            ));
            if c.replie {
                continue;
            }
            let tous = magasin.dossiers(c.id).unwrap_or_default();
            let visibles = if masques { tous } else { sans_masques(tous) };
            for (d, enfants) in arbre_visible(&visibles) {
                lignes.push(json_dossier("dossier", d, &c.adresse, d.profondeur, enfants));
            }
        }
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn dossiers_cibles(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let mut lignes = Vec::new();
        for c in magasin.comptes().unwrap_or_default() {
            if self.etat(c.id) != Etat::Pret {
                continue;
            }
            for d in magasin.dossiers(c.id).unwrap_or_default() {
                if !d.selectionnable {
                    continue;
                }
                lignes.push(json_dossier("cible", &d, &c.adresse, d.profondeur, false));
            }
        }
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn placer_compte(mut self: Pin<&mut Self>, compte: i32, avant: i32) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.placer_compte(compte as i64, (avant > 0).then_some(avant as i64));
        }
        self.as_mut().reviser();
    }

    pub fn decaler_compte(mut self: Pin<&mut Self>, compte: i32, sens: i32) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.decaler_compte(compte as i64, sens);
        }
        self.as_mut().reviser();
    }

    pub fn replier_compte(mut self: Pin<&mut Self>, compte: i32, replie: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.replier_compte(compte as i64, replie);
        }
        self.as_mut().reviser();
    }

    pub fn replier_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, replie: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let _ = magasin.replier_dossier(id, replie);
            }
        }
        self.as_mut().reviser();
    }

    pub fn masquer_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, masque: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let _ = magasin.masquer(id, masque);
            }
        }
        self.as_mut().reviser();
    }

    pub fn placer_favori(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        compte_avant: i32,
        chemin_avant: &QString,
    ) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let avant = if chemin_avant.is_empty() {
                    None
                } else {
                    magasin.dossier_id(compte_avant as i64, &chemin_avant.to_string()).ok()
                };
                let _ = magasin.placer_favori(id, avant);
            }
        }
        self.as_mut().reviser();
    }

    pub fn epingler_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, favori: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let _ = magasin.epingler(id, favori);
            }
        }
        self.as_mut().reviser();
    }

    pub fn ouvrir_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString) -> bool {
        let compte = compte as i64;
        let chemin = chemin.to_string();
        let Some(id) = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok()) else {
            self.as_mut().set_erreur(QString::from(&format!("dossier inconnu : {chemin}")));
            return false;
        };
        self.as_mut().poser_courant(Some((compte, chemin.clone(), id)));
        // Hors connexion, la liste de l'index s'affiche quand même, et les
        // messages gardés sur le poste s'ouvrent.
        if !self.as_mut().envoyer(compte, Commande::OuvrirDossier(chemin)) {
            self.as_mut().set_erreur(QString::from("compte hors ligne : messages gardés sur le poste seulement"));
        }
        true
    }

    pub fn messages(&self) -> QString {
        let (Some(magasin), Some((_, _, id))) = (self.magasin.as_ref(), self.courant.as_ref()) else {
            return QString::from("[]");
        };
        match magasin.messages(*id) {
            Ok(messages) => QString::from(&json_messages(&messages)),
            Err(_) => QString::from("[]"),
        }
    }

    pub fn changements_liste(mut self: Pin<&mut Self>, complet: bool) -> QString {
        let (Some(magasin), Some((_, _, id))) = (self.magasin.as_ref(), self.courant.as_ref()) else {
            return QString::from(r#"{"complet":[],"suite":false}"#);
        };
        let id = *id;
        // Lue avant les messages : une écriture qui tombe entre les deux
        // laisse une version plus ancienne que la liste, et le rafraîchissement
        // suivant relit — jamais l'inverse.
        let version = magasin.version_dossier(id).unwrap_or(-1);
        if let Some(liste) = self.liste.as_ref().filter(|l| l.dossier == id && !complet) {
            if version >= 0 && liste.version == version {
                return QString::from(AUCUN_CHANGEMENT);
            }
            let fin = liste.fin;
            let apres = magasin.messages_jusqua(id, fin).unwrap_or_default();
            let empreintes = empreintes_de(&apres);
            if let Some(json) = changements(&liste.empreintes, &apres, &empreintes) {
                self.as_mut().rust_mut().liste = Some(ListeEnvoyee { dossier: id, empreintes, fin, version });
                return QString::from(&json);
            }
        }
        // La liste entière — sa première page, la suite par `suiteListe`.
        let page = magasin.messages_page(id, None, PREMIERE_PAGE).unwrap_or_default();
        let fin = (page.len() == PREMIERE_PAGE).then(|| page.last().map(|m| (m.horodatage, m.uid))).flatten();
        let json = format!(r#"{{"complet":{},"suite":{}}}"#, json_messages(&page), fin.is_some());
        self.as_mut().rust_mut().liste = Some(ListeEnvoyee { dossier: id, empreintes: empreintes_de(&page), fin, version });
        QString::from(&json)
    }

    pub fn suite_liste(mut self: Pin<&mut Self>, nombre: i32) -> QString {
        const FINI: &str = r#"{"lignes":[],"suite":false}"#;
        let (Some(magasin), Some((_, _, id))) = (self.magasin.as_ref(), self.courant.as_ref()) else {
            return QString::from(FINI);
        };
        let id = *id;
        let Some(fin) = self.liste.as_ref().filter(|l| l.dossier == id).and_then(|l| l.fin) else {
            return QString::from(FINI);
        };
        let nombre = nombre.max(1) as usize;
        let page = magasin.messages_page(id, Some(fin), nombre).unwrap_or_default();
        let fin = (page.len() == nombre).then(|| page.last().map(|m| (m.horodatage, m.uid))).flatten();
        let json = format!(r#"{{"lignes":{},"suite":{}}}"#, json_messages(&page), fin.is_some());
        if let Some(liste) = self.as_mut().rust_mut().liste.as_mut() {
            liste.empreintes.extend(empreintes_de(&page));
            liste.fin = fin;
        }
        QString::from(&json)
    }

    pub fn chercher(&self, texte: &QString, partout: bool) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let (texte, categories) = criteres_recherche(magasin, &texte.to_string());
        if partout {
            return QString::from(&json_trouves(
                &magasin.chercher_avec(None, &texte, &categories, RECHERCHE_MAX).unwrap_or_default(),
            ));
        }
        let Some((_, _, id)) = self.courant.as_ref() else {
            return QString::from("[]");
        };
        let trouves = magasin.chercher_avec(Some(*id), &texte, &categories, RECHERCHE_MAX).unwrap_or_default();
        let messages: Vec<MessageLocal> = trouves.into_iter().map(|t| t.message).collect();
        QString::from(&json_messages(&messages))
    }

    pub fn chercher_sur_le_serveur(mut self: Pin<&mut Self>, texte: &QString) -> bool {
        // Une nouvelle recherche remplace la précédente, partout comprise.
        let numero = self.recherche.fetch_add(1, Ordering::SeqCst) + 1;
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        let texte = texte.to_string().trim().to_string();
        if texte.is_empty() || !self.sessions.contains_key(&compte) {
            return false;
        }
        self.as_mut().lancer_recherche(compte, texte, vec![chemin], false, numero)
    }

    pub fn chercher_partout_sur_le_serveur(mut self: Pin<&mut Self>, texte: &QString) -> i32 {
        // Nouvelle recherche ou arrêt : les fils de la précédente s'arrêtent.
        let numero = self.recherche.fetch_add(1, Ordering::SeqCst) + 1;
        let texte = texte.to_string().trim().to_string();
        if texte.is_empty() {
            return 0;
        }
        let mut comptes: Vec<i64> = self.sessions.keys().copied().collect();
        comptes.sort_unstable();
        let mut total = 0;
        for compte in comptes {
            // Tous les dossiers qui contiennent des messages, masqués compris :
            // la boîte de réception d'abord, puis l'ordre de l'arborescence.
            let chemins: Vec<String> = self
                .magasin
                .as_ref()
                .and_then(|m| m.dossiers(compte).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|d| d.selectionnable)
                .map(|d| d.chemin)
                .collect();
            let nombre = chemins.len();
            if nombre > 0 && self.as_mut().lancer_recherche(compte, texte.clone(), chemins, true, numero) {
                total += nombre;
            }
        }
        total as i32
    }

    /// Lance la recherche `numero` dans des dossiers d'un compte, dans son
    /// propre fil et par sa propre connexion (cf. `Recherche`). Faux si le
    /// compte n'a pas d'identifiants en mémoire, ou si le fil n'a pu partir.
    fn lancer_recherche(
        self: Pin<&mut Self>,
        compte: i64,
        texte: String,
        chemins: Vec<String>,
        partout: bool,
        numero: u64,
    ) -> bool {
        let Some(identifiants) = self.identites.get(&compte).cloned() else {
            return false;
        };
        let recherche = Recherche {
            compte,
            texte,
            chemins,
            partout,
            numero,
            courante: self.recherche.clone(),
            patience: PATIENCE_RECHERCHE,
            delai: delai_recherche(),
        };
        let profil = self.profil.clone();
        let fil = Destinataire::Qt(self.qt_thread());
        thread::Builder::new()
            .name(format!("mmail-recherche-{compte}"))
            .spawn(move || {
                let mut remettre = |issue: Issue| fil.remettre_recherche(compte, numero, issue);
                match Magasin::ouvrir(&profil) {
                    Ok(magasin) => recherche.executer(&mut || ouvrir_imap(&identifiants), &magasin, &mut remettre),
                    Err(e) => recherche.sans_index(&format!("index local : {e}"), &mut remettre),
                }
            })
            .is_ok()
    }

    pub fn trouves_par_uid(&self, compte: i32, chemin: &QString, uids: &QString) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) else {
            return QString::from("[]");
        };
        QString::from(&json_trouves(&magasin.trouves_par_uid(id, &lire_uids(uids)).unwrap_or_default()))
    }

    pub fn messages_par_uid(&self, uids: &QString) -> QString {
        let (Some(magasin), Some((_, _, id))) = (self.magasin.as_ref(), self.courant.as_ref()) else {
            return QString::from("[]");
        };
        QString::from(&json_messages(&magasin.messages_par_uid(*id, &lire_uids(uids)).unwrap_or_default()))
    }

    pub fn demander_corps(self: Pin<&mut Self>, uid: i32) -> bool {
        self.demander(uid, false, false)
    }

    pub fn afficher_images(self: Pin<&mut Self>, uid: i32) -> bool {
        self.demander(uid, false, true)
    }

    pub fn demander_source(self: Pin<&mut Self>, uid: i32) -> bool {
        self.demander(uid, true, false)
    }

    pub fn ouvrir_piece(self: Pin<&mut Self>, uid: i32, indice: i32) -> bool {
        self.demander_piece(uid, indice, None, false)
    }

    pub fn demander_dans(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, uid: i32, brut: bool) -> bool {
        let (compte, chemin) = (compte as i64, chemin.to_string());
        let Some(dossier) = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok()) else {
            return false;
        };
        if uid <= 0 {
            return false;
        }
        let uid = uid as u32;
        // Numéro 0 : hors de la suite des affichages du volet de lecture, que
        // cette lecture n'interrompt pas (cf. `rendre_si_actuel`). `lu` vrai :
        // le marquage revient à la fenêtre.
        let local = Local { compte, chemin: chemin.clone(), dossier, uid, lu: true, brut, en_ligne: false };
        let garde = self.magasin.as_ref().and_then(|m| m.garde_sur_le_poste(dossier, uid).ok().flatten());
        if let Some((id, _)) = garde {
            let profil = self.profil.clone();
            let fil = self.qt_thread();
            let lancement = thread::Builder::new().name("mmail-fenetre".into()).spawn(move || {
                let preparation = Cache::du_profil(&profil)
                    .lire(id)
                    .and_then(|octets| rendre_si_actuel(0, &octets, brut, false, compte, &dossier_affichage(&profil), false));
                let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| boite.afficher_du_poste(local, preparation, 0, false));
            });
            if lancement.is_ok() {
                return true;
            }
        }
        self.as_mut().envoyer(compte, Commande::Corps { chemin, uid, brut, distantes: false, numero: 0 })
    }

    pub fn ouvrir_piece_dans(self: Pin<&mut Self>, compte: i32, chemin: &QString, uid: i32, indice: i32) -> bool {
        let designe = self.magasin.as_ref().and_then(|m| {
            let chemin = chemin.to_string();
            m.dossier_id(compte as i64, &chemin).ok().map(|id| (compte as i64, chemin, id))
        });
        self.demander_piece_dans(designe, uid, indice, None, false)
    }

    pub fn enregistrer_piece_dans(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        uid: i32,
        indice: i32,
        url: &QString,
    ) -> bool {
        let fichier = QUrl::from(url).to_local_file().map(|c| c.to_string()).unwrap_or_default();
        if fichier.is_empty() {
            self.as_mut().set_erreur(QString::from("emplacement d'enregistrement invalide"));
            return false;
        }
        let designe = self.magasin.as_ref().and_then(|m| {
            let chemin = chemin.to_string();
            m.dossier_id(compte as i64, &chemin).ok().map(|id| (compte as i64, chemin, id))
        });
        self.demander_piece_dans(designe, uid, indice, Some(PathBuf::from(fichier)), false)
    }

    pub fn joindre_piece(self: Pin<&mut Self>, uid: i32, indice: i32) -> bool {
        self.demander_piece(uid, indice, None, true)
    }

    pub fn enregistrer_piece(mut self: Pin<&mut Self>, uid: i32, indice: i32, url: &QString) -> bool {
        let chemin = QUrl::from(url).to_local_file().map(|c| c.to_string()).unwrap_or_default();
        if chemin.is_empty() {
            self.as_mut().set_erreur(QString::from("emplacement d'enregistrement invalide"));
            return false;
        }
        self.demander_piece(uid, indice, Some(PathBuf::from(chemin)), false)
    }

    pub fn marquer_lu(self: Pin<&mut Self>, uids: &QString, lu: bool) -> bool {
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        self.marquer_lu_dans(compte as i32, &QString::from(&chemin), uids, lu)
    }

    pub fn marquer_lu_dans(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, uids: &QString, lu: bool) -> bool {
        let uids = lire_uids(uids);
        let (compte, chemin) = (compte as i64, chemin.to_string());
        let Some(id) = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok()) else {
            return false;
        };
        if uids.is_empty() {
            return false;
        }
        // L'index d'abord : la liste change tout de suite, le serveur suit.
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.marquer_lu(id, &uids, lu);
        }
        self.as_mut().noter_marquages(id, &uids);
        self.as_mut().reviser();
        self.as_mut().envoyer(compte, Commande::MarquerLu { chemin, uids, lu, discret: false })
    }

    pub fn marquer_lu_differe(mut self: Pin<&mut Self>, uid: i32) -> bool {
        let ouvert = self.courant.as_ref().map(|c| c.2);
        let attente = self.as_mut().rust_mut().lu_differe.take_if(|d| d.uid as i32 == uid && Some(d.dossier) == ouvert);
        let Some(LuDiffere { compte, chemin, dossier, uid, numero }) = attente else {
            return false;
        };
        self.as_mut().marquer_lu_a_l_affichage(compte, chemin, dossier, uid, numero);
        true
    }

    /// Retient que l'utilisateur vient de marquer ces messages : l'affichage en
    /// cours de l'un d'eux, demandé avant, ne le marquera plus « lu ». Sans
    /// cela, « Marquer comme non lu » fait pendant le chargement du message
    /// était défait par le marquage automatique, parti après lui (demande de
    /// Gwenaëlle, 19/10).
    fn noter_marquages(mut self: Pin<&mut Self>, dossier: i64, uids: &[u32]) {
        let numero = AFFICHAGE.load(Ordering::SeqCst);
        let mut r = self.as_mut().rust_mut();
        // Seuls comptent les affichages en cours : la table ne grossit pas.
        if r.marquages.len() > 1000 {
            r.marquages.clear();
        }
        for &uid in uids {
            r.marquages.insert((dossier, uid), numero);
        }
    }

    /// Marque « lu » le message affiché par la demande `numero` — sauf si
    /// l'utilisateur l'a marqué lui-même depuis. L'index et la ligne de la
    /// liste changent tout de suite, sans relire le dossier ; le serveur suit.
    fn marquer_lu_a_l_affichage(mut self: Pin<&mut Self>, compte: i64, chemin: String, dossier: i64, uid: u32, numero: u64) {
        if marque_depuis(&self.marquages, dossier, uid, numero) {
            return;
        }
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.marquer_lu(dossier, &[uid], true);
        }
        self.as_mut().oublier_lignes(dossier, &[uid]);
        self.as_mut().reviser();
        self.as_mut().lus_modifies(&QString::from(&uid.to_string()), true);
        self.as_mut().envoyer(compte, Commande::MarquerLu { chemin, uids: vec![uid], lu: true, discret: true });
    }

    /// Lignes changées dans l'interface sans `changements_liste` (`lus_modifies`) :
    /// la liste envoyée ne sait plus ce qu'elles montrent, le rafraîchissement
    /// suivant les renvoie. Sinon, un message marqué « lu » à l'affichage puis
    /// « non lu » avant la veille suivante gardait sa ligne en « lu » : l'index
    /// disait non lu, comme la liste envoyée, et rien ne semblait à renvoyer.
    fn oublier_lignes(mut self: Pin<&mut Self>, dossier: i64, uids: &[u32]) {
        let mut r = self.as_mut().rust_mut();
        if let Some(liste) = r.liste.as_mut().filter(|l| l.dossier == dossier) {
            liste.oublier(uids);
        }
    }

    pub fn marquer_suivi(mut self: Pin<&mut Self>, uids: &QString, suivi: bool) -> bool {
        let uids = lire_uids(uids);
        let Some((compte, chemin, id)) = self.courant.clone() else {
            return false;
        };
        if uids.is_empty() {
            return false;
        }
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.marquer_suivi(id, &uids, suivi);
        }
        self.as_mut().reviser();
        self.as_mut().envoyer(compte, Commande::MarquerSuivi { chemin, uids, suivi })
    }

    pub fn marquer_suivi_dans(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, uids: &QString, suivi: bool) -> bool {
        let compte = compte as i64;
        let chemin = chemin.to_string();
        let uids = lire_uids(uids);
        if uids.is_empty() {
            return false;
        }
        if let Some(id) = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok()) {
            if let Some(magasin) = self.magasin.as_ref() {
                let _ = magasin.marquer_suivi(id, &uids, suivi);
            }
        }
        self.as_mut().reviser();
        self.as_mut().envoyer(compte, Commande::MarquerSuivi { chemin, uids, suivi })
    }

    /// Compte et chemin désignés par l'interface : `compte` 0, le dossier
    /// ouvert.
    fn dossier_designe(&self, compte: i32, chemin: &QString) -> Option<(i64, String)> {
        if compte == 0 {
            return self.courant.as_ref().map(|(c, chemin, _)| (*c, chemin.clone()));
        }
        Some((compte as i64, chemin.to_string()))
    }

    pub fn categories(&self, compte: i32, chemin: &QString) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("{}");
        };
        let Some((compte, chemin)) = self.dossier_designe(compte, chemin) else {
            return QString::from("{}");
        };
        let boite = boite_categories(magasin, compte, &chemin);
        let (valeur, serveur) = magasin.catalogue(compte, &boite).ok().flatten().unwrap_or_default();
        let catalogue = Catalogue::lire(&valeur);
        let refusees = magasin
            .dossier_id(compte, &chemin)
            .and_then(|id| magasin.sans_mots_cles(id))
            .unwrap_or(false);
        let liste: Vec<serde_json::Value> = catalogue
            .liste()
            .into_iter()
            .map(|c| {
                serde_json::json!({
                    "motCle": c.mot_cle,
                    "nom": c.nom,
                    "couleur": c.couleur,
                    "origine": categories::est_d_origine(&c.mot_cle),
                })
            })
            .collect();
        let json = serde_json::json!({
            "compte": compte,
            "boite": boite,
            "serveur": serveur,
            "refusees": refusees,
            "creees": catalogue.creees(),
            "max": categories::MAX_CREEES,
            "liste": liste,
        });
        QString::from(&json.to_string())
    }

    pub fn marquer_categorie(self: Pin<&mut Self>, uids: &QString, mot_cle: &QString, present: bool) -> bool {
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        self.marquer_categorie_de(compte, chemin, lire_uids(uids), mot_cle.to_string(), present)
    }

    pub fn marquer_categorie_dans(
        self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        uids: &QString,
        mot_cle: &QString,
        present: bool,
    ) -> bool {
        self.marquer_categorie_de(compte as i64, chemin.to_string(), lire_uids(uids), mot_cle.to_string(), present)
    }

    fn marquer_categorie_de(
        mut self: Pin<&mut Self>,
        compte: i64,
        chemin: String,
        uids: Vec<u32>,
        mot_cle: String,
        present: bool,
    ) -> bool {
        if uids.is_empty() || !crate::protocole::mot_cle_valide(&mot_cle) {
            return false;
        }
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte, &chemin) {
                if magasin.sans_mots_cles(id).unwrap_or(false) {
                    self.as_mut().set_erreur(QString::from("ce serveur ne garde pas les catégories"));
                    return false;
                }
                let _ = magasin.marquer_categorie(id, &uids, &mot_cle, present);
            }
        }
        self.as_mut().reviser();
        self.as_mut().envoyer(compte, Commande::MarquerCategorie { chemin, uids, mot_cle, present })
    }

    pub fn poser_categorie(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        mot_cle: &QString,
        nom: &QString,
        couleur: &QString,
    ) -> QString {
        let Some((compte, chemin)) = self.dossier_designe(compte, chemin) else {
            return QString::from("");
        };
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("");
        };
        let boite = boite_categories(magasin, compte, &chemin);
        let (valeur, serveur) = magasin.catalogue(compte, &boite).ok().flatten().unwrap_or_default();
        let catalogue = Catalogue::lire(&valeur);
        let mut mot_cle = mot_cle.to_string();
        if mot_cle.is_empty() {
            let pris: Vec<String> = catalogue.liste().into_iter().map(|c| c.mot_cle).collect();
            mot_cle = categories::mot_cle_pour(&nom.to_string(), &pris);
        }
        let changement = Changement::Poser(Categorie {
            mot_cle: mot_cle.clone(),
            nom: nom.to_string().trim().to_string(),
            couleur: couleur.to_string().to_ascii_uppercase(),
        });
        if self.as_mut().changer_catalogue(compte, boite, catalogue, serveur, changement) {
            QString::from(&mot_cle)
        } else {
            QString::from("")
        }
    }

    pub fn retirer_categorie(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, mot_cle: &QString) -> bool {
        let Some((compte, chemin)) = self.dossier_designe(compte, chemin) else {
            return false;
        };
        let Some(magasin) = self.magasin.as_ref() else {
            return false;
        };
        let boite = boite_categories(magasin, compte, &chemin);
        let (valeur, serveur) = magasin.catalogue(compte, &boite).ok().flatten().unwrap_or_default();
        let catalogue = Catalogue::lire(&valeur);
        self.as_mut()
            .changer_catalogue(compte, boite, catalogue, serveur, Changement::Retirer(mot_cle.to_string()))
    }

    /// Applique un changement au catalogue rangé — l'interface le voit tout de
    /// suite —, puis le confie au fil du compte, qui le reporte sur le
    /// serveur. Hors connexion, le serveur reprend la main à la relecture
    /// suivante : le changement n'y est pas.
    fn changer_catalogue(
        mut self: Pin<&mut Self>,
        compte: i64,
        boite: String,
        mut catalogue: Catalogue,
        serveur: bool,
        changement: Changement,
    ) -> bool {
        if let Err(motif) = catalogue.appliquer(&changement) {
            self.as_mut().set_erreur(QString::from(&motif));
            return false;
        }
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.poser_catalogue(compte, &boite, &catalogue.ecrire(), serveur);
        }
        self.as_mut().verifier_catalogues();
        if self.sessions.contains_key(&compte) || self.identites.contains_key(&compte) {
            self.as_mut().envoyer(compte, Commande::Catalogue { boite, changement });
        }
        true
    }

    pub fn repondre_confirmation(mut self: Pin<&mut Self>, uid: i32, envoyer: bool) -> bool {
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        let identifiants = if envoyer {
            match self.identites.get(&compte).cloned() {
                Some(i) => Some(i),
                None => {
                    self.as_mut().set_erreur(QString::from("ce compte n'est pas connecté"));
                    return false;
                }
            }
        } else {
            None
        };
        self.as_mut().envoyer(compte, Commande::Confirmer { chemin, uid: uid as u32, identifiants })
    }

    pub fn envoyer_differes(mut self: Pin<&mut Self>) {
        let comptes: Vec<(i64, Identifiants)> = self
            .identites
            .iter()
            .filter(|(c, _)| self.sessions.contains_key(c))
            .map(|(c, i)| (*c, i.clone()))
            .collect();
        for (compte, identifiants) in comptes {
            self.as_mut().envoyer(compte, Commande::EnvoyerDifferes { identifiants });
        }
    }

    pub fn deplacer(self: Pin<&mut Self>, uids: &QString, compte_cible: i32, chemin_cible: &QString) -> bool {
        let Some((compte, source, _)) = self.courant.clone() else {
            return false;
        };
        self.deplacer_dans(compte as i32, &QString::from(&source), uids, compte_cible, chemin_cible)
    }

    pub fn deplacer_dans(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        uids: &QString,
        compte_cible: i32,
        chemin_cible: &QString,
    ) -> bool {
        let uids = lire_uids(uids);
        let (compte, source) = (compte as i64, chemin.to_string());
        let cible_compte = compte_cible as i64;
        let chemin_cible = chemin_cible.to_string();
        if uids.is_empty() || (cible_compte == compte && chemin_cible == source) {
            return false;
        }
        let identifiants = if cible_compte == compte {
            None
        } else {
            match self.identites.get(&cible_compte) {
                Some(i) if self.etat(cible_compte) == Etat::Pret => Some(i.clone()),
                _ => {
                    self.as_mut().set_erreur(QString::from(
                        "la boîte cible n'est pas connectée : le déplacement est refusé",
                    ));
                    return false;
                }
            }
        };
        let cible = Cible { compte: cible_compte, chemin: chemin_cible, identifiants };
        self.as_mut().envoyer(compte, Commande::Deplacer { source, uids, cible })
    }

    pub fn supprimer(self: Pin<&mut Self>, uids: &QString) -> bool {
        let Some((compte, source, _)) = self.courant.clone() else {
            return false;
        };
        self.supprimer_dans(compte as i32, &QString::from(&source), uids)
    }

    pub fn supprimer_dans(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, uids: &QString) -> bool {
        let compte = compte as i64;
        let source = chemin.to_string();
        let uids = lire_uids(uids);
        if uids.is_empty() {
            return false;
        }
        let corbeille =
            self.magasin.as_ref().and_then(|m| dossier_de_role(&m.dossiers(compte).ok()?, "Trash"));
        match corbeille {
            Some(corbeille) if corbeille != source => {
                let cible = Cible { compte, chemin: corbeille, identifiants: None };
                self.as_mut().envoyer(compte, Commande::Deplacer { source, uids, cible })
            }
            Some(_) => {
                self.as_mut().set_erreur(QString::from("ces messages sont déjà dans la corbeille"));
                false
            }
            None => {
                self.as_mut().set_erreur(QString::from("ce compte n'a pas de corbeille déclarée"));
                false
            }
        }
    }

    pub fn signaler_indesirable_dans(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, uids: &QString) -> bool {
        let compte = compte as i64;
        let source = chemin.to_string();
        let uids = lire_uids(uids);
        if uids.is_empty() {
            return false;
        }
        let dossiers = self.magasin.as_ref().and_then(|m| m.dossiers(compte).ok()).unwrap_or_default();
        let Some(indesirables) = dossier_indesirables(&dossiers) else {
            self.as_mut().set_erreur(QString::from("ce compte n'a pas de dossier de courrier indésirable"));
            return false;
        };
        let chemin = if source == indesirables { "INBOX".to_string() } else { indesirables };
        let cible = Cible { compte, chemin, identifiants: None };
        self.as_mut().envoyer(compte, Commande::Deplacer { source, uids, cible })
    }

    pub fn carnet(&self, filtre: &QString) -> QString {
        let fiches = self
            .magasin
            .as_ref()
            .and_then(|m| m.fiches_contacts(&filtre.to_string(), 500).ok())
            .unwrap_or_default();
        let liste = |v: &[String]| format!("[{}]", v.iter().map(|t| texte_json(t)).collect::<Vec<_>>().join(","));
        let lignes: Vec<String> = fiches
            .iter()
            .map(|f| {
                format!(
                    r#"{{"nom":{},"organisation":{},"courriels":{},"telephones":{},"carnet":{},"compte":{}}}"#,
                    texte_json(&f.nom),
                    texte_json(&f.organisation),
                    liste(&f.courriels),
                    liste(&f.telephones),
                    texte_json(&f.carnet),
                    texte_json(&f.compte)
                )
            })
            .collect();
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn filtres(&self) -> QString {
        QString::from(&self.magasin.as_ref().and_then(|m| m.filtres().ok()).unwrap_or_default())
    }

    pub fn poser_filtres(mut self: Pin<&mut Self>, json: &QString) -> bool {
        let json = json.to_string();
        if let Err(e) = crate::filtres::lire(&json) {
            self.as_mut().set_erreur(QString::from(&e));
            return false;
        }
        match self.magasin.as_ref().map(|m| m.poser_filtres(&json)) {
            Some(Ok(())) => true,
            _ => {
                self.as_mut().set_erreur(QString::from("index local indisponible : filtres non enregistrés"));
                false
            }
        }
    }

    pub fn appliquer_filtres(self: Pin<&mut Self>, compte: i32) -> i32 {
        self.filtrer(compte as i64, "INBOX", true)
    }

    /// Passe aux filtres les messages du dossier `chemin` du compte arrivés
    /// depuis leur dernier passage — tous si `tout`. La première fois (ou
    /// après une rupture d'`UIDVALIDITY`), ce qui est déjà là est seulement
    /// noté : les filtres ne valent que pour les arrivées, sauf demande. Rend
    /// le nombre de messages touchés.
    fn filtrer(mut self: Pin<&mut Self>, compte: i64, chemin: &str, tout: bool) -> i32 {
        let Some(magasin) = self.magasin.as_ref() else {
            return 0;
        };
        // Appelé à chaque relecture de la boîte de réception : rien n'est lu
        // de l'index au-delà de l'UID le plus haut tant qu'il n'a pas bougé.
        let etat = magasin.filtres_etat(compte, chemin).ok().flatten();
        let Ok((uidvalidity, dernier)) = magasin.uid_max(compte, chemin) else {
            return 0;
        };
        if uidvalidity == 0 {
            return 0;
        }
        let deja_lu = etat.filter(|(v, _)| *v == uidvalidity).map(|(_, u)| u);
        let apres = match deja_lu {
            _ if tout => 0,
            Some(uid) => uid,
            None => {
                let _ = magasin.poser_filtres_etat(compte, chemin, uidvalidity, dernier);
                return 0;
            }
        };
        if dernier <= apres {
            return 0;
        }
        if dernier > deja_lu.unwrap_or(0) {
            let _ = magasin.poser_filtres_etat(compte, chemin, uidvalidity, dernier);
        }
        let filtres = crate::filtres::lire(&magasin.filtres().unwrap_or_default()).unwrap_or_default();
        let Ok((_, recus)) = magasin.recus_apres(compte, chemin, apres) else {
            return 0;
        };
        if recus.is_empty() {
            return 0;
        }
        let mut plan = crate::filtres::Plan::default();
        let adresse =
            magasin.comptes().ok().and_then(|cs| cs.into_iter().find(|c| c.id == compte)).map(|c| c.adresse);
        if let (true, Some(adresse)) = (filtres.iter().any(|f| f.actif), adresse) {
            let ids: Vec<String> = recus.iter().map(|r| r.message_id.clone()).collect();
            let vus = magasin.filtres_vus(compte, &ids).unwrap_or_default();
            plan = crate::filtres::planifier(&filtres, &adresse, &recus, &|id| vus.contains(id));
            if !plan.vide() {
                let _ = magasin.noter_filtres_vus(compte, &plan.touches);
            }
        }
        // Les arrivées à signaler (0.6.2) — pas pour « appliquer à toute la
        // boîte » : le nombre, l'expéditeur et l'objet du dernier.
        let signal = if tout {
            None
        } else {
            let arrivees = crate::filtres::a_signaler(&recus, &plan);
            arrivees.last().map(|d| {
                let qui = if d.expediteur.is_empty() { d.adresse.clone() } else { d.expediteur.clone() };
                (arrivees.len() as i32, qui, d.sujet.clone())
            })
        };
        if plan.vide() {
            if let Some((nombre, qui, sujet)) = signal {
                self.as_mut().nouveau_courrier(compte as i32, nombre, &QString::from(&qui), &QString::from(&sujet));
            }
            return 0;
        }
        let dossiers = magasin.dossiers(compte).unwrap_or_default();
        let corbeille = dossier_de_role(&dossiers, "Trash");
        let indesirables = dossier_indesirables(&dossiers).map(|j| if j == chemin { "INBOX".to_string() } else { j });
        let touches = plan.touches.len() as i32;
        let source = chemin.to_string();
        if !plan.lus.is_empty() {
            self.as_mut().envoyer(compte, Commande::MarquerLu { chemin: source.clone(), uids: plan.lus, lu: true, discret: false });
        }
        if !plan.suivis.is_empty() {
            self.as_mut().envoyer(compte, Commande::MarquerSuivi { chemin: source.clone(), uids: plan.suivis, suivi: true });
        }
        let mut destinations: Vec<(String, Vec<u32>)> = plan.deplacements.into_iter().collect();
        if let Some(c) = corbeille {
            destinations.push((c, plan.corbeille));
        }
        if let Some(j) = indesirables {
            destinations.push((j, plan.indesirables));
        }
        let mut lances = 0;
        for (cible, uids) in destinations {
            if uids.is_empty() || cible == source {
                continue;
            }
            let cible = Cible { compte, chemin: cible, identifiants: None };
            if self.as_mut().envoyer(compte, Commande::Deplacer { source: source.clone(), uids, cible }) {
                lances += 1;
            }
        }
        self.as_mut().filtres_appliques(compte as i32, touches, lances);
        if let Some((nombre, qui, sujet)) = signal {
            self.as_mut().nouveau_courrier(compte as i32, nombre, &QString::from(&qui), &QString::from(&sujet));
        }
        touches
    }

    pub fn signaler_indesirable(mut self: Pin<&mut Self>, uids: &QString) -> bool {
        let Some((compte, source, _)) = self.courant.clone() else {
            return false;
        };
        let dossiers = self.magasin.as_ref().and_then(|m| m.dossiers(compte).ok()).unwrap_or_default();
        let Some(indesirables) = dossier_indesirables(&dossiers) else {
            self.as_mut().set_erreur(QString::from("ce compte n'a pas de dossier de courrier indésirable"));
            return false;
        };
        let cible = if source == indesirables { "INBOX".to_string() } else { indesirables };
        self.deplacer(uids, compte as i32, &QString::from(&cible))
    }

    pub fn vider_corbeilles(mut self: Pin<&mut Self>) {
        // On enumère les comptes connus de l'index, puis, pour chacun, ses
        // dossiers de corbeille et d'indésirables. La commande n'est envoyée
        // qu'aux comptes qui en ont ; `envoyer` rouvrira au besoin une session
        // tombée dont on connaît les identifiants.
        let comptes: Vec<i64> = self
            .magasin
            .as_ref()
            .and_then(|m| m.comptes().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect();
        for compte in comptes {
            let chemins: Vec<String> = self
                .magasin
                .as_ref()
                .and_then(|m| m.dossiers(compte).ok())
                .map(|dossiers| dossiers_a_vider(&dossiers))
                .unwrap_or_default();
            if !chemins.is_empty() {
                self.as_mut().envoyer(compte, Commande::ViderDossiers { chemins });
            }
        }
    }

    pub fn vider_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString) {
        let chemin = chemin.to_string();
        if chemin.is_empty() {
            return;
        }
        self.as_mut().envoyer(compte as i64, Commande::ViderDossiers { chemins: vec![chemin] });
    }

    pub fn creer_sous_dossier(mut self: Pin<&mut Self>, compte: i32, parent: &QString, nom: &QString) -> QString {
        let compte = compte as i64;
        let parent = parent.to_string();
        let nom = nom.to_string().trim().to_string();
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("index indisponible");
        };
        let dossiers = magasin.dossiers(compte).unwrap_or_default();
        let Some(separateur) = dossiers.iter().find(|d| d.chemin == parent).map(|d| d.separateur.clone()) else {
            return QString::from(&format!("dossier inconnu : {parent}"));
        };
        let chemin = match crate::protocole::chemin_de_sous_dossier(&parent, &separateur, &nom) {
            Ok(c) => c,
            Err(motif) => return QString::from(&motif),
        };
        if dossiers.iter().any(|d| d.chemin == chemin) {
            return QString::from(&format!("« {nom} » existe déjà dans ce dossier"));
        }
        if !self.sessions.contains_key(&compte) && !self.identites.contains_key(&compte) {
            return QString::from("ce compte n'est pas connecté");
        }
        self.as_mut().envoyer(compte, Commande::CreerDossier { parent, chemin, nom });
        QString::from("")
    }

    pub fn actualiser(mut self: Pin<&mut Self>) {
        let comptes: Vec<i64> = self
            .magasin
            .as_ref()
            .and_then(|m| m.comptes().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect();
        let courant = self.courant.clone();
        for compte in comptes {
            let mut commandes = vec![Commande::Arborescence];
            if let Some((c, chemin, _)) = courant.as_ref() {
                if *c == compte {
                    commandes.push(Commande::OuvrirDossier(chemin.clone()));
                }
            }
            if self.sessions.contains_key(&compte) {
                for commande in commandes {
                    self.as_mut().envoyer(compte, commande);
                }
            } else if self.identites.contains_key(&compte) {
                self.as_mut().lancer(compte, commandes);
            }
        }
    }

    pub fn preparer(self: Pin<&mut Self>, uid: i32, mode: &QString, jeton: &QString, adresses: &QString) -> bool {
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        self.preparer_dans(compte as i32, &QString::from(&chemin), uid, mode, jeton, adresses)
    }

    pub fn preparer_dans(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        uid: i32,
        mode: &QString,
        jeton: &QString,
        adresses: &QString,
    ) -> bool {
        let (compte, chemin) = (compte as i64, chemin.to_string());
        let autres: Vec<String> = serde_json::from_str(&adresses.to_string()).unwrap_or_default();
        let commande = Commande::Preparer {
            chemin,
            uid: uid as u32,
            mode: mode.to_string(),
            jeton: jeton.to_string(),
            autres,
        };
        self.as_mut().envoyer(compte, commande)
    }

    /// Lit la rédaction transmise par l'interface ; `None` (et l'erreur posée)
    /// si le JSON est illisible.
    fn lire_redaction(mut self: Pin<&mut Self>, redaction: &QString) -> Option<Redaction> {
        match serde_json::from_str::<Redaction>(&redaction.to_string()) {
            Ok(r) => Some(r),
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("rédaction illisible : {e}")));
                None
            }
        }
    }

    pub fn importer_signature(mut self: Pin<&mut Self>, adresse: &QString, url: &QString) -> QString {
        let Some(fichier) = QUrl::from(url).to_local_file().map(|c| PathBuf::from(c.to_string())) else {
            self.as_mut().set_erreur(QString::from("fichier de signature introuvable"));
            return QString::from("");
        };
        let dossier = dossier_signatures(&self.profil).join(crate::signature::nom_de_dossier(&adresse.to_string()));
        match crate::signature::importer(&fichier, &dossier, &serie_signature()) {
            Ok(html) => QString::from(&html),
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("import de la signature : {e}")));
                QString::from("")
            }
        }
    }

    pub fn image_signature(mut self: Pin<&mut Self>, adresse: &QString, url: &QString) -> QString {
        let Some(source) = QUrl::from(url).to_local_file().map(|c| PathBuf::from(c.to_string())) else {
            self.as_mut().set_erreur(QString::from("image introuvable"));
            return QString::from("");
        };
        let dossier = dossier_signatures(&self.profil).join(crate::signature::nom_de_dossier(&adresse.to_string()));
        match crate::signature::copier_image(&source, &dossier, &serie_signature()) {
            Ok(copie) => QString::from(&crate::rendu::url_fichier(&copie)),
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&e));
                QString::from("")
            }
        }
    }

    pub fn envoyer_message(mut self: Pin<&mut Self>, compte: i32, redaction: &QString) -> bool {
        let compte = compte as i64;
        let Some(redaction) = self.as_mut().lire_redaction(redaction) else {
            return false;
        };
        let Some(identifiants) = self.identites.get(&compte).cloned() else {
            self.as_mut().set_erreur(QString::from("ce compte n'est pas connecté"));
            return false;
        };
        self.as_mut().envoyer(compte, Commande::Envoyer { redaction, identifiants })
    }

    pub fn enregistrer_brouillon(mut self: Pin<&mut Self>, compte: i32, redaction: &QString) -> bool {
        let Some(redaction) = self.as_mut().lire_redaction(redaction) else {
            return false;
        };
        self.as_mut().envoyer(compte as i64, Commande::Brouillon { redaction })
    }

    pub fn garder_redaction(mut self: Pin<&mut Self>, id: &QString, etat: &QString) -> bool {
        if self.profil.is_empty() {
            return false;
        }
        let garde = crate::garde::Garde::du_profil(&self.profil);
        match garde.garder(&id.to_string(), &etat.to_string(), &dossier_pieces(&self.profil)) {
            Ok(()) => true,
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("rédaction gardée sur le poste : {e}")));
                false
            }
        }
    }

    pub fn oublier_redaction_gardee(&self, id: &QString) {
        if !self.profil.is_empty() {
            crate::garde::Garde::du_profil(&self.profil).oublier(&id.to_string());
        }
    }

    pub fn redactions_gardees(&self) -> QString {
        if self.profil.is_empty() {
            return QString::from("[]");
        }
        let liste = crate::garde::Garde::du_profil(&self.profil).lister();
        QString::from(&serde_json::Value::Array(liste).to_string())
    }

    pub fn decrire_fichier(&self, url: &QString) -> QString {
        let texte = url.to_string();
        let chemin = if texte.starts_with("file:") {
            QUrl::from(url).to_local_file().map(|c| c.to_string()).unwrap_or_default()
        } else {
            texte
        };
        let Ok(meta) = std::fs::metadata(&chemin) else {
            return QString::from("");
        };
        if !meta.is_file() {
            return QString::from("");
        }
        let nom = Path::new(&chemin).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // `risquee` : la rédaction ne l'ouvre pas, comme à la lecture.
        QString::from(&format!(
            r#"{{"chemin":{},"nom":{},"taille":{},"risquee":{}}}"#,
            texte_json(&chemin),
            texte_json(&nom),
            meta.len(),
            crate::index::ouverture_risquee(&nom)
        ))
    }

    pub fn non_lus_receptions(&self) -> i32 {
        self.magasin.as_ref().and_then(|m| m.non_lus_receptions().ok()).unwrap_or(0) as i32
    }

    pub fn boites_de_reception(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        QString::from(&json_trouves(&magasin.boites_de_reception(UNIFIEE_MAX).unwrap_or_default()))
    }

    pub fn dossier_images_collees(&self) -> QString {
        let dossier = dossier_images_collees(&self.profil);
        let _ = std::fs::create_dir_all(&dossier);
        QString::from(&dossier.to_string_lossy().into_owned())
    }

    pub fn importer_image(&self, url: &QString) -> QString {
        QString::from(&importer_image(&self.profil, &url.to_string()).unwrap_or_default())
    }

    pub fn adresses_connues(&self, filtre: &QString) -> QString {
        let contacts = self.magasin.as_ref().and_then(|m| m.contacts(&filtre.to_string(), 8).ok()).unwrap_or_default();
        let lignes: Vec<String> = contacts
            .iter()
            .map(|(nom, adresse)| format!(r#"{{"nom":{},"adresse":{}}}"#, texte_json(nom), texte_json(adresse)))
            .collect();
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn role_courant(&self) -> QString {
        let role = match (&self.courant, &self.magasin) {
            (Some((compte, chemin, id)), Some(m)) => m
                .dossier(*id)
                .ok()
                .flatten()
                .map(|d| {
                    if !d.role.is_empty() {
                        return d.role;
                    }
                    if d.profondeur == 0 && d.nom == DOSSIER_DIFFERE {
                        return "Differe".to_string();
                    }
                    // Rôle reconnu au nom du dossier, faute d'attribut
                    // SPECIAL-USE : un brouillon s'y reprend d'un double clic.
                    let dossiers = m.dossiers(*compte).unwrap_or_default();
                    if dossier_indesirables(&dossiers).as_ref() == Some(chemin) {
                        return "Junk".to_string();
                    }
                    ["Drafts", "Sent", "Trash"]
                        .into_iter()
                        .find(|role| dossier_de_role(&dossiers, role).as_ref() == Some(chemin))
                        .unwrap_or_default()
                        .to_string()
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        QString::from(&role)
    }

    // ----------------------------------------------------------------- privé

    fn etat(&self, compte: i64) -> Etat {
        self.etats.get(&compte).copied().unwrap_or(Etat::HorsLigne)
    }

    fn reviser(mut self: Pin<&mut Self>) {
        let suivante = self.revision.wrapping_add(1);
        self.as_mut().set_revision(suivante);
        self.as_mut().verifier_catalogues();
    }

    /// Fait augmenter `revision_categories` si un catalogue rangé a changé —
    /// relu par un fil de compte, ou changé ici.
    fn verifier_catalogues(mut self: Pin<&mut Self>) {
        use std::hash::{Hash, Hasher};
        let Some(valeurs) = self.magasin.as_ref().and_then(|m| m.tous_les_catalogues().ok()) else {
            return;
        };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        valeurs.hash(&mut h);
        let empreinte = h.finish();
        if empreinte != self.empreinte_catalogues {
            self.as_mut().rust_mut().empreinte_catalogues = empreinte;
            let suivante = self.revision_categories.wrapping_add(1);
            self.as_mut().set_revision_categories(suivante);
        }
    }

    fn poser_courant(mut self: Pin<&mut Self>, courant: Option<(i64, String, i64)>) {
        let (compte, chemin) = match &courant {
            Some((c, chemin, _)) => (*c as i32, QString::from(chemin)),
            None => (0, QString::from("")),
        };
        self.as_mut().rust_mut().courant = courant;
        self.as_mut().set_compte_courant(compte);
        self.as_mut().set_dossier_courant(chemin);
    }

    fn demander_piece(self: Pin<&mut Self>, uid: i32, indice: i32, destination: Option<PathBuf>, joindre: bool) -> bool {
        let courant = self.courant.clone();
        self.demander_piece_dans(courant, uid, indice, destination, joindre)
    }

    /// Une pièce jointe d'un message du dossier `designe` (compte, chemin,
    /// identifiant dans l'index).
    fn demander_piece_dans(
        mut self: Pin<&mut Self>,
        designe: Option<(i64, String, i64)>,
        uid: i32,
        indice: i32,
        destination: Option<PathBuf>,
        joindre: bool,
    ) -> bool {
        if uid <= 0 || indice < 0 {
            return false;
        }
        let Some((compte, chemin, dossier)) = designe else {
            return false;
        };
        let (uid, indice) = (uid as u32, indice as usize);
        // Pièce d'un message gardé sur le poste : écrite sans réseau, et hors
        // du fil de l'interface — la lecture du message, son décodage et
        // l'écriture, sur un partage réseau parfois, la figeaient (0.6.6).
        let garde = self.magasin.as_ref().and_then(|m| m.garde_sur_le_poste(dossier, uid).ok().flatten());
        if let Some((id, _)) = garde {
            let ouvrir = destination.is_none() && !joindre;
            let travail = dossier_travail_piece(&self.profil, compte, uid, indice, joindre);
            let profil = self.profil.clone();
            let fil = self.qt_thread();
            let demande = PieceDuPoste { compte, chemin: chemin.clone(), uid, indice, destination: destination.clone(), joindre };
            let lancement = thread::Builder::new().name("mmail-piece".into()).spawn(move || {
                let issue = Cache::du_profil(&profil)
                    .lire(id)
                    .map(|octets| ecrire_piece_de(&octets, indice, demande.destination.clone(), &travail, ouvrir));
                let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| boite.piece_du_poste(demande, issue));
            });
            if lancement.is_ok() {
                return true;
            }
        }
        self.as_mut().envoyer(compte, Commande::Piece { chemin, uid, indice, destination, joindre })
    }

    /// Fin, sur le fil Qt, de l'écriture d'une pièce d'un message gardé sur le
    /// poste. Si son fichier manquait, le fil du compte la relit sur le serveur.
    fn piece_du_poste(mut self: Pin<&mut Self>, d: PieceDuPoste, issue: Option<Result<PathBuf, String>>) {
        let ouvrir = d.destination.is_none() && !d.joindre;
        match issue {
            Some(Ok(fichier)) => self.as_mut().recevoir(d.compte, Issue::Piece { fichier, ouvrir, joindre: d.joindre }),
            Some(Err(m)) => self.as_mut().set_erreur(QString::from(&format!("pièce jointe : {m}"))),
            None => {
                let PieceDuPoste { compte, chemin, uid, indice, destination, joindre } = d;
                self.as_mut().envoyer(compte, Commande::Piece { chemin, uid, indice, destination, joindre });
            }
        }
    }

    fn demander(mut self: Pin<&mut Self>, uid: i32, brut: bool, distantes: bool) -> bool {
        if uid <= 0 {
            return false;
        }
        let Some((compte, chemin, dossier)) = self.courant.clone() else {
            return false;
        };
        let uid = uid as u32;
        // Un autre message : le marquage différé du précédent n'a plus cours.
        // Le même (ses images distantes) le garde.
        if self.lu_differe.as_ref().is_some_and(|d| d.uid != uid || d.dossier != dossier) {
            self.as_mut().rust_mut().lu_differe = None;
        }
        let numero = AFFICHAGE.fetch_add(1, Ordering::SeqCst) + 1;
        // Un message gardé sur le poste s'affiche sans réseau — sauf pour en
        // télécharger les images distantes, que le fil du compte va chercher.
        let en_ligne = self.sessions.contains_key(&compte);
        if !(distantes && en_ligne) {
            let garde = self.magasin.as_ref().and_then(|m| m.garde_sur_le_poste(dossier, uid).ok().flatten());
            if let Some((id, lu)) = garde {
                // Lecture, analyse, assainissement et images hors du fil de
                // l'interface : sur une lettre d'information chargée, ils
                // figeaient la fenêtre à chaque message parcouru.
                let profil = self.profil.clone();
                let fil = self.qt_thread();
                let local = Local { compte, chemin: chemin.clone(), dossier, uid, lu, brut, en_ligne };
                let lancement = thread::Builder::new().name("mmail-affichage".into()).spawn(move || {
                    let preparation = Cache::du_profil(&profil).lire(id).and_then(|octets| {
                        let atelier = dossier_affichage(&profil);
                        rendre_si_actuel(numero, &octets, brut, distantes, compte, &atelier, !lu)
                    });
                    if AFFICHAGE.load(Ordering::SeqCst) == numero {
                        let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| {
                            boite.afficher_du_poste(local, preparation, numero, distantes);
                        });
                    }
                });
                if lancement.is_ok() {
                    return true;
                }
            }
        }
        self.as_mut().envoyer(compte, Commande::Corps { chemin, uid, brut, distantes, numero })
    }

    /// Fin, sur le fil Qt, de l'affichage d'un message gardé sur le poste. Si
    /// son fichier manquait, le fil du compte le relit sur le serveur.
    fn afficher_du_poste(mut self: Pin<&mut Self>, l: Local, preparation: Option<Lu>, numero: u64, distantes: bool) {
        let Some(preparation) = preparation else {
            let Local { compte, chemin, uid, brut, .. } = l;
            self.as_mut().envoyer(compte, Commande::Corps { chemin, uid, brut, distantes, numero });
            return;
        };
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.poser_pieces(l.dossier, l.uid, preparation.pieces != "[]");
        }
        // Le marquage « lu » part au serveur s'il est joignable ; hors
        // connexion, le message reste non lu.
        let non_lu = !l.brut && !l.lu && l.en_ligne;
        self.as_mut().recevoir(l.compte, Issue::Corps { chemin: l.chemin, uid: l.uid, lu: preparation, non_lu, numero });
    }

    /// Lance le fil de travail d'un compte, avec des commandes à traiter dès la
    /// connexion ouverte.
    fn lancer(mut self: Pin<&mut Self>, compte: i64, commandes: Vec<Commande>) -> bool {
        let Some(identifiants) = self.identites.get(&compte).cloned() else {
            return false;
        };
        self.as_mut().fermer_session(compte);

        let generation = self.generation + 1;
        // La connexion, et les seules commandes qui comptent : les autres ne
        // le décomptent jamais, et l'indicateur d'activité tournait sans fin
        // (revue du 19/10, n° 13).
        let attente = Arc::new(AtomicUsize::new(1 + commandes.iter().filter(|c| c.comptee()).count()));
        let (envoi, reception) = mpsc::channel();
        for commande in commandes {
            let _ = envoi.send(commande);
        }
        let travail = Travail {
            fil: Destinataire::Qt(self.qt_thread()),
            compte,
            generation,
            attente: attente.clone(),
            atelier: atelier(&self.profil),
            profil: self.profil.clone(),
        };
        let lancement = thread::Builder::new()
            .name(format!("mmail-imap-{compte}"))
            .spawn(move || travail.executer(identifiants, reception));
        if let Err(e) = lancement {
            self.as_mut().set_erreur(QString::from(&format!("fil de travail : {e}")));
            return false;
        }
        {
            let mut noyau = self.as_mut().rust_mut();
            noyau.generation = generation;
            noyau.sessions.insert(compte, Session { envoi, attente, generation });
            noyau.etats.insert(compte, Etat::Connexion);
        }
        self.as_mut().set_erreur(QString::from(""));
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
        true
    }

    fn envoyer(mut self: Pin<&mut Self>, compte: i64, commande: Commande) -> bool {
        if !self.sessions.contains_key(&compte) {
            // Session tombée, identifiants connus : on la rouvre, la commande
            // attendra la connexion.
            if self.identites.contains_key(&compte) {
                return self.as_mut().lancer(compte, vec![commande]);
            }
            self.as_mut().set_erreur(QString::from("ce compte n'est pas connecté"));
            return false;
        }
        let refusee = {
            let session = &self.sessions[&compte];
            let comptee = commande.comptee();
            if comptee {
                session.attente.fetch_add(1, Ordering::SeqCst);
            }
            match session.envoi.send(commande) {
                Ok(()) => None,
                Err(mpsc::SendError(commande)) => {
                    if comptee {
                        session.attente.fetch_sub(1, Ordering::SeqCst);
                    }
                    Some(commande)
                }
            }
        };
        if let Some(commande) = refusee {
            // Le fil du compte s'est arrêté sans rendre son issue : on le
            // relance, la commande attendra la connexion au lieu d'être perdue.
            self.as_mut().rust_mut().sessions.remove(&compte);
            if self.identites.contains_key(&compte) {
                return self.as_mut().lancer(compte, vec![commande]);
            }
            self.as_mut().set_erreur(QString::from("la session du compte s'est fermée"));
            self.as_mut().rafraichir_occupe();
            return false;
        }
        self.as_mut().rafraichir_occupe();
        true
    }

    /// Lâche la session d'un compte sans l'attendre. Son fil s'arrête de
    /// lui-même dès qu'il constate la fermeture du canal ; ce qu'il rendrait
    /// d'ici là porte une génération périmée et sera ignoré.
    fn fermer_session(mut self: Pin<&mut Self>, compte: i64) {
        self.as_mut().rust_mut().sessions.remove(&compte);
    }

    /// `rechercheInachevee`, avec le nom lisible du dossier.
    fn signaler_inachevee(self: Pin<&mut Self>, compte: i64, chemin: &str, texte: &str, motif: &str) {
        let separateur = self
            .magasin
            .as_ref()
            .and_then(|m| m.dossiers(compte).ok())
            .and_then(|d| d.into_iter().find(|d| d.chemin == chemin).map(|d| d.separateur))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/".into());
        let mut noms = segments_du_chemin(chemin, &separateur);
        if noms.first().is_some_and(|n| n.eq_ignore_ascii_case("INBOX")) {
            noms[0] = "Boîte de réception".into();
        }
        self.recherche_inachevee(
            compte as i32,
            &QString::from(chemin),
            &QString::from(&noms.join(" › ")),
            &QString::from(texte),
            &QString::from(motif),
        );
    }

    fn rafraichir_occupe(mut self: Pin<&mut Self>) {
        let occupe = self.sessions.values().any(|s| s.attente.load(Ordering::SeqCst) > 0);
        self.as_mut().set_occupe(occupe);
    }

    fn rafraichir_attente(mut self: Pin<&mut Self>) {
        let attente = self
            .magasin
            .as_ref()
            .and_then(|m| m.nombre_operations().ok())
            .unwrap_or(0) as i32;
        self.as_mut().set_en_attente(attente);
    }

    /// Applique sur le fil Qt ce que le fil de travail d'un compte a rendu.
    fn recevoir(mut self: Pin<&mut Self>, compte: i64, issue: Issue) {
        self.as_mut().conserver_jetons();
        let compte_qt = compte as i32;
        match issue {
            Issue::Precharge { .. } => {}
            Issue::Connecte => {
                let adresse = self.identites.get(&compte).map(|i| i.utilisateur.clone()).unwrap_or_default();
                {
                    let mut noyau = self.as_mut().rust_mut();
                    noyau.etats.insert(compte, Etat::Pret);
                    noyau.nouveaux.remove(&compte);
                    noyau.authentifies.insert(compte);
                }
                // Ce compte peut être la source ou la cible de déplacements
                // interrompus : chaque session connectée reprend les siens.
                let identites = self.identites.clone();
                let connectes: Vec<i64> = self
                    .sessions
                    .keys()
                    .copied()
                    .filter(|c| *c == compte || self.etat(*c) == Etat::Pret)
                    .collect();
                for c in connectes {
                    self.as_mut().envoyer(c, Commande::Reprendre(identites.clone()));
                }
                self.as_mut().set_erreur(QString::from(""));
                self.as_mut().reviser();
                self.as_mut().connecte(compte_qt, &QString::from(&adresse));
            }
            Issue::Arborescence => {
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                // La boîte de réception, relue en fond : les filtres voient
                // les arrivées.
                self.as_mut().filtrer(compte, "INBOX", false);
            }
            Issue::Dossier { chemin, veille } => {
                self.as_mut().reviser();
                self.as_mut().dossier_ouvert(compte_qt, &QString::from(&chemin), veille);
                if chemin == "INBOX" {
                    self.as_mut().filtrer(compte, "INBOX", false);
                }
            }
            Issue::Corps { chemin, uid, lu, non_lu, numero } => {
                let chemin_qt = QString::from(&chemin);
                // Numéro 0 : la fenêtre d'un message, qui le marque elle-même.
                if non_lu && numero != 0 {
                    // Les compteurs de l'arborescence, et la seule ligne du
                    // message : relire la liste entière ralentissait le passage
                    // d'un message à l'autre.
                    let dossier = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok());
                    match (dossier, self.delai_lu) {
                        (Some(dossier), 0) => self.as_mut().marquer_lu_a_l_affichage(compte, chemin, dossier, uid, numero),
                        // Après un délai : l'interface en rappelle la fin.
                        (Some(dossier), secondes) if secondes > 0 => {
                            self.as_mut().rust_mut().lu_differe = Some(LuDiffere { compte, chemin, dossier, uid, numero });
                            self.as_mut().marquage_lu_differe(uid as i32, secondes);
                        }
                        // Jamais : à la main seulement.
                        _ => {}
                    }
                }
                self.as_mut().corps_recu(
                    uid as i32,
                    &QString::from(&lu.texte),
                    lu.brut,
                    lu.html,
                    lu.bloquees as i32,
                    &QString::from(&lu.pieces),
                    &QString::from(&lu.confirmation),
                    &QString::from(&lu.entete),
                    compte_qt,
                    &chemin_qt,
                );
                let invitation = lu.invitation.as_ref().map(|e| self.as_mut().decrire_invitation(compte, e)).unwrap_or_default();
                self.as_mut().invitation_recue(uid as i32, &QString::from(&invitation));
            }
            Issue::Programme { jeton, echeance } => {
                self.as_mut().reviser();
                self.as_mut().programme(&QString::from(&jeton), &QString::from(&echeance.to_string()));
            }
            Issue::DifferesEnvoyes { nombre, erreurs } => {
                self.as_mut().reviser();
                self.as_mut().differes_envoyes(nombre as i32, &QString::from(&erreurs));
            }
            Issue::Piece { fichier, ouvrir, joindre } => {
                let chemin = QString::from(fichier.to_string_lossy().as_ref());
                let url = QUrl::from_local_file(&chemin).to_qstring();
                if joindre {
                    self.as_mut().piece_a_joindre(&url);
                } else {
                    self.as_mut().piece_ecrite(&url, &chemin, ouvrir);
                }
            }
            Issue::Marque { discret } => {
                self.as_mut().reviser();
                if !discret {
                    self.as_mut().drapeaux_modifies();
                }
            }
            Issue::Deplace { cible, chemin_cible, rapport } => {
                // La cible a changé aussi : ses compteurs, et sa liste si c'est
                // le dossier ouvert.
                if cible != compte && self.sessions.contains_key(&cible) {
                    self.as_mut().envoyer(cible, Commande::Arborescence);
                }
                let courant = self.courant.clone();
                if let Some((c, chemin, _)) = courant {
                    if c == cible && chemin == chemin_cible {
                        self.as_mut().envoyer(cible, Commande::OuvrirDossier(chemin));
                    }
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                let erreurs = rapport.erreurs.join("\n");
                if !erreurs.is_empty() {
                    self.as_mut().set_erreur(QString::from(&erreurs));
                }
                self.as_mut()
                    .deplacement_termine(rapport.deplaces as i32, &QString::from(&erreurs));
            }
            Issue::DeplacementEchoue { message } => {
                // Le message d'erreur arrive par l'échec qui suit, ou par celui
                // de la connexion : ici, seulement de quoi rendre les lignes.
                self.as_mut().deplacement_termine(0, &QString::from(&message));
            }
            Issue::RecherchePartout { chemin, texte, uids, restants, inacheve } => {
                if let Some(motif) = inacheve {
                    self.as_mut().signaler_inachevee(compte, &chemin, &texte, &motif);
                }
                let uids: Vec<String> = uids.iter().map(u32::to_string).collect();
                self.as_mut().resultats_partout(
                    compte_qt,
                    &QString::from(&chemin),
                    &QString::from(&texte),
                    &QString::from(&uids.join(",")),
                    restants as i32,
                );
            }
            Issue::Recherche { chemin, texte, uids, inacheve } => {
                let uids: Vec<String> = uids.iter().map(u32::to_string).collect();
                self.as_mut().resultats_serveur(
                    compte_qt,
                    &QString::from(&chemin),
                    &QString::from(&texte),
                    &QString::from(&uids.join(",")),
                );
                if let Some(motif) = inacheve {
                    self.as_mut().signaler_inachevee(compte, &chemin, &texte, &motif);
                }
            }
            Issue::MarquageAbandonne { chemin } => {
                if let Some(magasin) = self.magasin.as_ref() {
                    if let Ok(id) = magasin.dossier_id(compte, &chemin) {
                        let _ = magasin.oublier_modseq(id);
                    }
                }
            }
            Issue::Prepare { jeton, contenu } => {
                self.as_mut().preparation(&QString::from(&jeton), &QString::from(&contenu));
            }
            Issue::Envoye { jeton, avertissement } => {
                // « Éléments envoyés » et l'original (répondu) ont changé.
                self.as_mut().reviser();
                self.as_mut().envoye(&QString::from(&jeton), &QString::from(&avertissement));
            }
            Issue::Parti { redaction, erreur: Some(message), .. } => {
                self.as_mut().echec_redaction(&QString::from(&redaction.jeton), &QString::from(&message));
            }
            Issue::Parti { redaction, copie, erreur: None } => {
                // La suite passe par le fil du compte, rouvert au besoin.
                let jeton = redaction.jeton.clone();
                if !self.as_mut().envoyer(compte, Commande::ApresEnvoi { redaction, copie }) {
                    let avertissement = "compte hors ligne : pas de copie dans les éléments envoyés";
                    self.as_mut().envoye(&QString::from(&jeton), &QString::from(avertissement));
                }
            }
            Issue::BrouillonEnregistre { jeton, uid } => {
                self.as_mut().reviser();
                self.as_mut().brouillon_enregistre(&QString::from(&jeton), uid as i32);
            }
            Issue::EchecRedaction { jeton, message } => {
                self.as_mut().echec_redaction(&QString::from(&jeton), &QString::from(&message));
            }
            Issue::Vide { nombre } => {
                // Le fil a déjà relu l'arborescence : les compteurs des dossiers
                // vidés sont à jour dans l'index, il ne reste qu'à rafraîchir.
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                self.as_mut().corbeilles_videes(nombre as i32);
            }
            Issue::DossierCree { parent, chemin, nom } => {
                // Déplié, le parent montre le dossier qu'on vient de créer.
                if let Some(magasin) = self.magasin.as_ref() {
                    if let Ok(id) = magasin.dossier_id(compte, &parent) {
                        let _ = magasin.replier_dossier(id, false);
                    }
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                self.as_mut().dossier_cree(compte_qt, &QString::from(&chemin), &QString::from(&nom));
            }
            Issue::Repris { rapport } => {
                // Les cibles ont reçu des messages : leurs compteurs aussi.
                let connectes: Vec<i64> =
                    self.sessions.keys().copied().filter(|c| *c != compte).collect();
                for c in connectes {
                    self.as_mut().envoyer(c, Commande::Arborescence);
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                if !rapport.erreurs.is_empty() {
                    let message = format!("reprise des déplacements : {}", rapport.erreurs.join("\n"));
                    self.as_mut().set_erreur(QString::from(&message));
                }
            }
            Issue::Echec { etape, message, session_perdue } => {
                if session_perdue {
                    let nouveau = self.nouveaux.contains(&compte);
                    {
                        let mut noyau = self.as_mut().rust_mut();
                        noyau.sessions.remove(&compte);
                        noyau.etats.insert(compte, Etat::Erreur);
                        noyau.motifs.insert(compte, format!("{etape} — {message}"));
                    }
                    if nouveau && (etape == "connexion" || etape == "identifiants") {
                        // Un compte dont la toute première connexion échoue
                        // n'a rien à faire dans le profil.
                        {
                            let mut noyau = self.as_mut().rust_mut();
                            noyau.nouveaux.remove(&compte);
                            noyau.identites.remove(&compte);
                            noyau.etats.remove(&compte);
                        }
                        if let Some(magasin) = self.magasin.as_ref() {
                            let _ = magasin.retirer_compte(compte);
                        }
                    }
                    if etape == "identifiants" {
                        self.as_mut().rust_mut().identites.remove(&compte);
                    }
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                let message = QString::from(&message);
                self.as_mut().set_erreur(message.clone());
                self.as_mut().echec(compte_qt, &QString::from(etape), &message);
            }
        }
        self.as_mut().rafraichir_occupe();
    }
}

/// Dossier des copies de travail des déplacements, à côté de l'index.
fn atelier(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("operations"))
        .unwrap_or_else(|| PathBuf::from("operations"))
}

/// Dossier de travail des pièces jointes ouvertes, à côté de l'index. Vidé à
/// chaque ouverture du profil.
fn dossier_pieces(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("pieces-jointes"))
        .unwrap_or_else(|| PathBuf::from("pieces-jointes"))
}

/// Dossier de travail des images des messages affichés, à côté de l'index.
/// Vidé à chaque ouverture du profil, et au fil des affichages.
fn dossier_affichage(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("affichage"))
        .unwrap_or_else(|| PathBuf::from("affichage"))
}

/// Dossier de travail d'une pièce jointe écrite pour être ouverte, ou jointe
/// à une rédaction : un par pièce, le nom d'origine pouvant se répéter.
fn dossier_travail_piece(profil: &str, compte: i64, uid: u32, indice: usize, joindre: bool) -> PathBuf {
    let prefixe = if joindre { "joindre-" } else { "" };
    dossier_pieces(profil).join(format!("{prefixe}{compte}-{uid}-{indice}"))
}

/// Images des signatures, à côté de l'index : l'un des deux seuls dossiers
/// d'où une image part intégrée à un message.
fn dossier_signatures(profil: &str) -> PathBuf {
    Path::new(profil).parent().map(|p| p.join("signatures")).unwrap_or_else(|| PathBuf::from("signatures"))
}

/// Images des brouillons repris, l'autre dossier autorisé.
fn dossier_images_redaction(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("images-redaction"))
        .unwrap_or_else(|| PathBuf::from("images-redaction"))
}

/// Préfixe unique des fichiers d'une signature : une image remplacée ne
/// réutilise jamais le nom d'une autre, que le moteur de Qt garde en cache.
/// Images collées ou glissées dans le corps d'une rédaction : sous le dossier
/// des images de rédaction, l'un des deux dont l'envoi intègre les images.
fn dossier_images_collees(profil: &str) -> PathBuf {
    dossier_images_redaction(profil).join("collees")
}

/// Copie une image locale parmi les images collées et rend son adresse
/// `file:`. Ce qui n'est pas une image, ou dépasse 10 Mo — la limite de
/// l'envoi —, n'est pas copié.
fn importer_image(profil: &str, adresse: &str) -> Option<String> {
    let source = crate::rendu::chemin_de_url(adresse)?;
    let type_mime = crate::rendu::type_image(&source.to_string_lossy())?;
    let meta = std::fs::metadata(&source).ok()?;
    if !meta.is_file() || meta.len() > 10 * 1024 * 1024 {
        return None;
    }
    let dossier = dossier_images_collees(profil);
    std::fs::create_dir_all(&dossier).ok()?;
    let cible = dossier.join(format!("glissee-{}.{}", serie_signature(), crate::rendu::extension_image(type_mime)));
    std::fs::copy(&source, &cible).ok()?;
    Some(crate::rendu::url_fichier(&cible))
}

fn serie_signature() -> String {
    format!("{}-{}", maintenant(), SERIE_AFFICHAGE.fetch_add(1, Ordering::Relaxed))
}

/// Image désignée par une adresse `file:` du HTML à envoyer, si elle est dans
/// l'un des dossiers autorisés du profil. Le HTML vient de la fenêtre de
/// rédaction, mais un brouillon repris ou un texte collé peut désigner
/// n'importe quel fichier : il ne part jamais.
fn image_autorisee(profil: &str, adresse: &str) -> Option<(String, Vec<u8>)> {
    let chemin = crate::rendu::chemin_de_url(adresse)?.canonicalize().ok()?;
    let autorise = [dossier_signatures(profil), dossier_images_redaction(profil)]
        .iter()
        .filter_map(|d| d.canonicalize().ok())
        .any(|d| chemin.starts_with(d));
    if !autorise {
        return None;
    }
    let type_mime = crate::rendu::type_image(&chemin.to_string_lossy())?;
    (std::fs::metadata(&chemin).ok()?.len() <= 10 * 1024 * 1024)
        .then(|| std::fs::read(&chemin).ok())
        .flatten()
        .map(|octets| (type_mime.to_string(), octets))
}

/// « 3,5,7 » → [3, 5, 7] ; ce qui n'est pas un nombre est ignoré.
/// Mots et critères de catégorie d'un texte cherché (cf.
/// `categories::separer_criteres`) : chaque nom devient les mots-clés qu'il
/// désigne dans les catalogues rangés.
fn criteres_recherche(magasin: &Magasin, texte: &str) -> (String, Vec<Vec<String>>) {
    let (mots, noms) = categories::separer_criteres(texte);
    if noms.is_empty() {
        return (mots, Vec::new());
    }
    let catalogues = magasin.tous_les_catalogues().unwrap_or_default();
    (mots, noms.iter().map(|n| categories::mots_cles_nommes(n, &catalogues)).collect())
}

/// Boîte dont le catalogue de catégories vaut pour un dossier (0.6.15) : la
/// boîte partagée qui le contient — chemin de sa boîte de réception, comme
/// « Shared/contact@exemple.fr/INBOX » —, à défaut la boîte de réception du
/// compte.
fn boite_categories(magasin: &Magasin, compte: i64, chemin: &str) -> String {
    let partagees = magasin.compte_par_id(compte).ok().flatten().map(|c| c.partagees).unwrap_or_default();
    if partagees.is_empty() {
        return "INBOX".into();
    }
    let dossiers = magasin.dossiers(compte).unwrap_or_default();
    let separateur = dossiers
        .iter()
        .find(|d| d.chemin == chemin)
        .map(|d| d.separateur.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/".into());
    let segments: Vec<&str> = chemin.split(separateur.as_str()).collect();
    for (i, segment) in segments.iter().enumerate() {
        let adresse = crate::protocole::decoder_utf7(segment);
        if partagees.iter().any(|a| a.eq_ignore_ascii_case(&adresse)) {
            let racine = segments[..=i].join(&separateur);
            let reception = format!("{racine}{separateur}INBOX");
            if dossiers.iter().any(|d| d.chemin == reception && d.selectionnable) {
                return reception;
            }
            return racine;
        }
    }
    "INBOX".into()
}

fn lire_uids(texte: &QString) -> Vec<u32> {
    texte
        .to_string()
        .split(',')
        .filter_map(|u| u.trim().parse().ok())
        .filter(|&u: &u32| u > 0)
        .collect()
}

/// Dossier d'indésirables d'un compte : celui qui porte l'attribut `\Junk`,
/// à défaut celui qui s'appelle « Junk » — c'est sur ce nom que Mailcow fait
/// apprendre son filtre —, à défaut un nom usuel.
fn dossier_indesirables(dossiers: &[DossierLocal]) -> Option<String> {
    let selectionnables = || dossiers.iter().filter(|d| d.selectionnable);
    selectionnables()
        .find(|d| d.role == "Junk")
        .or_else(|| selectionnables().find(|d| d.chemin.eq_ignore_ascii_case("Junk")))
        .or_else(|| {
            selectionnables().find(|d| {
                let nom = d.nom.to_lowercase();
                d.role.is_empty()
                    && ["indésirable", "indesirable", "pourriel", "junk", "spam"].iter().any(|m| nom.contains(m))
            })
        })
        .map(|d| d.chemin.clone())
}

/// Noms usuels des dossiers de rôle, pour un serveur qui ne les désigne pas
/// (OVH n'annonce pas SPECIAL-USE). Les noms standard d'abord : sur la boîte
/// d'essai OVH, « Sent » et « Éléments envoyés » coexistent, et « Sent » est
/// celui qui a reçu le dernier envoi (relevé du 08/10/2026).
fn noms_de_role(role: &str) -> &'static [&'static str] {
    match role {
        "Drafts" => &["drafts", "brouillons"],
        "Sent" => &["sent", "sent items", "sent messages", "envoyés", "éléments envoyés", "messages envoyés"],
        "Trash" => &["trash", "deleted items", "deleted messages", "corbeille", "éléments supprimés"],
        _ => &[],
    }
}

/// Dossier qui tient un rôle : celui qui porte l'attribut SPECIAL-USE ; à
/// défaut, un dossier sans rôle qui en porte un nom usuel — le moins profond
/// d'abord, puis dans l'ordre de `noms_de_role`. Sert à ranger (brouillons,
/// copie des envois, corbeille où déplacer), jamais à purger : cf.
/// `dossiers_a_vider`.
fn dossier_de_role(dossiers: &[DossierLocal], role: &str) -> Option<String> {
    if let Some(d) = dossiers.iter().find(|d| d.role == role) {
        return Some(d.chemin.clone());
    }
    let noms = noms_de_role(role);
    dossiers
        .iter()
        .filter(|d| d.selectionnable && d.role.is_empty())
        .filter_map(|d| {
            let nom = d.nom.to_lowercase();
            noms.iter().position(|n| *n == nom).map(|rang| ((d.profondeur, rang), d))
        })
        .min_by_key(|(cle, _)| *cle)
        .map(|(_, d)| d.chemin.clone())
}

/// Noms usuels d'une corbeille ou d'un dossier d'indésirables, pour un
/// serveur qui n'annonce aucun rôle. Comparés au nom entier : « Spambox » ou
/// « Signalements spam » sont des dossiers de l'utilisateur.
const NOMS_A_VIDER: &[&str] = &[
    "trash",
    "corbeille",
    "deleted items",
    "deleted messages",
    "éléments supprimés",
    "junk",
    "junk e-mail",
    "spam",
    "indésirables",
    "courrier indésirable",
    "pourriel",
];

/// Dossiers que « Vider les corbeilles » purge définitivement. Dès que le
/// serveur annonce des rôles (SPECIAL-USE), seuls ceux qui portent `\Trash`
/// ou `\Junk` : un nom ne suffit jamais à justifier une purge. Sans aucun rôle
/// annoncé, les noms usuels, à l'identique.
fn dossiers_a_vider(dossiers: &[DossierLocal]) -> Vec<String> {
    let par_role = |d: &DossierLocal| d.role == "Trash" || d.role == "Junk";
    let roles_annonces = dossiers.iter().any(|d| !d.role.is_empty());
    dossiers
        .iter()
        .filter(|d| d.selectionnable)
        .filter(|d| if roles_annonces { par_role(d) } else { NOMS_A_VIDER.contains(&d.nom.to_lowercase().as_str()) })
        .map(|d| d.chemin.clone())
        .collect()
}

// -------------------------------------------------------------- fil de travail

struct Travail {
    fil: Destinataire,
    compte: i64,
    generation: u64,
    attente: Arc<AtomicUsize>,
    atelier: PathBuf,
    profil: String,
}

/// Ce que possède le fil de travail d'un compte.
struct Etabli {
    client: Client,
    magasin: Magasin,
    /// Sessions ouvertes sur d'autres boîtes, pour y déposer des messages.
    annexes: HashMap<i64, Client>,
    /// Identifiants des autres boîtes, reçus avec les commandes.
    identites: HashMap<i64, Identifiants>,
    /// Dossier que l'utilisateur regarde, celui que la veille resynchronise.
    /// Pas forcément le dossier sélectionné : un vidage, une reprise ou un
    /// envoi différé en sélectionnent d'autres.
    ouvert: Option<String>,
    /// Dernier passage du ménage des messages sortis de la fenêtre gardée.
    menage: Option<Instant>,
    /// Dernier message entier lu sur le serveur (dossier, UIDVALIDITY, UID) :
    /// ouvrir ses pièces jointes une à une, y répondre ou le transférer ne le
    /// retélécharge pas, même hors de la fenêtre gardée sur le poste.
    dernier: Option<(String, u32, u32, Vec<u8>)>,
    /// Envois différés vus réservés, et depuis quand (cf. `differes_interrompus`).
    differes_reserves: HashMap<u32, i64>,
}

/// À qui le fil d'un compte remet ses issues : l'objet Qt — ou, dans les
/// tests du fil, un canal (0.6.6).
#[derive(Clone)]
enum Destinataire {
    Qt(CxxQtThread<qobject::Boite>),
    #[cfg(test)]
    Essai(mpsc::Sender<Issue>),
}

impl Destinataire {
    /// Remet l'issue d'une recherche sur le serveur, que l'interface ignore
    /// si une autre recherche l'a remplacée entre-temps (0.6.16).
    fn remettre_recherche(&self, compte: i64, numero: u64, issue: Issue) {
        match self {
            // Échoue seulement si l'objet Qt n'existe plus : rien à faire alors.
            Destinataire::Qt(fil) => {
                let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| {
                    if boite.recherche.load(Ordering::SeqCst) == numero {
                        boite.recevoir(compte, issue);
                    }
                });
            }
            #[cfg(test)]
            Destinataire::Essai(canal) => {
                let _ = canal.send(issue);
            }
        }
    }
}

/// De quoi remettre une issue à l'interface depuis un autre fil que celui du
/// compte (l'envoi SMTP).
struct Retour {
    fil: Destinataire,
    compte: i64,
    generation: u64,
}

impl Retour {
    /// Remet une issue à l'interface, qui l'ignorera si la session a changé
    /// — sauf `toujours`.
    fn remettre(&self, issue: Issue, toujours: bool) {
        let generation = self.generation;
        let compte = self.compte;
        match &self.fil {
            // Échoue seulement si l'objet Qt n'existe plus : rien à faire alors.
            Destinataire::Qt(fil) => {
                let _ = fil.queue(move |boite: Pin<&mut qobject::Boite>| {
                    let a_jour = boite.sessions.get(&compte).map(|s| s.generation) == Some(generation);
                    if a_jour || toujours {
                        boite.recevoir(compte, issue);
                    }
                });
            }
            #[cfg(test)]
            Destinataire::Essai(canal) => {
                let _ = canal.send(issue);
            }
        }
    }
}

impl Travail {
    fn retour(&self) -> Retour {
        Retour { fil: self.fil.clone(), compte: self.compte, generation: self.generation }
    }

    /// Remet une issue à l'interface, qui l'ignorera si la session a changé.
    fn remettre(&self, issue: Issue) {
        self.retour().remettre(issue, false);
    }

    fn terminer(&self, nombre: usize) {
        if nombre > 0 {
            self.attente.fetch_sub(nombre, Ordering::SeqCst);
        }
    }

    /// Rend l'issue d'une commande qui ne sera jamais traitée (connexion
    /// impossible, session perdue) : une rédaction ne doit pas rester « en
    /// cours d'envoi », ni une ligne masquée par un déplacement qui n'aura pas
    /// lieu, ni un marquage local que le serveur n'a pas reçu.
    fn abandonner(&self, commande: Commande, motif: &str) {
        let issue = match commande {
            Commande::Envoyer { redaction, .. } => Issue::EchecRedaction {
                jeton: redaction.jeton,
                message: format!("{motif} — le message n'est pas parti"),
            },
            Commande::Brouillon { redaction } => Issue::EchecRedaction {
                jeton: redaction.jeton,
                message: format!("{motif} — le brouillon n'est pas enregistré"),
            },
            // Le message est parti : seule sa suite manque.
            Commande::ApresEnvoi { redaction, .. } => Issue::Envoye {
                jeton: redaction.jeton,
                avertissement: format!("{motif} — pas de copie dans les éléments envoyés"),
            },
            Commande::Preparer { jeton, .. } => Issue::EchecRedaction { jeton, message: motif.to_string() },
            Commande::Deplacer { .. } => {
                Issue::DeplacementEchoue { message: format!("{motif} — messages non déplacés") }
            }
            Commande::MarquerLu { chemin, .. }
            | Commande::MarquerSuivi { chemin, .. }
            | Commande::MarquerCategorie { chemin, .. } => Issue::MarquageAbandonne { chemin },
            _ => return,
        };
        self.remettre(issue);
    }

    /// Attend la prochaine chose à faire : une commande de l'interface,
    /// l'échéance de la veille, ou — en `IDLE` sur le dossier ouvert, quand le
    /// serveur le sait — un changement qu'il signale. Un nouveau message
    /// apparaît ainsi en quelques secondes, au lieu d'attendre la veille.
    fn attendre(&self, e: &mut Etabli, reception: &Receiver<Commande>, veille: Instant) -> Reveil {
        let perdue = |err: Echec| Reveil::Perdue(echec("reseau", format!("attente des nouveaux messages : {err}"), &err));
        // Le dossier ouvert ; à défaut, la boîte de réception : un compte que
        // l'on ne regarde pas voit arriver son courrier sans attendre la veille
        // (compteurs, pastille, boîte de réception unifiée).
        let ouvert = Some(dossier_surveille(e)).filter(|_| e.client.sait("IDLE"));
        if let Some(chemin) = ouvert {
            let debut = assurer_selection(&mut e.client, &chemin).and_then(|()| Ok(e.client.idle_commencer()?));
            match debut {
                Ok(()) => {
                    let reveil = loop {
                        match reception.try_recv() {
                            Ok(commande) => break Reveil::Commande(commande),
                            Err(TryRecvError::Disconnected) => break Reveil::Fermee,
                            Err(TryRecvError::Empty) => {}
                        }
                        if Instant::now() >= veille {
                            break Reveil::Veille;
                        }
                        // Une commande attend au plus ce délai : le canal de
                        // l'interface et la socket ne s'attendent pas ensemble.
                        match e.client.idle_attendre(Duration::from_millis(100)) {
                            Ok(Some(ligne)) if signale_un_changement(&ligne) => break Reveil::Signal,
                            Ok(_) => {}
                            Err(err) => return perdue(Echec::Imap(err)),
                        }
                    };
                    return match e.client.idle_terminer() {
                        Err(err) if matches!(err, Erreur::Reseau(_)) => perdue(Echec::Imap(err)),
                        _ => reveil,
                    };
                }
                Err(err) if err.reseau() => return perdue(err),
                // Dossier disparu, IDLE refusé : attente ordinaire, jusqu'à
                // la prochaine ouverture d'un dossier.
                Err(_) => e.ouvert = None,
            }
        }
        match reception.recv_timeout(veille.saturating_duration_since(Instant::now())) {
            Ok(commande) => Reveil::Commande(commande),
            Err(RecvTimeoutError::Timeout) => Reveil::Veille,
            Err(RecvTimeoutError::Disconnected) => Reveil::Fermee,
        }
    }

    fn abandonner_toutes(&self, commandes: Vec<Commande>, cause: &Issue) {
        let motif = match cause {
            Issue::Echec { message, .. } => message.as_str(),
            _ => "session perdue",
        };
        for commande in commandes {
            self.abandonner(commande, motif);
        }
    }

    fn executer(self, identifiants: Identifiants, reception: Receiver<Commande>) {
        let etabli = match self.etablir(&identifiants) {
            Ok(etabli) => etabli,
            Err(issue) => {
                // La connexion et les commandes qui l'attendaient sont perdues ;
                // chacune rend son issue avant l'échec, qui ferme la session.
                let restantes: Vec<Commande> = reception.try_iter().collect();
                self.terminer(1 + restantes.iter().filter(|c| c.comptee()).count());
                self.abandonner_toutes(restantes, &issue);
                self.remettre(issue);
                return;
            }
        };
        drop(identifiants);
        self.travailler(etabli, reception);
    }

    /// Traite les commandes d'une session établie, jusqu'à la fermeture du
    /// canal par l'interface ou la perte de la session.
    fn travailler(self, mut etabli: Etabli, reception: Receiver<Commande>) {
        self.terminer(1);
        self.remettre(Issue::Connecte);

        // La boîte de réception s'indexe dès la connexion ouverte — même celle
        // d'un compte que l'on ne regarde pas : la boîte de réception unifiée
        // et la pastille de la barre en ont besoin — puis se précharge, pour se
        // lire hors connexion.
        let mut file: Vec<Commande> = vec![Commande::Signale, Commande::Precharger { chemin: "INBOX".into() }];
        // Échéance fixe : les commandes reçues entre-temps ne la repoussent pas.
        // Mesurée en temps d'inactivité, la veille ne venait jamais — la
        // minuterie des envois différés écrit à chaque compte toutes les minutes.
        let mut veille = Instant::now() + VEILLE;
        loop {
            if file.is_empty() {
                match self.attendre(&mut etabli, &reception, veille) {
                    Reveil::Commande(commande) => file.push(commande),
                    Reveil::Veille => file.push(Commande::Veille),
                    Reveil::Signal => file.push(Commande::Signale),
                    Reveil::Fermee => break,
                    Reveil::Perdue(issue) => {
                        let restantes: Vec<Commande> = reception.try_iter().collect();
                        self.terminer(restantes.iter().filter(|c| c.comptee()).count());
                        self.abandonner_toutes(restantes, &issue);
                        self.remettre(issue);
                        return;
                    }
                }
            }
            loop {
                match reception.try_recv() {
                    Ok(commande) => file.push(commande),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        etabli.client.fermer();
                        return;
                    }
                }
            }
            // Échéance passée pendant un long préchargement : la veille passe
            // devant lui.
            if Instant::now() >= veille && !file.iter().any(|c| matches!(c, Commande::Veille)) {
                file.push(Commande::Veille);
            }
            let (restantes, abandonnees) = regrouper(file);
            file = restantes;
            // Le préchargement passe après tout ce que l'utilisateur demande
            // (tri stable).
            file.sort_by_key(Commande::rang);
            self.terminer(abandonnees);

            let commande = file.remove(0);
            // Un dossier qu'on vient de lire se précharge ensuite.
            let a_precharger = match &commande {
                Commande::OuvrirDossier(chemin) => Some(chemin.clone()),
                Commande::Veille => etabli.ouvert.clone(),
                Commande::Signale => Some(dossier_surveille(&etabli)),
                _ => None,
            };
            let est_veille = matches!(commande, Commande::Veille);
            let comptee = commande.comptee();
            let issue = self.traiter(commande, &mut etabli);
            if est_veille {
                veille = Instant::now() + VEILLE;
            }
            let perdue = matches!(issue, Some(Issue::Echec { session_perdue: true, .. }));
            self.terminer(comptee as usize);
            let issue = match issue {
                Some(Issue::Precharge { chemin, reste }) => {
                    if reste {
                        file.push(Commande::Precharger { chemin });
                    }
                    None
                }
                autre => autre,
            };
            if let (false, Some(chemin)) = (perdue, a_precharger) {
                file.push(Commande::Precharger { chemin });
            }
            if perdue {
                // Les commandes en attente ne seront pas traitées : chacune
                // rend son issue avant l'échec, qui ferme la session — après
                // lui, l'interface ne les écouterait plus.
                let restantes: Vec<Commande> = file.drain(..).chain(reception.try_iter()).collect();
                self.terminer(restantes.iter().filter(|c| c.comptee()).count());
                if let Some(issue) = issue {
                    self.abandonner_toutes(restantes, &issue);
                    self.remettre(issue);
                }
                return;
            }
            if let Some(issue) = issue {
                self.remettre(issue);
            }
        }
        etabli.client.fermer();
    }

    /// Connexion, authentification et relevé de l'arborescence.
    fn etablir(&self, p: &Identifiants) -> Result<Etabli, Issue> {
        let perdue = |etape, message| Issue::Echec { etape, message, session_perdue: true };
        let magasin = Magasin::ouvrir(&self.profil)
            .map_err(|e| perdue("connexion", format!("index local : {e}")))?;
        let mut client = Client::connecter(&p.hote, PORT_IMAPS)
            .map_err(|e| perdue("connexion", format!("connexion à {} : {e}", p.hote)))?;
        authentifier(&mut client, p).map_err(|e| match e {
            Erreur::Reseau(_) => perdue("connexion", format!("connexion à {} : {e}", p.hote)),
            _ => perdue("identifiants", format!("{} : identifiants refusés ({e})", p.utilisateur)),
        })?;
        synchro::arborescence(&mut client, &magasin, self.compte)
            .map_err(|e| perdue("connexion", format!("lecture des dossiers : {e}")))?;
        Ok(Etabli {
            client,
            magasin,
            annexes: HashMap::new(),
            identites: HashMap::new(),
            ouvert: None,
            menage: None,
            dernier: None,
            differes_reserves: HashMap::new(),
        })
    }

    fn traiter(&self, commande: Commande, e: &mut Etabli) -> Option<Issue> {
        let compte = self.compte;
        match commande {
            Commande::Arborescence => Some(match synchro::arborescence(&mut e.client, &e.magasin, compte) {
                Ok(_) => Issue::Arborescence,
                Err(err) => echec("dossier", format!("lecture des dossiers : {err}"), &err),
            }),
            Commande::OuvrirDossier(chemin) => {
                e.ouvert = Some(chemin.clone());
                Some(match synchro::synchroniser(&mut e.client, &e.magasin, compte, &chemin) {
                    Ok(_) => Issue::Dossier { chemin, veille: false },
                    Err(err) => echec("dossier", format!("ouverture de {chemin} : {err}"), &err),
                })
            }
            Commande::Precharger { chemin } => Some(match self.precharger(e, &chemin) {
                Ok(reste) => Issue::Precharge { chemin, reste },
                Err(err) if err.reseau() => echec("reseau", format!("préchargement de {chemin} : {err}"), &err),
                // Une erreur d'index ou de disque n'arrête rien : le message se
                // relira sur le serveur.
                Err(_) => Issue::Precharge { chemin, reste: false },
            }),
            Commande::Corps { chemin, uid, brut, distantes, numero } => {
                let atelier = dossier_affichage(&self.profil);
                let cache = Cache::du_profil(&self.profil);
                match lire_corps(e, compte, &chemin, uid, brut, distantes, &atelier, &cache, numero) {
                    Ok(Some((lu, non_lu))) => Some(Issue::Corps { chemin, uid, lu, non_lu, numero }),
                    // Un autre message a été demandé entre-temps.
                    Ok(None) => None,
                    Err(err) => Some(echec("message", format!("lecture du message {uid} : {err}"), &err)),
                }
            }
            Commande::Piece { chemin, uid, indice, destination, joindre } => {
                let ouvrir = destination.is_none() && !joindre;
                let dossier = dossier_travail_piece(&self.profil, compte, uid, indice, joindre);
                Some(match ecrire_piece(e, &self.profil, compte, &chemin, uid, indice, destination, &dossier, ouvrir) {
                    Ok(fichier) => Issue::Piece { fichier, ouvrir, joindre },
                    Err(err) => echec("piece", format!("pièce jointe : {err}"), &err),
                })
            }
            Commande::MarquerSuivi { chemin, uids, suivi } => {
                Some(match synchro::marquer_suivi(&mut e.client, &e.magasin, compte, &chemin, &uids, suivi) {
                    Ok(()) => Issue::Marque { discret: false },
                    Err(err) => echec("tri", format!("drapeau de suivi : {err}"), &err),
                })
            }
            Commande::MarquerCategorie { chemin, uids, mot_cle, present } => Some(
                match synchro::marquer_categorie(&mut e.client, &e.magasin, compte, &chemin, &uids, &mot_cle, present) {
                    Ok(()) => Issue::Marque { discret: false },
                    Err(err) => echec("tri", format!("catégorie : {err}"), &err),
                },
            ),
            Commande::Catalogue { boite, changement } => Some(
                match synchro::changer_catalogue(&mut e.client, &e.magasin, compte, &boite, &changement) {
                    // Le catalogue rangé a pu changer : l'interface le relit.
                    Ok(()) => Issue::Marque { discret: true },
                    Err(err) => echec("tri", format!("catégories : {err}"), &err),
                },
            ),
            Commande::Confirmer { chemin, uid, identifiants } => Some(match self.confirmer(e, &chemin, uid, identifiants) {
                Ok(()) => Issue::Marque { discret: false },
                Err(message) => Issue::Echec { etape: "message", message, session_perdue: false },
            }),
            Commande::EnvoyerDifferes { identifiants } => match self.envoyer_differes(e, &identifiants) {
                Ok((0, _)) => None,
                Ok((nombre, erreurs)) => Some(Issue::DifferesEnvoyes { nombre, erreurs: erreurs.join(" ; ") }),
                Err(message) => Some(Issue::DifferesEnvoyes { nombre: 0, erreurs: message }),
            },
            Commande::MarquerLu { chemin, uids, lu, discret } => {
                Some(match synchro::marquer_lu(&mut e.client, &e.magasin, compte, &chemin, &uids, lu) {
                    Ok(()) => Issue::Marque { discret },
                    Err(err) => echec("tri", format!("marquage : {err}"), &err),
                })
            }
            Commande::Deplacer { source, uids, cible } => {
                if let Some(identifiants) = cible.identifiants.clone() {
                    e.identites.insert(cible.compte, identifiants);
                }
                let resultat = if cible.compte == compte {
                    deplacement::dans_la_boite(&mut e.client, &e.magasin, compte, &source, &uids, &cible.chemin)
                } else {
                    self.entre_boites(e, &source, &uids, &cible)
                };
                // Les compteurs des deux dossiers ont changé.
                let _ = synchro::arborescence(&mut e.client, &e.magasin, compte);
                Some(match resultat {
                    Ok(rapport) => Issue::Deplace { cible: cible.compte, chemin_cible: cible.chemin, rapport },
                    Err(err) => {
                        let issue = self.echec_de_tri(e, cible.compte, err);
                        // Avant l'échec, qui peut fermer la session : l'interface
                        // doit remettre les lignes quoi qu'il arrive ensuite.
                        if let Issue::Echec { message, .. } = &issue {
                            self.remettre(Issue::DeplacementEchoue { message: message.clone() });
                        }
                        issue
                    }
                })
            }
            Commande::ViderDossiers { chemins } => {
                let mut nombre = 0u32;
                let mut echec_dossier = None;
                for chemin in &chemins {
                    match e.client.vider(chemin) {
                        Ok(k) => nombre += k,
                        Err(err) => {
                            echec_dossier = Some((chemin.clone(), err));
                            break;
                        }
                    }
                }
                // Les compteurs des dossiers vidés ont changé.
                let _ = synchro::arborescence(&mut e.client, &e.magasin, compte);
                Some(match echec_dossier {
                    None => Issue::Vide { nombre },
                    Some((chemin, err)) => {
                        let echec_imap = Echec::Imap(err);
                        echec("tri", format!("vidage de {chemin} : {echec_imap}"), &echec_imap)
                    }
                })
            }
            Commande::CreerDossier { parent, chemin, nom } => {
                Some(match e.client.creer(&chemin) {
                    Ok(()) => {
                        // L'abonnement n'est qu'une commodité pour les autres
                        // logiciels : son refus n'annule pas la création.
                        let _ = e.client.abonner(&chemin);
                        let _ = synchro::arborescence(&mut e.client, &e.magasin, compte);
                        Issue::DossierCree { parent, chemin, nom }
                    }
                    Err(err) => {
                        let echec_imap = Echec::Imap(err);
                        echec("dossier", format!("création de « {nom} » : {echec_imap}"), &echec_imap)
                    }
                })
            }
            Commande::Reprendre(identites) => {
                e.identites.extend(identites);
                self.reprendre(e)
            }
            Commande::Preparer { chemin, uid, mode, jeton, autres } => Some(match self.preparer(e, &chemin, uid, &mode, &autres) {
                Ok(p) => Issue::Prepare { jeton, contenu: serde_json::to_string(&p).unwrap_or_default() },
                Err(message) => Issue::EchecRedaction { jeton, message },
            }),
            Commande::Envoyer { redaction, identifiants: _ } if redaction.envoi_differe > maintenant() => {
                let jeton = redaction.jeton.clone();
                Some(match self.programmer(e, &redaction) {
                    Ok(()) => Issue::Programme { jeton, echeance: redaction.envoi_differe },
                    Err(message) => Issue::EchecRedaction { jeton, message },
                })
            }
            Commande::Envoyer { redaction, identifiants } => self.lancer_envoi(redaction, identifiants),
            Commande::ApresEnvoi { redaction, copie } => {
                let avertissement = self.apres_envoi(e, &redaction, &copie);
                Some(Issue::Envoye { jeton: redaction.jeton, avertissement })
            }
            Commande::Brouillon { redaction } => {
                let jeton = redaction.jeton.clone();
                Some(match self.enregistrer_brouillon(e, &redaction) {
                    Ok(uid) => Issue::BrouillonEnregistre { jeton, uid },
                    Err(message) => Issue::EchecRedaction { jeton, message },
                })
            }
            Commande::Signale => {
                let chemin = dossier_surveille(e);
                let ouvert = e.ouvert.as_deref() == Some(chemin.as_str());
                match synchro::synchroniser(&mut e.client, &e.magasin, compte, &chemin) {
                    Ok(bilan) if bilan.change_la_liste() && ouvert => Some(Issue::Dossier { chemin, veille: true }),
                    // Boîte de réception surveillée en fond : ses compteurs ont
                    // changé, l'arborescence est à relire.
                    Ok(bilan) if bilan.change_la_liste() => Some(Issue::Arborescence),
                    Ok(_) => None,
                    Err(err) => Some(echec("reseau", format!("nouveaux messages : {err}"), &err)),
                }
            }
            Commande::Veille => {
                if let Err(err) = synchro::arborescence(&mut e.client, &e.magasin, compte) {
                    return Some(echec("reseau", format!("veille : {err}"), &err));
                }
                // Une reprise faite pendant la veille se remet à part ; la
                // veille continue.
                if let Some(issue) = self.reprendre(e) {
                    if matches!(issue, Issue::Echec { session_perdue: true, .. }) {
                        return Some(issue);
                    }
                    self.remettre(issue);
                }
                match e.ouvert.clone() {
                    Some(chemin) => Some(match synchro::synchroniser(&mut e.client, &e.magasin, compte, &chemin) {
                        Ok(bilan) if bilan.change_la_liste() => Issue::Dossier { chemin, veille: true },
                        Ok(_) => Issue::Arborescence,
                        Err(err) => echec("reseau", format!("veille : {err}"), &err),
                    }),
                    None => Some(Issue::Arborescence),
                }
            }
        }
    }

    fn entre_boites(
        &self,
        e: &mut Etabli,
        source: &str,
        uids: &[u32],
        cible: &Cible,
    ) -> Result<Rapport, Echec> {
        let annexe = annexe(e, cible.compte)?;
        // L'annexe est retirée le temps de l'opération : les deux sessions
        // sont empruntées en même temps.
        let mut annexe = annexe;
        let resultat = deplacement::entre_boites(
            &mut e.client,
            &mut annexe,
            &e.magasin,
            self.compte,
            source,
            uids,
            cible.compte,
            &cible.chemin,
            &self.atelier,
        );
        e.annexes.insert(cible.compte, annexe);
        resultat
    }

    /// Reprend les déplacements interrompus dont ce compte est la source.
    fn reprendre(&self, e: &mut Etabli) -> Option<Issue> {
        let operations = e.magasin.operations(self.compte).ok()?;
        if operations.is_empty() {
            return None;
        }
        let mut erreurs = Vec::new();
        let mut soldees = 0;
        for operation in operations {
            if !e.identites.contains_key(&operation.compte_cible) {
                // Cible pas encore connectée : la reprise attendra.
                continue;
            }
            let mut annexe = match annexe(e, operation.compte_cible) {
                Ok(a) => a,
                Err(err) => {
                    erreurs.push(err.to_string());
                    continue;
                }
            };
            let resultat = deplacement::reprendre(&mut e.client, &mut annexe, &e.magasin, &operation);
            e.annexes.insert(operation.compte_cible, annexe);
            match resultat {
                Ok(()) => soldees += 1,
                Err(err) => {
                    let _ = e.magasin.noter_echec_operation(operation.id, &err.to_string());
                    if err.reseau() && e.client.noop().is_err() {
                        return Some(echec("reseau", format!("reprise : {err}"), &err));
                    }
                    e.annexes.remove(&operation.compte_cible);
                    erreurs.push(err.to_string());
                }
            }
        }
        if soldees == 0 && erreurs.is_empty() {
            return None;
        }
        let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        Some(Issue::Repris { rapport: Rapport { deplaces: soldees, absents: 0, erreurs } })
    }

    /// Un déplacement a échoué : si c'est la boîte cible qui a lâché, la
    /// session de la source reste valable.
    fn echec_de_tri(&self, e: &mut Etabli, cible: i64, err: Echec) -> Issue {
        if err.reseau() {
            e.annexes.remove(&cible);
            if e.client.noop().is_ok() {
                return Issue::Echec {
                    etape: "tri",
                    message: format!(
                        "déplacement interrompu ({err}) : ce qui n'a pas été déplacé est resté à sa place"
                    ),
                    session_perdue: false,
                };
            }
        }
        echec("tri", format!("déplacement : {err}"), &err)
    }
}

/// Ce qui met fin à l'attente du fil d'un compte.
enum Reveil {
    Commande(Commande),
    Veille,
    /// Le serveur a signalé un changement du dossier ouvert.
    Signal,
    /// L'interface a lâché la session.
    Fermee,
    Perdue(Issue),
}

/// Dossier qu'un compte surveille en `IDLE` : celui qu'on y a ouvert, à défaut
/// sa boîte de réception.
fn dossier_surveille(e: &Etabli) -> String {
    e.ouvert.clone().unwrap_or_else(|| "INBOX".into())
}

/// Vrai si une ligne reçue pendant `IDLE` annonce un changement du dossier :
/// message arrivé ou retiré, drapeaux changés. « * OK Still here » n'en est pas un.
fn signale_un_changement(ligne: &str) -> bool {
    let ligne = ligne.to_ascii_uppercase();
    ligne.starts_with("* ")
        && (ligne.ends_with(" EXISTS")
            || ligne.ends_with(" EXPUNGE")
            || ligne.starts_with("* VANISHED")
            || ligne.contains(" FETCH "))
}

/// Le message entier : gardé sur le poste, ou dernier lu par ce fil, sinon relu
/// sur le serveur. Une pièce jointe, une réponse, un transfert ou une
/// confirmation de lecture ne retéléchargent plus un message déjà là.
fn message_entier(e: &mut Etabli, profil: &str, compte: i64, chemin: &str, uid: u32) -> Result<Vec<u8>, Echec> {
    let id = e.magasin.dossier_id(compte, chemin)?;
    if let Some((mid, _)) = e.magasin.garde_sur_le_poste(id, uid)? {
        if let Some(octets) = Cache::du_profil(profil).lire(mid) {
            return Ok(octets);
        }
    }
    let validite = e.magasin.dossier(id)?.map(|d| d.uid_validity).unwrap_or(0);
    if let Some((c, v, u, octets)) = &e.dernier {
        if c == chemin && *v == validite && *u == uid {
            return Ok(octets.clone());
        }
    }
    assurer_selection(&mut e.client, chemin)?;
    let octets = e.client.corps(uid)?;
    e.dernier = Some((chemin.to_string(), validite, uid, octets.clone()));
    Ok(octets)
}

/// En-têtes lus sur les messages du dossier des envois différés.
const CHAMPS_DIFFERE: &str = "X-MMAIL-ENVOI-DIFFERE X-MMAIL-POSTE-DIFFERE MESSAGE-ID";

/// Au-delà, une réservation qui n'a pas abouti est tenue pour un envoi
/// interrompu (arrêt du poste, réponse perdue).
const RESERVATION_MAX: i64 = 10 * 60;

/// Sur un serveur sans CONDSTORE, délai après l'échéance au-delà duquel un
/// autre poste que celui d'origine envoie le message (celui-ci est fermé).
const RELAIS_AUTRE_POSTE: i64 = 15 * 60;

/// Identifiant du poste et du profil qui programment un envoi différé : deux
/// sessions d'un même serveur RDS n'ont pas le même.
fn poste_differe(profil: &str) -> String {
    let empreinte = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        format!("{}\n{profil}", crate::poste::nom()).as_bytes(),
    );
    empreinte.as_ref()[..8].iter().map(|o| format!("{o:02x}")).collect()
}

/// Valeur d'un champ dans les en-têtes lus par `champs`.
fn champ_differe(brut: &[u8], nom: &str) -> Option<String> {
    let texte = String::from_utf8_lossy(brut);
    texte.lines().find_map(|ligne| {
        let (cle, valeur) = ligne.split_once(':')?;
        cle.trim().eq_ignore_ascii_case(nom).then(|| valeur.trim().to_string())
    })
}

/// Messages du dossier des envois différés arrivés à échéance, avec leur
/// `MODSEQ`. Sont exclus ceux qu'un client a réservés (`\Draft`) ou retirés
/// (`\Deleted`, réservation des versions antérieures à la 0.6.4) ; et, sur un
/// serveur sans CONDSTORE — où la réservation ne départage pas deux postes —,
/// ceux qu'un autre poste a programmés, tant qu'il a le temps de les envoyer.
fn differes_echus(
    entetes: &[crate::protocole::Entete],
    maintenant: i64,
    condstore: bool,
    poste: &str,
) -> Vec<(u32, u64)> {
    entetes
        .iter()
        .filter(|en| !en.supprime() && !en.reserve())
        .filter(|en| {
            let Some(echeance) = redaction::echeance(&en.brut) else { return false };
            if echeance > maintenant {
                return false;
            }
            condstore
                || maintenant >= echeance + RELAIS_AUTRE_POSTE
                || champ_differe(&en.brut, redaction::ENTETE_POSTE).map_or(true, |p| p == poste)
        })
        .map(|en| (en.uid, en.modseq))
        .collect()
}

/// Messages réservés dont l'envoi n'a pas abouti : `vus` garde l'heure où ce
/// fil a vu chacun réservé pour la première fois ; rend ceux qui le sont
/// depuis `RESERVATION_MAX`, avec leur `Message-ID`.
fn differes_interrompus(
    entetes: &[crate::protocole::Entete],
    vus: &mut HashMap<u32, i64>,
    maintenant: i64,
) -> Vec<(u32, String)> {
    let reserves: Vec<(u32, String)> = entetes
        .iter()
        .filter(|en| en.reserve() && !en.supprime() && redaction::echeance(&en.brut).is_some())
        .map(|en| (en.uid, champ_differe(&en.brut, "Message-ID").unwrap_or_default()))
        .collect();
    vus.retain(|uid, _| reserves.iter().any(|(u, _)| u == uid));
    reserves
        .into_iter()
        .filter(|(uid, _)| maintenant - *vus.entry(*uid).or_insert(maintenant) >= RESERVATION_MAX)
        .collect()
}

/// Session annexe ouverte sur une autre boîte, retirée de la réserve pour
/// l'opération en cours.
fn annexe(e: &mut Etabli, compte: i64) -> Result<Client, Echec> {
    if let Some(mut client) = e.annexes.remove(&compte) {
        // Gardée depuis le dernier déplacement : le serveur a pu la fermer
        // entre-temps (Dovecot coupe une session inactive). Un premier dépôt
        // échouait alors, et les messages suivants n'étaient pas traités.
        if client.noop().is_ok() {
            return Ok(client);
        }
    }
    let identifiants = e
        .identites
        .get(&compte)
        .cloned()
        .ok_or_else(|| Echec::Imap(Erreur::Refuse("boîte cible non connectée".into())))?;
    Ok(ouvrir_imap(&identifiants)?)
}

// ------------------------------------------------------------- rédaction

impl Travail {
    /// Chemin du dossier de ce compte qui tient un rôle (« Drafts », « Sent »,
    /// « Trash ») : cf. `dossier_de_role`.
    fn dossier_de_role(&self, e: &Etabli, role: &str) -> Option<String> {
        dossier_de_role(&e.magasin.dossiers(self.compte).ok()?, role)
    }

    /// Fait une opération dans un autre dossier que celui qui est sélectionné,
    /// puis y revient : la veille relit le dossier sélectionné, qui doit rester
    /// celui que l'utilisateur regarde.
    fn ailleurs<T>(
        &self,
        e: &mut Etabli,
        chemin: &str,
        operation: impl FnOnce(&mut Client) -> Result<T, Erreur>,
    ) -> Result<T, String> {
        let avant = e.client.selection().map(str::to_string);
        assurer_selection(&mut e.client, chemin).map_err(|x| x.to_string())?;
        let resultat = operation(&mut e.client).map_err(|x| x.to_string());
        if let Some(avant) = avant {
            let _ = assurer_selection(&mut e.client, &avant);
        }
        resultat
    }

    /// Dossier des envois différés (« Envoi différé », à la racine), créé au
    /// besoin si `creer`.
    fn dossier_differe(&self, e: &mut Etabli, creer: bool) -> Result<Option<String>, String> {
        let trouve = e
            .magasin
            .dossiers(self.compte)
            .map_err(|x| x.to_string())?
            .into_iter()
            .find(|d| d.profondeur == 0 && d.nom == DOSSIER_DIFFERE)
            .map(|d| d.chemin);
        if trouve.is_some() || !creer {
            return Ok(trouve);
        }
        let chemin = crate::protocole::encoder_utf7(DOSSIER_DIFFERE);
        e.client.creer(&chemin).map_err(|x| format!("création de « {DOSSIER_DIFFERE} » : {x}"))?;
        synchro::arborescence(&mut e.client, &e.magasin, self.compte).map_err(|x| x.to_string())?;
        Ok(Some(chemin))
    }

    /// Retire définitivement un message repris (brouillon, envoi différé) :
    /// il est remplacé par ce qui vient d'être enregistré ou envoyé.
    fn effacer_repris(&self, e: &mut Etabli, r: &Redaction) -> Result<(), String> {
        if r.brouillon_uid == 0 {
            return Ok(());
        }
        let chemin = if r.brouillon_chemin.is_empty() {
            self.dossier_de_role(e, "Drafts").ok_or("ce compte n'a pas de dossier des brouillons")?
        } else {
            r.brouillon_chemin.clone()
        };
        let uid = r.brouillon_uid;
        self.ailleurs(e, &chemin, |c| c.supprimer(&[uid]))
    }

    /// Répond à une demande de confirmation de lecture, puis pose `$MDNSent`
    /// pour qu'elle ne soit plus posée — ici ni ailleurs.
    fn confirmer(&self, e: &mut Etabli, chemin: &str, uid: u32, id: Option<Identifiants>) -> Result<(), String> {
        if let Some(id) = id {
            let octets = message_entier(e, &self.profil, self.compte, chemin, uid).map_err(|x| x.to_string())?;
            let (destinataire, rapport) =
                redaction::confirmation_lecture(&octets, &id.utilisateur, "", maintenant())
                    .ok_or("ce message ne demande pas de confirmation de lecture")?;
            remettre_message(&id, &id.utilisateur, &[destinataire], &rapport, false)
                .map_err(|x| format!("confirmation de lecture : {x}"))?;
        }
        self.ailleurs(e, chemin, |c| c.marquer(&[uid], "$MDNSent", true))
    }

    /// Envoie les messages différés arrivés à échéance. Rend combien, et les
    /// erreurs de ceux qui n'ont pas pu partir (ils restent pour la prochaine
    /// fois).
    fn envoyer_differes(&self, e: &mut Etabli, id: &Identifiants) -> Result<(usize, Vec<String>), String> {
        let Some(dossier) = self.dossier_differe(e, false)? else {
            return Ok((0, Vec::new()));
        };
        let avant = e.client.selection().map(str::to_string);
        let resultat = (|| -> Result<(usize, Vec<String>), String> {
            let selection = e.client.selectionner(&dossier, None).map_err(|x| x.to_string())?;
            if selection.etat.messages == 0 {
                return Ok((0, Vec::new()));
            }
            let maintenant = maintenant();
            let entetes = e.client.champs("1:*", CHAMPS_DIFFERE).map_err(|x| x.to_string())?;
            let condstore = e.client.condstore();
            let echus = differes_echus(&entetes, maintenant, condstore, &poste_differe(&self.profil));
            let interrompus = differes_interrompus(&entetes, &mut e.differes_reserves, maintenant);
            let envoyes = self.dossier_de_role(e, "Sent");
            let (mut nombre, mut erreurs) = (0, Vec::new());
            for (uid, modseq) in echus {
                // Réservé d'abord : tout MMail ouvert sur le compte fait ce même
                // relevé chaque minute. Celui qui pose `\Draft` le premier
                // envoie ; les autres voient le message modifié et passent.
                match e.client.reserver(uid, modseq) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(x) => {
                        erreurs.push(format!("message {uid} : {x}"));
                        continue;
                    }
                }
                let envoi = e
                    .client
                    .corps(uid)
                    .map_err(|x| x.to_string())
                    .and_then(|brut| redaction::preparer_envoi_differe(&brut, maintenant));
                let envoi = match envoi {
                    Ok(envoi) => envoi,
                    Err(x) => {
                        let _ = e.client.marquer(&[uid], "\\Draft", false);
                        erreurs.push(format!("message {uid} : {x}"));
                        continue;
                    }
                };
                match remettre_message(id, &envoi.de, &envoi.destinataires, &envoi.envoi, envoi.accuse) {
                    Ok(()) => {}
                    // Refusé par le serveur : pas parti. La réservation est
                    // levée, il repartira à la prochaine minute.
                    Err(x @ Erreur::Refuse(_)) => {
                        let _ = e.client.marquer(&[uid], "\\Draft", false);
                        erreurs.push(format!("message {uid} : {x}"));
                        continue;
                    }
                    // Coupure : il a pu partir. Il reste réservé, et sera
                    // vérifié dans « Éléments envoyés » avant d'être repris.
                    Err(x) => {
                        erreurs.push(format!("message {uid} : envoi incertain ({x}) ; vérifié dans 10 min avant d'être repris"));
                        continue;
                    }
                }
                nombre += 1;
                // Graph range lui-même la copie d'un compte Microsoft, Gmail
                // celle d'un compte Google.
                if let (Some(envoyes), None) = (&envoyes, &id.oauth) {
                    let _ = e.client.deposer(envoyes, &["\\Seen".into()], "", &envoi.copie);
                }
                // Parti : il quitte « Envoi différé ». S'il y reste, il garde
                // `\Draft` et ne repartira pas avant d'être retrouvé dans
                // « Éléments envoyés ».
                if let Err(x) = e.client.supprimer(&[uid]) {
                    erreurs.push(format!("message {uid} envoyé mais resté dans « {DOSSIER_DIFFERE} » : {x}"));
                }
            }
            // Envois interrompus : partis s'ils sont dans « Éléments envoyés »
            // (ils quittent alors « Envoi différé »), repris sinon.
            for (uid, message_id) in interrompus {
                e.differes_reserves.remove(&uid);
                let lisible = !message_id.is_empty() && !message_id.contains(['\r', '\n', '"', '\\']);
                let parti = match (&envoyes, lisible) {
                    (Some(envoyes), true) => {
                        self.ailleurs(e, envoyes, |c| c.chercher_message_id(&message_id)).map(|v| !v.is_empty())
                    }
                    _ => Err("dossier des éléments envoyés introuvable".to_string()),
                };
                let _ = assurer_selection(&mut e.client, &dossier);
                match parti {
                    Ok(true) => {
                        let _ = e.client.supprimer(&[uid]);
                    }
                    Ok(false) => {
                        let _ = e.client.marquer(&[uid], "\\Draft", false);
                        erreurs.push(format!("message {uid} : envoi interrompu, repris"));
                    }
                    Err(x) => erreurs.push(format!(
                        "message {uid} : envoi interrompu, à vérifier ({x}) ; il reste réservé dans « {DOSSIER_DIFFERE} »"
                    )),
                }
            }
            Ok((nombre, erreurs))
        })();
        if let Some(avant) = avant {
            let _ = assurer_selection(&mut e.client, &avant);
        }
        if resultat.as_ref().is_ok_and(|(n, _)| *n > 0) {
            let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        }
        resultat
    }

    /// Garde sur le poste, par lot de quelques messages, ceux d'un dossier reçus
    /// depuis moins d'un mois (décision 17), et retire ceux qui sont sortis de
    /// la fenêtre. Le dossier ouvert reste sélectionné. Rend vrai s'il en
    /// reste à garder.
    fn precharger(&self, e: &mut Etabli, chemin: &str) -> Result<bool, Echec> {
        const LOT: usize = 8;
        const OCTETS_PAR_LOT: usize = 8 * 1024 * 1024;
        let cache = Cache::du_profil(&self.profil);
        let limite = crate::cache::limite(maintenant());
        // Une fois par heure : la fenêtre se compte en jours, et ce relevé
        // parcourait l'index à chaque lot de huit messages.
        if e.menage.is_none_or(|t| t.elapsed() > Duration::from_secs(3600)) {
            for id in e.magasin.gardes_perimes(limite)? {
                cache.retirer(id);
                e.magasin.poser_etat_corps(id, "headers")?;
            }
            e.menage = Some(Instant::now());
        }
        let dossier = e.magasin.dossier_id(self.compte, chemin)?;
        let a_faire = e.magasin.a_garder(dossier, limite, LOT + 1, crate::cache::taille_max())?;
        if a_faire.is_empty() {
            return Ok(false);
        }
        let avant = e.client.selection().map(str::to_string);
        assurer_selection(&mut e.client, chemin)?;
        let mut faits = 0;
        let mut octets_lus = 0;
        let mut ecriture_impossible = false;
        for (id, uid) in a_faire.iter().take(LOT) {
            match e.client.corps(*uid) {
                Ok(octets) => {
                    octets_lus += octets.len();
                    if cache.ecrire(*id, &octets).is_err() {
                        // Disque plein, droits : le message reste à garder, et
                        // le préchargement s'arrête jusqu'à la veille suivante
                        // — marqué en échec, il ne l'aurait jamais été (0.6.6).
                        ecriture_impossible = true;
                        break;
                    }
                    e.magasin.poser_etat_corps(*id, "full")?;
                }
                Err(Erreur::Reseau(m)) => return Err(Echec::Imap(Erreur::Reseau(m))),
                // Disparu entre-temps, ou refusé : pas de nouvel essai.
                Err(_) => e.magasin.poser_etat_corps(*id, "echec")?,
            }
            faits += 1;
            if octets_lus > OCTETS_PAR_LOT {
                break;
            }
        }
        if let Some(avant) = avant {
            if avant != chemin {
                assurer_selection(&mut e.client, &avant)?;
            }
        }
        Ok(!ecriture_impossible && a_faire.len() > faits)
    }

    /// La rédaction, ses images locales autorisées remplacées par `cid:`, et
    /// ces images.
    fn avec_images(&self, r: &Redaction) -> (Redaction, Vec<redaction::ImageIntegree>) {
        if r.html.is_empty() {
            return (r.clone(), Vec::new());
        }
        let profil = self.profil.clone();
        let (html, images) = redaction::integrer_images(&r.html, |adresse| image_autorisee(&profil, adresse));
        (Redaction { html, ..r.clone() }, images)
    }

    fn preparer(&self, e: &mut Etabli, chemin: &str, uid: u32, mode: &str, autres: &[String]) -> Result<Preparation, String> {
        let octets = message_entier(e, &self.profil, self.compte, chemin, uid)
            .map_err(|x| format!("lecture du message : {x}"))?;
        // L'adresse du compte, puis ses autres adresses d'envoi.
        let mut propres: Vec<String> =
            e.magasin.compte_par_id(self.compte).ok().flatten().map(|c| vec![c.adresse]).unwrap_or_default();
        propres.extend(autres.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()));
        let mut p = redaction::preparer(&octets, mode, &propres);
        if mode == "brouillon" {
            p.brouillon_uid = uid;
            p.brouillon_chemin = chemin.to_string();
        } else {
            p.origine_chemin = chemin.to_string();
            p.origine_uid = uid;
        }
        if mode == "brouillon" && !p.html.is_empty() {
            // Images du corps (signature) : remises en fichiers, que la
            // fenêtre de rédaction affiche et que l'envoi réintégrera.
            let dossier = dossier_images_redaction(&self.profil).join(format!("{}-{uid}-{}", self.compte, serie_signature()));
            for (rang, (cid, type_mime, contenu)) in redaction::images_du_brouillon(&octets).into_iter().enumerate() {
                let fichier = dossier.join(format!("i{rang}.{}", crate::rendu::extension_image(&type_mime)));
                if std::fs::create_dir_all(&dossier).and_then(|_| std::fs::write(&fichier, contenu)).is_ok() {
                    p.html = p.html.replace(&format!("cid:{cid}"), &crate::rendu::url_fichier(&fichier));
                }
            }
        }
        if mode != "brouillon" && !p.html.is_empty() {
            // Images du message cité : remises en fichiers, dans le dossier
            // dont l'envoi réintègre les images.
            let dossier = dossier_images_redaction(&self.profil).join(format!("{}-{uid}-{}", self.compte, serie_signature()));
            let mut html = std::mem::take(&mut p.html);
            for (rang, (type_mime, contenu)) in std::mem::take(&mut p.images).into_iter().enumerate() {
                let repere = format!("\"{}{rang}\"", crate::rendu::REPERE_IMAGE);
                let fichier = dossier.join(format!("c{rang}.{}", crate::rendu::extension_image(&type_mime)));
                let adresse = (std::fs::create_dir_all(&dossier).is_ok() && std::fs::write(&fichier, &contenu).is_ok())
                    .then(|| crate::rendu::url_fichier(&fichier));
                html = match adresse {
                    Some(adresse) => html.replace(&repere, &format!("\"{adresse}\"")),
                    None => html.replace(&repere, "\"\""),
                };
            }
            p.html = html;
        }
        if mode == "transferer" || mode == "brouillon" {
            // Pièces reprises : écrites à part, elles se joignent ensuite comme
            // n'importe quel fichier.
            let racine = dossier_pieces(&self.profil)
                .join(format!("redaction-{}-{uid}-{}", self.compte, maintenant()));
            for piece in crate::index::pieces_jointes(&octets) {
                let Some((nom, contenu)) = crate::index::extraire_piece(&octets, piece.indice) else {
                    continue;
                };
                let dossier = racine.join(piece.indice.to_string());
                let fichier = dossier.join(&nom);
                std::fs::create_dir_all(&dossier)
                    .and_then(|_| std::fs::write(&fichier, contenu))
                    .map_err(|x| format!("pièce jointe {nom} : {x}"))?;
                p.pieces.push(fichier.to_string_lossy().into_owned());
            }
        }
        Ok(p)
    }

    /// Range un message dans « Envoi différé », daté de son heure d'envoi.
    fn programmer(&self, e: &mut Etabli, r: &Redaction) -> Result<(), String> {
        let fichiers = lire_fichiers(&r.pieces)?;
        let (r_images, images) = self.avec_images(r);
        let f = redaction::fabriquer_avec_images(&r_images, r.envoi_differe, &fichiers, &images)?;
        if f.destinataires.is_empty() {
            return Err("aucun destinataire".into());
        }
        let dossier = self.dossier_differe(e, true)?.ok_or("dossier des envois différés introuvable")?;
        let copie = [
            format!("{}: {}\r\n", redaction::ENTETE_POSTE, poste_differe(&self.profil)).as_bytes(),
            &f.copie[..],
        ]
        .concat();
        e.client
            .deposer(&dossier, &["\\Seen".into()], "", &copie)
            .map_err(|x| format!("mise en attente : {x}"))?;
        let _ = self.effacer_repris(e, r);
        let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        Ok(())
    }

    /// Fabrique le message, puis l'envoie par SMTP dans un fil à lui : un
    /// gros envoi, ou un serveur lent, tenait le compte entier — ni message
    /// à lire, ni dossier à ouvrir jusqu'à la fin. Rien n'est rendu s'il part :
    /// son issue (`Parti`) revient par l'interface, qui en confie la suite au
    /// fil du compte.
    fn lancer_envoi(&self, r: Redaction, id: Identifiants) -> Option<Issue> {
        let jeton = r.jeton.clone();
        let fabrique = lire_fichiers(&r.pieces).and_then(|fichiers| {
            let (r_images, images) = self.avec_images(&r);
            redaction::fabriquer_avec_images(&r_images, maintenant(), &fichiers, &images)
        });
        let f = match fabrique {
            Ok(f) if f.destinataires.is_empty() => return Some(Issue::EchecRedaction { jeton, message: "aucun destinataire".into() }),
            Ok(f) => f,
            Err(message) => return Some(Issue::EchecRedaction { jeton, message }),
        };
        let retour = self.retour();
        let lance = thread::Builder::new().name("mmail-smtp".into()).spawn(move || {
            let erreur = remettre_message(&id, &r.de, &f.destinataires, &f.envoi, r.accuse_remise)
                .err()
                .map(|x| format!("envoi : {x}"));
            retour.remettre(Issue::Parti { redaction: r, copie: f.copie, erreur }, true);
        });
        match lance {
            Ok(_) => None,
            Err(x) => Some(Issue::EchecRedaction { jeton, message: format!("envoi : {x}") }),
        }
    }

    /// Ce qui suit un envoi réussi. Le message est parti : rien ici ne peut
    /// plus le faire échouer, seulement donner lieu à un avertissement.
    fn apres_envoi(&self, e: &mut Etabli, r: &Redaction, copie: &[u8]) -> String {
        // Ceux à qui l'on écrit seront proposés à la prochaine saisie.
        let destinataires: Vec<(String, String)> = [&r.a, &r.cc, &r.cci]
            .iter()
            .filter_map(|liste| redaction::adresses(liste).ok())
            .flatten()
            .collect();
        let _ = e.magasin.noter_correspondants(&destinataires, maintenant());
        let mut avertissements = Vec::new();
        // Envoyé par Graph ou par Gmail : le service a déjà rangé la copie.
        let copie_rangee = e.magasin.compte_par_id(self.compte).ok().flatten().is_some_and(|c| c.oauth());
        match self.dossier_de_role(e, "Sent") {
            _ if copie_rangee => {}
            Some(envoyes) => {
                if let Err(x) = e.client.deposer(&envoyes, &["\\Seen".into()], "", copie) {
                    avertissements.push(format!("copie dans « {envoyes} » : {x}"));
                }
            }
            None => avertissements.push("aucun dossier des éléments envoyés : pas de copie gardée".into()),
        }
        if let Err(x) = self.effacer_repris(e, r) {
            avertissements.push(format!("brouillon non effacé : {x}"));
        }
        if r.origine_uid > 0 && !r.origine_chemin.is_empty() {
            let drapeaux = drapeaux_d_origine(&r.origine_mode);
            let uid = r.origine_uid;
            let marquage = self.ailleurs(e, &r.origine_chemin, |c| {
                drapeaux.iter().try_for_each(|d| c.marquer(&[uid], d, true))
            });
            if let Err(x) = marquage {
                avertissements.push(format!("marquage du message d'origine : {x}"));
            }
        }
        let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        avertissements.join(" ; ")
    }

    fn enregistrer_brouillon(&self, e: &mut Etabli, r: &Redaction) -> Result<u32, String> {
        let fichiers = lire_fichiers(&r.pieces)?;
        let (r_images, images) = self.avec_images(r);
        let f = redaction::fabriquer_avec_images(&r_images, maintenant(), &fichiers, &images)?;
        let brouillons =
            self.dossier_de_role(e, "Drafts").ok_or("ce compte n'a pas de dossier des brouillons")?;
        let uid = e
            .client
            .deposer(&brouillons, &["\\Draft".into(), "\\Seen".into()], "", &f.copie)
            .map_err(|x| format!("enregistrement du brouillon : {x}"))?;
        // La version précédente s'efface une fois la nouvelle en place.
        let _ = self.effacer_repris(e, r);
        let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        Ok(uid.unwrap_or(0))
    }

}

/// Lit les fichiers à joindre. Un fichier disparu entre-temps arrête tout : un
/// message qui partirait sans l'une de ses pièces serait pire qu'un refus.
fn lire_fichiers(chemins: &[String]) -> Result<Vec<Fichier>, String> {
    chemins
        .iter()
        .map(|chemin| {
            let nom = Path::new(chemin).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            std::fs::read(chemin)
                .map(|contenu| Fichier { nom: nom.clone(), contenu })
                .map_err(|x| format!("pièce jointe « {nom} » illisible : {x}"))
        })
        .collect()
}

/// Nom de fichier d'un nouvel objet de l'agenda, tiré de son UID quand il s'y
/// prête — lettres, chiffres, tiret, point —, aléatoire sinon : un UID
/// d'Outlook contient souvent `/`, `+` ou `=`.
fn nom_d_objet(uid: &str) -> String {
    if !uid.is_empty() && uid.len() <= 120 && uid.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        uid.to_string()
    } else {
        crate::saisie::nouvel_uid()
    }
}

fn maintenant() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Ce qu'il faut à l'interface pour afficher un message lu.
struct Lu {
    texte: String,
    brut: bool,
    /// `texte` est du HTML assaini.
    html: bool,
    /// Images distantes laissées de côté.
    bloquees: usize,
    /// Pièces jointes, en JSON.
    pieces: String,
    /// Adresse à qui confirmer la lecture, si l'expéditeur le demande.
    confirmation: String,
    /// Partie calendrier d'une invitation, d'une réponse ou d'une annulation.
    invitation: Option<crate::invitation::Extrait>,
    /// De, À, Cc et date, pour la fenêtre du message (0.6.16, JSON ; vide
    /// pour la source).
    entete: String,
}

/// Lit le corps d'un message du dossier ouvert. Un message de moins d'un mois
/// est gardé sur le poste au passage. Rend de quoi l'afficher, et vrai si le
/// message, affiché (et non sa source), était non lu : l'interface le marque
/// alors comme lu, comme le fait Outlook.
#[allow(clippy::too_many_arguments)]
fn lire_corps(
    e: &mut Etabli,
    compte: i64,
    chemin: &str,
    uid: u32,
    brut: bool,
    distantes: bool,
    atelier: &Path,
    cache: &Cache,
    numero: u64,
) -> Result<Option<(Lu, bool)>, Echec> {
    let id = e.magasin.dossier_id(compte, chemin)?;
    let garde = e.magasin.garde_sur_le_poste(id, uid)?.and_then(|(mid, lu)| cache.lire(mid).map(|o| (o, lu)));
    let (octets, confirmation_possible) = match garde {
        // Gardé sur le poste — on vient ici pour ses images distantes : la
        // confirmation de lecture se propose comme à l'affichage local, tant
        // que le message n'est pas lu.
        Some((octets, lu)) => (octets, !lu),
        None => {
            assurer_selection(&mut e.client, chemin)?;
            let complet = e
                .client
                .message_complet(uid)?
                .ok_or_else(|| Echec::Imap(Erreur::Refuse(format!("message {uid} disparu"))))?;
            if let Some((mid, horodatage, _)) = e.magasin.identite_message(id, uid)? {
                // Même règle qu'au préchargement : récent, et sous le plafond
                // de taille.
                if horodatage >= crate::cache::limite(maintenant())
                    && complet.octets.len() as i64 <= crate::cache::taille_max()
                    && cache.ecrire(mid, &complet.octets).is_ok()
                {
                    e.magasin.poser_etat_corps(mid, "full")?;
                }
            }
            let validite = e.magasin.dossier(id)?.map(|d| d.uid_validity).unwrap_or(0);
            e.dernier = Some((chemin.to_string(), validite, uid, complet.octets.clone()));
            // Demande de confirmation de lecture, sauf si l'on y a déjà répondu.
            let deja = complet.drapeaux.iter().any(|d| d.eq_ignore_ascii_case("$MDNSent"));
            (complet.octets, !deja)
        }
    };
    let Some(lu) = rendre_si_actuel(numero, &octets, brut, distantes, compte, atelier, confirmation_possible) else {
        return Ok(None);
    };
    // Le message entier dit s'il porte des pièces jointes : cela prime sur ce
    // que ses en-têtes laissaient supposer dans la liste.
    e.magasin.poser_pieces(id, uid, lu.pieces != "[]")?;
    // Le marquage « lu » se décide dans l'interface, qui sait si l'utilisateur
    // a marqué le message entre-temps. Les images distantes d'un message déjà
    // affiché ne le marquent pas : marqué « non lu », il le reste.
    let non_lu = !brut && !distantes && matches!(e.magasin.message(id, uid)?, Some(m) if !m.lu);
    Ok(Some((lu, non_lu)))
}

/// De quoi afficher un message entier : sa source, son HTML assaini et ses
/// images, ou son texte ; ses pièces jointes ; la confirmation de lecture
/// demandée, si `confirmation_possible`. `None` si sa demande n'est plus la
/// dernière (cf. `AFFICHAGE`).
///
/// Seule l'écriture des images tient le verrou de l'atelier : l'analyse et le
/// téléchargement des images distantes se font avant, si bien qu'un hôte
/// d'images lent ne fige plus l'affichage des autres messages ; et un message
/// quitté abandonne ses téléchargements entre deux lots (0.6.6).
fn rendre_si_actuel(
    numero: u64,
    octets: &[u8],
    brut: bool,
    distantes: bool,
    compte: i64,
    atelier: &Path,
    confirmation_possible: bool,
) -> Option<Lu> {
    // Numéro 0 : la fenêtre d'un message (0.6.16), hors du volet de lecture
    // — toujours actuelle.
    let actuel = || numero == 0 || AFFICHAGE.load(Ordering::SeqCst) == numero;
    if !actuel() {
        return None;
    }
    let confirmation = if brut || !confirmation_possible {
        String::new()
    } else {
        redaction::confirmation_demandee(octets).unwrap_or_default()
    };
    let invitation = if brut { None } else { crate::invitation::extraire(octets) };
    let entete = if brut { String::new() } else { crate::index::entete_affichable(octets) };
    let mut lu =
        Lu { texte: String::new(), brut, html: false, bloquees: 0, pieces: String::new(), confirmation, invitation, entete };
    lu.pieces = json_pieces(&crate::index::pieces_jointes(octets));
    if brut {
        lu.texte = String::from_utf8_lossy(octets).into_owned();
    } else if let Some(corps) = crate::rendu::corps_html(octets) {
        let telechargees = if distantes {
            telecharger(&corps.distantes, &actuel)?
        } else {
            vec![None; corps.distantes.len()]
        };
        let _verrou = ATELIER.lock().unwrap_or_else(|e| e.into_inner());
        if !actuel() {
            return None;
        }
        // Les images ne servent qu'à l'affichage : si leur écriture échoue,
        // le message s'affiche sans elles plutôt que pas du tout.
        let (html, bloquees) = poser_images(corps, compte, telechargees, atelier, numero == 0);
        lu.texte = html;
        lu.html = true;
        lu.bloquees = bloquees;
        return Some(lu);
    } else {
        lu.texte = crate::index::corps_affichable(octets);
    }
    actuel().then_some(lu)
}

/// Numéro de la dernière demande d'affichage d'un message, d'où qu'elle vienne.
/// Un rendu commencé pour un message que l'utilisateur a déjà quitté est
/// abandonné : il effacerait, en posant ses images, celles du message affiché.
static AFFICHAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Une seule écriture d'images à la fois dans le dossier d'affichage : le fil
/// du compte et les fils d'affichage local y effacent puis y écrivent.
static ATELIER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Vrai si `chemin` est un dossier présent, sans attendre plus d'une seconde :
/// un partage réseau ou un lecteur réseau dont le serveur ne répond pas fait
/// attendre le système plusieurs secondes, et la question vient du fil de
/// l'interface (0.6.6). Faute de réponse à temps, le dossier est tenu pour
/// absent ; le fil qui le teste finit seul.
fn dossier_present(chemin: PathBuf) -> bool {
    let (envoi, reponse) = std::sync::mpsc::channel();
    let lance = thread::Builder::new().name("mmail-dossier".into()).spawn(move || {
        let _ = envoi.send(chemin.is_dir());
    });
    lance.is_ok() && reponse.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
}

/// Une pièce jointe demandée d'un message gardé sur le poste, de quoi la
/// redemander au serveur si son fichier manque.
struct PieceDuPoste {
    compte: i64,
    chemin: String,
    uid: u32,
    indice: usize,
    destination: Option<PathBuf>,
    joindre: bool,
}

/// Ce qu'il faut, une fois un message du poste rendu, pour terminer son
/// affichage sur le fil Qt.
struct Local {
    compte: i64,
    chemin: String,
    dossier: i64,
    uid: u32,
    lu: bool,
    brut: bool,
    en_ligne: bool,
}

/// Numéro de série des affichages : chaque affichage a son propre dossier, et
/// donc ses propres adresses de fichier — le moteur de Qt garde en cache une
/// image par adresse, et montrerait sinon celle du message précédent.
static SERIE_AFFICHAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Écrit les images d'un message HTML dans le dossier de travail du compte, et
/// remplace leurs repères par l'adresse des fichiers. Les images distantes
/// sont celles déjà téléchargées (`telechargees`, une par adresse) ; les
/// autres cèdent la place à un aplat discret, aux dimensions annoncées. Rend
/// le HTML et le nombre d'images distantes non affichées.
fn poser_images(
    corps: crate::rendu::CorpsHtml,
    compte: i64,
    telechargees: Vec<Option<(String, Vec<u8>)>>,
    atelier: &Path,
    fenetre: bool,
) -> (String, usize) {
    use crate::rendu::{extension_image, REPERE_DISTANTE, REPERE_IMAGE};
    let serie = SERIE_AFFICHAGE.fetch_add(1, Ordering::Relaxed);
    let prefixe = format!("{compte}-");
    // Seuls les affichages précédents de ce compte s'effacent : un autre
    // compte peut être en train d'écrire les siens. Les images d'une fenêtre
    // de message (0.6.16) restent tant qu'elle est ouverte — jusqu'au
    // prochain lancement, qui vide le dossier.
    if !fenetre {
        if let Ok(entrees) = std::fs::read_dir(atelier) {
            for entree in entrees.flatten() {
                if entree.file_name().to_string_lossy().starts_with(&prefixe) {
                    let _ = std::fs::remove_dir_all(entree.path());
                }
            }
        }
    }
    let dossier = atelier.join(if fenetre { format!("fenetre-{serie}") } else { format!("{compte}-{serie}") });
    let pret = std::fs::create_dir_all(&dossier).is_ok();
    let ecrire = |nom: String, octets: &[u8]| -> Option<String> {
        let fichier = dossier.join(nom);
        (pret && std::fs::write(&fichier, octets).is_ok()).then(|| crate::rendu::url_fichier(&fichier))
    };

    let mut html = corps.html;
    for (rang, (type_mime, octets)) in corps.images.iter().enumerate() {
        let adresse = ecrire(format!("i{rang}.{}", extension_image(type_mime)), octets).unwrap_or_default();
        html = html.replace(&format!("\"{REPERE_IMAGE}{rang}\""), &format!("\"{adresse}\""));
    }
    let aplat = ecrire("aplat.png".to_string(), APLAT_PNG).unwrap_or_default();
    let mut bloquees = 0;
    for (rang, image) in telechargees.into_iter().enumerate() {
        let adresse = match image.and_then(|(type_mime, octets)| {
            ecrire(format!("d{rang}.{}", extension_image(&type_mime)), &octets)
        }) {
            Some(adresse) => adresse,
            None => {
                bloquees += 1;
                aplat.clone()
            }
        };
        html = html.replace(&format!("\"{REPERE_DISTANTE}{rang}\""), &format!("\"{adresse}\""));
    }
    (html, bloquees)
}

/// Une image PNG d'un pixel gris très clair : la place d'une image distante
/// non téléchargée.
const APLAT_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x78, 0xf7, 0xee, 0x1d,
    0x00, 0x05, 0x98, 0x02, 0xcb, 0x29, 0x7b, 0x1d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Images distantes d'un message, à la demande de l'utilisateur : HTTPS seul,
/// 5 Mo par image, quatre téléchargements à la fois. Une image qui ne vient
/// pas reste un aplat — rien n'empêche l'affichage du message. `None` si
/// `actuel` cesse d'être vrai : vérifié entre deux lots, il abandonne les
/// téléchargements d'un message que l'utilisateur a quitté.
fn telecharger(adresses: &[String], actuel: &dyn Fn() -> bool) -> Option<Vec<Option<(String, Vec<u8>)>>> {
    const TAILLE: usize = 5 * 1024 * 1024;
    const MAX: usize = 60;
    const EN_PARALLELE: usize = 4;
    let mut resultats: Vec<Option<(String, Vec<u8>)>> = vec![None; adresses.len()];
    let a_faire: Vec<usize> = (0..adresses.len().min(MAX))
        .filter(|&i| adresses[i].to_ascii_lowercase().starts_with("https://"))
        .collect();
    for lot in a_faire.chunks(EN_PARALLELE) {
        if !actuel() {
            return None;
        }
        let lus: Vec<(usize, Option<(String, Vec<u8>)>)> = thread::scope(|portee| {
            let fils: Vec<_> = lot
                .iter()
                .map(|&i| {
                    let adresse = adresses[i].replace("&amp;", "&");
                    portee.spawn(move || {
                        let reponse = crate::http::requete_bornee("GET", &adresse, "image/*", TAILLE).ok()?;
                        let type_mime = reponse.type_contenu.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
                        (reponse.statut == 200 && type_mime.starts_with("image/") && !reponse.corps.is_empty())
                            .then_some((type_mime, reponse.corps))
                    })
                })
                .collect();
            lot.iter().zip(fils).map(|(&i, f)| (i, f.join().ok().flatten())).collect()
        });
        for (i, image) in lus {
            resultats[i] = image;
        }
    }
    Some(resultats)
}


/// Relit un message et écrit l'une de ses pièces jointes : à l'emplacement
/// choisi, ou dans `dossier` pour l'ouvrir. Une pièce exécutable n'est jamais
/// écrite pour être ouverte — le refus est ici, et pas seulement dans
/// l'interface.
#[allow(clippy::too_many_arguments)]
fn ecrire_piece(
    e: &mut Etabli,
    profil: &str,
    compte: i64,
    chemin: &str,
    uid: u32,
    indice: usize,
    destination: Option<PathBuf>,
    dossier: &Path,
    pour_ouvrir: bool,
) -> Result<PathBuf, Echec> {
    let octets = message_entier(e, profil, compte, chemin, uid)?;
    ecrire_piece_de(&octets, indice, destination, dossier, pour_ouvrir)
        .map_err(|m| Echec::Index(crate::magasin::Erreur(m)))
}

/// Écrit l'une des pièces jointes d'un message entier : à l'emplacement
/// choisi, ou dans `dossier` pour l'ouvrir ou la joindre. Une pièce
/// exécutable n'est jamais écrite pour être ouverte (`pour_ouvrir`) — le refus
/// est ici, et pas seulement dans l'interface.
fn ecrire_piece_de(
    octets: &[u8],
    indice: usize,
    destination: Option<PathBuf>,
    dossier: &Path,
    pour_ouvrir: bool,
) -> Result<PathBuf, String> {
    let (nom, contenu) =
        crate::index::extraire_piece(octets, indice).ok_or_else(|| format!("pièce jointe {indice} introuvable"))?;
    let fichier = match destination {
        Some(destination) => destination,
        None => {
            if pour_ouvrir && crate::index::ouverture_risquee(&nom) {
                return Err(format!("{nom} est un programme ou un script : enregistrez-le plutôt que de l'ouvrir"));
            }
            std::fs::create_dir_all(dossier).map_err(|x| format!("{} : {x}", dossier.display()))?;
            dossier.join(&nom)
        }
    };
    std::fs::write(&fichier, &contenu).map_err(|x| format!("{} : {x}", fichier.display()))?;
    marquer_provenance(&fichier);
    Ok(fichier)
}

/// Marque un fichier reçu par courrier comme « provenant d'Internet » (zone 3,
/// flux `Zone.Identifier`) : Office l'ouvre en mode protégé et n'en exécute
/// pas les macros, SmartScreen avertit avant de lancer un programme (0.6.4).
/// Sans effet hors de Windows ou sur un disque qui ne garde pas de flux.
fn marquer_provenance(fichier: &Path) {
    if cfg!(windows) {
        let mut flux = fichier.as_os_str().to_owned();
        flux.push(":Zone.Identifier");
        let _ = std::fs::write(PathBuf::from(flux), b"[ZoneTransfer]\r\nZoneId=3\r\n");
    }
}

// ------------------------------------------------- recherche sur le serveur

/// Attente d'une recherche sur le serveur (0.6.16) : 5 min 30 s, sauf réglage
/// de la machine (`MMAIL_DELAI_RECHERCHE`, en secondes, de 5 à 3 600). Un
/// dossier dont l'index plein texte n'est pas construit fait attendre le
/// serveur que son indexeur ait fini, sans rien envoyer : Mailcow l'attend
/// 300 s (`fts_index_timeout`) avant de répondre `NO [INUSE]`. MMail coupait
/// au bout de 30 s, et le compte passait « erreur » (22/10, LALANNE).
fn delai_recherche() -> Duration {
    static DELAI: OnceLock<Duration> = OnceLock::new();
    *DELAI.get_or_init(|| {
        let secondes = std::env::var("MMAIL_DELAI_RECHERCHE")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|s| (5..=3600).contains(s))
            .unwrap_or(330);
        Duration::from_secs(secondes)
    })
}

/// Première attente d'un dossier, dans une recherche sur plusieurs : au-delà,
/// il passe après les autres, qui n'ont pas à l'attendre. Un dossier dont
/// l'index est construit répond en une seconde ou deux.
const PATIENCE_RECHERCHE: Duration = Duration::from_secs(15);

/// Recherche sur le serveur dans des dossiers d'un compte, par sa propre
/// connexion et dans son propre fil (0.6.16). Elle passait par la session du
/// compte, qui ne faisait rien d'autre en attendant — et qui tombait au bout
/// de 30 s sans réponse, le compte passant « erreur ». Ici, la session du
/// compte n'est pas touchée ; un dossier qui tarde passe après les autres,
/// puis a toute l'attente (`delai`) ; une recherche remplacée s'arrête.
struct Recherche {
    compte: i64,
    texte: String,
    chemins: Vec<String>,
    /// Recherche dans toutes les boîtes : issues `RecherchePartout`.
    partout: bool,
    numero: u64,
    /// Numéro de la recherche en cours, tenu par l'interface.
    courante: Arc<AtomicU64>,
    /// Première attente de chaque dossier quand il y en a plusieurs
    /// (`PATIENCE_RECHERCHE`).
    patience: Duration,
    /// Attente d'un dossier seul, ou des dossiers reportés (`delai_recherche`).
    delai: Duration,
}

/// Ce qu'un dossier a donné.
enum Etape {
    Trouves(Vec<u32>),
    /// Pas de réponse à la première attente : repris après les autres.
    Reporte,
    /// Le serveur n'a pas fini : pourquoi.
    Inacheve(String),
    /// Recherche remplacée : on s'arrête là.
    Abandon,
}

impl Recherche {
    fn actuelle(&self) -> bool {
        self.courante.load(Ordering::SeqCst) == self.numero
    }

    /// Interroge le serveur dossier après dossier et remet une issue pour
    /// chacun. `ouvrir` ouvre une session authentifiée sur le compte.
    fn executer(
        &self,
        ouvrir: &mut dyn FnMut() -> Result<Client, Erreur>,
        magasin: &Magasin,
        remettre: &mut dyn FnMut(Issue),
    ) {
        let (mots, categories) = criteres_recherche(magasin, &self.texte);
        let mut client = None;
        let mut restants = self.chemins.len();
        // Un dossier seul a toute l'attente d'emblée ; à plusieurs, ceux qui
        // tardent passent après les autres.
        let seul = self.chemins.len() == 1;
        let premiere = if seul { self.delai } else { self.patience };
        let mut reportes = Vec::new();
        for chemin in &self.chemins {
            if !self.actuelle() {
                break;
            }
            match self.dans_le_dossier(&mut client, ouvrir, magasin, chemin, (mots.as_str(), categories.as_slice()), premiere, seul) {
                Etape::Reporte => reportes.push(chemin.clone()),
                Etape::Abandon => return,
                etape => {
                    restants -= 1;
                    remettre(self.issue(chemin, etape, restants));
                }
            }
        }
        for chemin in &reportes {
            if !self.actuelle() {
                break;
            }
            match self.dans_le_dossier(&mut client, ouvrir, magasin, chemin, (mots.as_str(), categories.as_slice()), self.delai, true) {
                Etape::Abandon => return,
                etape => {
                    restants -= 1;
                    remettre(self.issue(chemin, etape, restants));
                }
            }
        }
        if let Some(mut client) = client {
            client.fermer();
        }
    }

    /// L'index du poste ne s'ouvre pas : chaque dossier rend son motif.
    fn sans_index(&self, motif: &str, remettre: &mut dyn FnMut(Issue)) {
        for (rang, chemin) in self.chemins.iter().enumerate() {
            let restants = self.chemins.len() - rang - 1;
            remettre(self.issue(chemin, Etape::Inacheve(motif.to_string()), restants));
        }
    }

    fn issue(&self, chemin: &str, etape: Etape, restants: usize) -> Issue {
        let (uids, inacheve) = match etape {
            Etape::Trouves(uids) => (uids, None),
            Etape::Inacheve(motif) => (Vec::new(), Some(motif)),
            Etape::Reporte | Etape::Abandon => (Vec::new(), None),
        };
        let (chemin, texte) = (chemin.to_string(), self.texte.clone());
        if self.partout {
            Issue::RecherchePartout { chemin, texte, uids, restants, inacheve }
        } else {
            Issue::Recherche { chemin, texte, uids, inacheve }
        }
    }

    /// Cherche dans un dossier, et range dans l'index ceux des messages
    /// trouvés qu'il n'a pas encore — les plus récents, `RECHERCHE_MAX` au
    /// plus : un dossier jamais ouvert rend ses résultats comme les autres.
    /// Sans réponse, la session est fermée (elle est au milieu d'une
    /// commande) ; `dernier` : pas de reprise après, l'échec est rendu.
    #[allow(clippy::too_many_arguments)]
    fn dans_le_dossier(
        &self,
        client: &mut Option<Client>,
        ouvrir: &mut dyn FnMut() -> Result<Client, Erreur>,
        magasin: &Magasin,
        chemin: &str,
        (mots, categories): (&str, &[Vec<String>]),
        delai: Duration,
        dernier: bool,
    ) -> Etape {
        let echec = |motif: String| if dernier { Etape::Inacheve(motif) } else { Etape::Reporte };
        if client.is_none() {
            match ouvrir() {
                Ok(c) => *client = Some(c),
                Err(e) => return echec(format!("connexion impossible ({e})")),
            }
        }
        let c = client.as_mut().expect("session de recherche");
        let etat = match c.examiner(chemin) {
            Ok(etat) => etat,
            // Disparu, ou refusé : il ne rend rien.
            Err(Erreur::Refuse(_)) => return Etape::Trouves(Vec::new()),
            Err(e) => {
                *client = None;
                return echec(format!("connexion perdue ({e})"));
            }
        };
        let courante = self.courante.clone();
        let numero = self.numero;
        let attente = Attente {
            delai,
            // Au dernier passage, la progression envoyée par le serveur
            // (`* OK Indexed 40% …`) prolonge l'attente.
            prolonger: dernier,
            continuer: Arc::new(move || courante.load(Ordering::SeqCst) == numero),
        };
        let debut = Instant::now();
        let uids = match c.chercher_patiemment(mots, categories, attente) {
            Ok(uids) => uids,
            Err(Erreur::Abandon) => {
                *client = None;
                return Etape::Abandon;
            }
            Err(Erreur::Delai) => {
                *client = None;
                return echec(format!("pas de réponse du serveur en {}", duree_lisible(debut.elapsed())));
            }
            // Dovecot a attendu son indexeur jusqu'au bout (`fts_index_timeout`).
            Err(Erreur::Refuse(m)) if m.contains("[INUSE]") => {
                return Etape::Inacheve("le serveur prépare encore l'index de recherche de ce dossier".into())
            }
            // Recherche refusée (texte que le serveur ne sait pas chercher…).
            Err(Erreur::Refuse(_)) => return Etape::Trouves(Vec::new()),
            Err(e) => {
                *client = None;
                return echec(format!("connexion perdue ({e})"));
            }
        };
        let Ok(id) = magasin.dossier_id(self.compte, chemin) else {
            return Etape::Trouves(uids);
        };
        // Dossier recréé sur le serveur depuis la dernière synchronisation :
        // ces UID ne désignent plus les messages de l'index, que la session
        // du compte reprendra.
        if magasin.dossier(id).ok().flatten().is_some_and(|d| d.uid_validity != 0 && d.uid_validity != etat.uid_validity) {
            return Etape::Trouves(Vec::new());
        }
        if let Err(e) = synchro::completer(magasin, c, id, &uids, RECHERCHE_MAX) {
            if e.reseau() {
                *client = None;
            }
        }
        Etape::Trouves(uids)
    }
}

/// Une durée pour l'utilisateur : « 45 s », « 5 min », « 5 min 30 s ».
fn duree_lisible(duree: Duration) -> String {
    let secondes = duree.as_secs();
    match (secondes / 60, secondes % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

fn echec(etape: &'static str, message: String, erreur: &Echec) -> Issue {
    let session_perdue = erreur.reseau();
    Issue::Echec {
        etape: if session_perdue { "reseau" } else { etape },
        message,
        session_perdue,
    }
}

// ---------------------------------------------------------------- sérialisation

/// Sérialise un dossier pour l'interface.
/// Vrai si `d` est dans `parent`, à quelque profondeur que ce soit : la
/// parenté se lit au chemin — « A/B/C » est sous « A ».
fn est_sous(parent: &DossierLocal, d: &DossierLocal) -> bool {
    !parent.separateur.is_empty()
        && d.chemin.len() > parent.chemin.len() + parent.separateur.len()
        && d.chemin.starts_with(&parent.chemin)
        && d.chemin[parent.chemin.len()..].starts_with(&parent.separateur)
}

/// Dossiers montrés quand les dossiers masqués sont cachés : ni un dossier
/// masqué, ni rien de ce qu'il contient — ses sous-dossiers réapparaissaient
/// seuls, à la racine (retour de Manu, 01/10).
fn sans_masques(dossiers: Vec<DossierLocal>) -> Vec<DossierLocal> {
    let masques: Vec<DossierLocal> = dossiers.iter().filter(|d| d.masque).cloned().collect();
    dossiers
        .into_iter()
        .filter(|d| !d.masque && !masques.iter().any(|p| est_sous(p, d)))
        .collect()
}

/// Dossiers d'un compte tels que l'arborescence les montre : chacun suivi de
/// ses sous-dossiers — et non plus à sa place dans l'ordre alphabétique, où
/// « INBOX/Banque » tombait après « Eléments infectés » et semblait en
/// dépendre —, rien sous un dossier replié, et pour chacun l'indication
/// qu'il a des sous-dossiers (le chevron). La parenté se lit au chemin :
/// « A/B » est sous « A ». Les dossiers de tête gardent l'ordre de l'index
/// (boîte de réception, dossiers à rôle, puis les autres) ; les
/// sous-dossiers suivent l'ordre alphabétique, sans égard aux majuscules.
fn arbre_visible(dossiers: &[DossierLocal]) -> Vec<(&DossierLocal, bool)> {
    let sous = est_sous;
    // Parent direct : le plus long des dossiers qui contiennent celui-ci.
    let parents: Vec<Option<usize>> = dossiers
        .iter()
        .map(|d| {
            (0..dossiers.len())
                .filter(|&i| sous(&dossiers[i], d))
                .max_by_key(|&i| dossiers[i].chemin.len())
        })
        .collect();
    let enfants = |i: Option<usize>| -> Vec<usize> {
        let mut liste: Vec<usize> = (0..dossiers.len()).filter(|&j| parents[j] == i).collect();
        if i.is_some() {
            liste.sort_by_key(|&j| (dossiers[j].nom.to_lowercase(), j));
        }
        liste
    };
    fn visiter<'a>(
        i: usize,
        dossiers: &'a [DossierLocal],
        enfants: &dyn Fn(Option<usize>) -> Vec<usize>,
        sortie: &mut Vec<(&'a DossierLocal, bool)>,
    ) {
        let directs = enfants(Some(i));
        sortie.push((&dossiers[i], !directs.is_empty()));
        if !dossiers[i].replie {
            for j in directs {
                visiter(j, dossiers, enfants, sortie);
            }
        }
    }
    let mut sortie = Vec::with_capacity(dossiers.len());
    for racine in enfants(None) {
        visiter(racine, dossiers, &enfants, &mut sortie);
    }
    sortie
}

fn json_dossier(genre: &str, d: &DossierLocal, adresse: &str, profondeur: u32, enfants: bool) -> String {
    format!(
        r#"{{"genre":{},"compte":{},"adresse":{},"chemin":{},"nom":{},"separateur":{},"profondeur":{},"role":{},"selectionnable":{},"messages":{},"nonLus":{},"masque":{},"favori":{},"enfants":{},"replie":{}}}"#,
        texte_json(genre),
        d.compte_id,
        texte_json(adresse),
        texte_json(&d.chemin),
        texte_json(&d.nom),
        texte_json(&d.separateur),
        profondeur,
        texte_json(&d.role),
        d.selectionnable,
        d.messages,
        d.non_lus,
        d.masque,
        d.favori.is_some(),
        enfants,
        d.replie
    )
}

/// Sérialise les pièces jointes d'un message pour l'interface.
fn json_pieces(pieces: &[crate::index::PieceJointe]) -> String {
    let corps: Vec<String> = pieces
        .iter()
        .map(|p| {
            format!(
                r#"{{"indice":{},"nom":{},"type":{},"taille":{},"risquee":{}}}"#,
                p.indice,
                texte_json(&p.nom),
                texte_json(&p.type_mime),
                p.taille,
                p.risquee
            )
        })
        .collect();
    format!("[{}]", corps.join(","))
}

/// Résultats d'une recherche dans l'index ; au-delà, l'interface demande de
/// préciser.
const RECHERCHE_MAX: usize = 500;

/// Boîte de réception unifiée : les messages les plus récents de toutes les
/// boîtes de réception, au plus.
const UNIFIEE_MAX: usize = 1000;

/// Messages trouvés dans tous les comptes : la ligne d'une liste, plus son
/// compte, son chemin et le nom de son dossier.
/// Drapeaux posés sur le message d'origine une fois la réponse ou le
/// transfert parti.
fn drapeaux_d_origine(mode: &str) -> &'static [&'static str] {
    match mode {
        "transferer" => &["$Forwarded"],
        "repondre_tous" => &["\\Answered", "$ReplyAll"],
        _ => &["\\Answered"],
    }
}

/// Chemin d'un dossier découpé en noms lisibles, du premier niveau au dossier
/// lui-même : « INBOX.&AMk-l&AOk-ments envoy&AOk-s » → « INBOX »,
/// « Éléments envoyés ».
fn segments_du_chemin(chemin: &str, separateur: &str) -> Vec<String> {
    match separateur.chars().next() {
        Some(sep) => chemin.split(sep).map(crate::protocole::decoder_utf7).collect(),
        None => vec![crate::protocole::decoder_utf7(chemin)],
    }
}

fn json_trouves(trouves: &[crate::magasin::Trouve]) -> String {
    let corps: Vec<String> = trouves
        .iter()
        .map(|t| {
            let ligne = json_message(&t.message);
            let segments: Vec<String> =
                segments_du_chemin(&t.chemin, &t.separateur).iter().map(|s| texte_json(s)).collect();
            format!(
                r#"{},"compte":{},"chemin":{},"dossier":{},"segments":[{}],"role":{}}}"#,
                &ligne[..ligne.len() - 1],
                t.compte,
                texte_json(&t.chemin),
                texte_json(&t.nom_dossier),
                segments.join(","),
                texte_json(&t.role)
            )
        })
        .collect();
    format!("[{}]", corps.join(","))
}

/// Sérialise une liste de messages pour l'interface.
fn json_messages(messages: &[MessageLocal]) -> String {
    let corps: Vec<String> = messages.iter().map(json_message).collect();
    format!("[{}]", corps.join(","))
}

fn json_message(m: &MessageLocal) -> String {
    format!(
        r#"{{"uid":{},"h":{},"expediteur":{},"adresse":{},"destinataires":{},"sujet":{},"date":{},"taille":{},"lu":{},"repondu":{},"reponduTous":{},"transfere":{},"pieces":{},"suivi":{},"importance":{},"motsCles":{}}}"#,
        m.uid,
        m.horodatage,
        texte_json(&m.expediteur),
        texte_json(&m.adresse),
        texte_json(&m.destinataires),
        texte_json(&m.sujet),
        texte_json(&m.date),
        m.taille,
        m.lu,
        m.repondu,
        m.repondu_tous,
        m.transfere,
        m.pieces,
        m.suivi,
        m.importance,
        texte_json(&m.mots_cles)
    )
}

/// Au-delà, l'interface relit la liste entière plutôt que d'appliquer les
/// changements un à un.
const CHANGEMENTS_MAX: usize = 2000;

/// Réponse de `changements_liste` quand la version du dossier n'a pas bougé.
const AUCUN_CHANGEMENT: &str = r#"{"retires":[],"ajoutes":[],"modifies":[]}"#;

/// UID, date et empreinte de chaque message d'une liste.
fn empreintes_de(liste: &[MessageLocal]) -> Vec<(u32, i64, u64)> {
    liste.iter().map(|m| (m.uid, m.horodatage, empreinte(m))).collect()
}

/// Empreinte des champs affichés d'un message.
fn empreinte(m: &MessageLocal) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.hash(&mut h);
    h.finish()
}

/// Différence entre la liste envoyée (`avant`) et l'index (`apres`, avec ses
/// empreintes) : retirés, ajoutés et modifiés, en JSON ; `None` s'il y en a
/// trop. Un message dont la date a changé est retiré puis ajouté : sa place
/// dans la liste triée n'est plus la même.
fn changements(avant: &[(u32, i64, u64)], apres: &[MessageLocal], empreintes: &[(u32, i64, u64)]) -> Option<String> {
    let connus: HashMap<u32, (i64, u64)> = avant.iter().map(|&(u, h, e)| (u, (h, e))).collect();
    let nouveaux: HashMap<u32, i64> = empreintes.iter().map(|&(u, h, _)| (u, h)).collect();
    let retires: Vec<String> = avant
        .iter()
        .filter(|(u, h, _)| nouveaux.get(u) != Some(h))
        .map(|(u, h, _)| format!(r#"{{"uid":{u},"h":{h}}}"#))
        .collect();
    if retires.len() > CHANGEMENTS_MAX {
        return None;
    }
    let (mut ajoutes, mut modifies) = (Vec::new(), Vec::new());
    for (m, &(_, h, e)) in apres.iter().zip(empreintes) {
        match connus.get(&m.uid) {
            Some(&(h0, e0)) if h0 == h => {
                if e0 != e {
                    modifies.push(m.clone());
                }
            }
            _ => ajoutes.push(m.clone()),
        }
        if retires.len() + ajoutes.len() + modifies.len() > CHANGEMENTS_MAX {
            return None;
        }
    }
    Some(format!(
        r#"{{"retires":[{}],"ajoutes":{},"modifies":{}}}"#,
        retires.join(","),
        json_messages(&ajoutes),
        json_messages(&modifies)
    ))
}

/// Chaîne JSON échappée. Écrite ici pour ne pas dépendre d'un sérialiseur
/// entier là où quelques champs suffisent — et testée pour les cas qui mordent.
fn texte_json(valeur: &str) -> String {
    let mut sortie = String::with_capacity(valeur.len() + 2);
    sortie.push('"');
    for c in valeur.chars() {
        match c {
            '"' => sortie.push_str("\\\""),
            '\\' => sortie.push_str("\\\\"),
            '\n' => sortie.push_str("\\n"),
            '\r' => sortie.push_str("\\r"),
            '\t' => sortie.push_str("\\t"),
            c if (c as u32) < 0x20 => sortie.push_str(&format!("\\u{:04x}", c as u32)),
            // Séparateurs de ligne Unicode : valides en JSON, mais pas dans un
            // littéral JavaScript ancien ; on les échappe par prudence.
            '\u{2028}' => sortie.push_str("\\u2028"),
            '\u{2029}' => sortie.push_str("\\u2029"),
            c => sortie.push(c),
        }
    }
    sortie.push('"');
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(chemin: &str) -> Commande {
        Commande::OuvrirDossier(chemin.into())
    }
    /// Lecture pour le volet : son numéro d'affichage n'est jamais 0.
    fn corps(uid: u32) -> Commande {
        Commande::Corps { chemin: "INBOX".into(), uid, brut: false, distantes: false, numero: uid as u64 }
    }
    fn noms(file: &[Commande]) -> Vec<String> {
        file.iter()
            .map(|c| match c {
                Commande::OuvrirDossier(d) => format!("ouvrir {d}"),
                Commande::Corps { uid, .. } => format!("corps {uid}"),
                Commande::Piece { uid, .. } => format!("piece {uid}"),
                Commande::MarquerLu { .. } => "marquer".into(),
                Commande::MarquerSuivi { .. } => "suivi".into(),
                Commande::MarquerCategorie { .. } => "categorie".into(),
                Commande::Catalogue { .. } => "catalogue".into(),
                Commande::Confirmer { .. } => "confirmer".into(),
                Commande::EnvoyerDifferes { .. } => "differes".into(),
                Commande::Deplacer { .. } => "deplacer".into(),
                Commande::ViderDossiers { .. } => "vider".into(),
                Commande::CreerDossier { .. } => "creer".into(),
                Commande::Arborescence => "arborescence".into(),
                Commande::Reprendre(_) => "reprendre".into(),
                Commande::Veille => "veille".into(),
                Commande::Signale => "signale".into(),
                Commande::Precharger { chemin } => format!("precharger {chemin}"),
                Commande::Preparer { .. } => "preparer".into(),
                Commande::Envoyer { .. } => "envoyer".into(),
                Commande::ApresEnvoi { .. } => "apres-envoi".into(),
                Commande::Brouillon { .. } => "brouillon".into(),
            })
            .collect()
    }

    #[test]
    fn prechargement_derriere_ce_que_l_utilisateur_demande() {
        let (mut file, abandonnees) =
            regrouper(vec![Commande::Precharger { chemin: "INBOX".into() }, dossier("Sent"), corps(4)]);
        file.sort_by_key(Commande::rang);
        assert_eq!(noms(&file), vec!["ouvrir Sent", "corps 4", "precharger INBOX"]);
        assert_eq!(abandonnees, 0);
        assert!(!Commande::Precharger { chemin: "INBOX".into() }.comptee());
    }

    /// La lecture pour la fenêtre d'un message (numéro 0, 0.6.16) reste dans
    /// la file, quoi que l'on demande ensuite au volet.
    #[test]
    fn lecture_d_une_fenetre_jamais_abandonnee() {
        let fenetre = Commande::Corps { chemin: "Archives".into(), uid: 9, brut: false, distantes: false, numero: 0 };
        let (file, abandonnees) = regrouper(vec![fenetre, corps(1), dossier("Sent"), corps(2)]);
        assert_eq!(noms(&file), vec!["corps 9", "ouvrir Sent", "corps 2"]);
        assert_eq!(abandonnees, 1);
    }

    #[test]
    fn seul_le_dernier_dossier_demande_est_lu() {
        let (file, abandonnees) = regrouper(vec![dossier("INBOX"), dossier("Sent"), dossier("Archives")]);
        assert_eq!(noms(&file), vec!["ouvrir Archives"]);
        assert_eq!(abandonnees, 2);
    }

    #[test]
    fn un_message_delaisse_n_est_pas_telecharge() {
        let (file, _) = regrouper(vec![corps(1), corps(2), corps(3)]);
        assert_eq!(noms(&file), vec!["corps 3"]);
        // Changer de dossier rend caduque la lecture d'un message de l'ancien.
        let (file, _) = regrouper(vec![corps(4), dossier("Sent")]);
        assert_eq!(noms(&file), vec!["ouvrir Sent"]);
    }

    #[test]
    fn un_deplacement_n_est_jamais_abandonne() {
        let deplacer = Commande::Deplacer {
            source: "INBOX".into(),
            uids: vec![1],
            cible: Cible { compte: 1, chemin: "Archives".into(), identifiants: None },
        };
        let marquer = Commande::MarquerLu { chemin: "INBOX".into(), uids: vec![2], lu: true, discret: false };
        let (file, abandonnees) =
            regrouper(vec![dossier("INBOX"), deplacer, marquer, dossier("Sent"), corps(9)]);
        assert_eq!(noms(&file), vec!["deplacer", "marquer", "ouvrir Sent", "corps 9"]);
        assert_eq!(abandonnees, 1);
    }

    #[test]
    fn la_veille_cede_la_place_et_ne_compte_pas() {
        let (file, abandonnees) = regrouper(vec![Commande::Veille, dossier("INBOX")]);
        assert_eq!(noms(&file), vec!["ouvrir INBOX"]);
        assert_eq!(abandonnees, 0, "la veille n'a pas été comptée à l'envoi");
    }

    #[test]
    fn la_veille_n_est_pas_chassee_par_une_tache_de_fond() {
        // La minuterie des envois différés écrit à chaque compte toutes les
        // minutes : elle ne doit pas faire sauter la veille.
        let differes = Commande::EnvoyerDifferes {
            identifiants: Identifiants {
                hote: "h".into(),
                hote_envoi: String::new(),
                utilisateur: "u".into(),
                mot_de_passe: "p".into(),
                oauth: None,
            },
        };
        let (file, _) = regrouper(vec![Commande::Veille, differes]);
        assert_eq!(noms(&file), vec!["veille", "differes"]);
        let (file, _) = regrouper(vec![Commande::Veille, Commande::Veille]);
        assert_eq!(noms(&file), vec!["veille"]);
    }

    fn local(chemin: &str, role: &str) -> DossierLocal {
        let nom = chemin.rsplit('/').next().unwrap_or(chemin).to_string();
        DossierLocal { chemin: chemin.into(), nom, role: role.into(), selectionnable: true, ..Default::default() }
    }

    #[test]
    fn seuls_les_roles_corbeille_et_indesirables_sont_vides() {
        let dossiers = vec![
            local("INBOX", ""),
            local("Trash", "Trash"),
            local("Junk", "Junk"),
            local("Spambox", ""),
            local("Clients/Signalements spam", ""),
            local("Corbeille", ""),
            local("Sent", "Sent"),
        ];
        assert_eq!(dossiers_a_vider(&dossiers), vec!["Trash", "Junk"]);
    }

    #[test]
    fn sans_role_annonce_seuls_les_noms_exacts() {
        let mut non_selectionnable = local("Deleted Items", "");
        non_selectionnable.selectionnable = false;
        let dossiers = vec![
            local("INBOX", ""),
            local("INBOX/Trash", ""),
            local("Courrier indésirable", ""),
            local("Spambox", ""),
            local("Archives/spam 2025", ""),
            non_selectionnable,
        ];
        assert_eq!(dossiers_a_vider(&dossiers), vec!["INBOX/Trash", "Courrier indésirable"]);
    }

    #[test]
    fn differes_echus_sans_ceux_deja_reserves() {
        use crate::simule::{echange, session, DOVECOT};
        let fetch = |seq: u32, uid: u32, modseq: u64, drapeaux: &str, t: i64, poste: &str| {
            let mut e = format!("X-MMail-Envoi-Differe: {t}\r\nMessage-ID: <m{uid}@x>\r\n");
            if !poste.is_empty() {
                e.push_str(&format!("X-MMail-Poste-Differe: {poste}\r\n"));
            }
            e.push_str("\r\n");
            format!(
                "* {seq} FETCH (UID {uid} FLAGS ({drapeaux}) INTERNALDATE \"05-Oct-2026 10:00:00 +0200\" \
                 RFC822.SIZE 9 MODSEQ ({modseq}) BODY[HEADER.FIELDS ({CHAMPS_DIFFERE})] {{{}}}\r\n{e})\r\n",
                e.len()
            )
        };
        let reponses = [
            fetch(1, 11, 300, "", 1_000, ""),
            fetch(2, 12, 301, "\\Deleted", 1_000, ""),
            fetch(3, 13, 302, "", 9_000, ""),
            fetch(4, 14, 303, "\\Draft", 1_000, ""),
            fetch(5, 15, 304, "", 4_500, "autre"),
            fetch(6, 16, 305, "", 4_500, "moi"),
        ]
        .concat();
        let (mut client, journal) =
            session(DOVECOT, vec![echange("UID FETCH 1:* (UID FLAGS INTERNALDATE RFC822.SIZE MODSEQ", &reponses, "OK")]);
        let entetes = client.champs("1:*", CHAMPS_DIFFERE).unwrap();
        assert!(journal.lock().unwrap()[0].contains("X-MMAIL-POSTE-DIFFERE"));
        // 12 est retiré, 14 réservé par un autre MMail, 13 pas échu. Avec
        // CONDSTORE, la réservation départage les postes : 15 part d'ici aussi.
        assert_eq!(differes_echus(&entetes, 5_000, true, "moi"), vec![(11, 300), (15, 304), (16, 305)]);
        // Sans CONDSTORE, 15 est laissé à son poste…
        assert_eq!(differes_echus(&entetes, 5_000, false, "moi"), vec![(11, 300), (16, 305)]);
        // … tant qu'il a le temps de l'envoyer.
        assert_eq!(
            differes_echus(&entetes, 4_500 + RELAIS_AUTRE_POSTE, false, "moi"),
            vec![(11, 300), (15, 304), (16, 305)]
        );

        // 14 est réservé : interrompu s'il l'est encore 10 min plus tard.
        let mut vus = HashMap::new();
        assert!(differes_interrompus(&entetes, &mut vus, 5_000).is_empty());
        assert!(differes_interrompus(&entetes, &mut vus, 5_000 + RESERVATION_MAX - 1).is_empty());
        assert_eq!(differes_interrompus(&entetes, &mut vus, 5_000 + RESERVATION_MAX), vec![(14, "<m14@x>".to_string())]);
        // Une réservation levée sort du suivi.
        let sans: Vec<_> = entetes.iter().filter(|e| e.uid != 14).cloned().collect();
        assert!(differes_interrompus(&sans, &mut vus, 9_000).is_empty());
        assert!(vus.is_empty());
    }

    #[test]
    fn poste_d_origine_d_un_envoi_differe() {
        let a = poste_differe("/home/a/profil/index.sqlite");
        assert_eq!(a.len(), 16);
        assert_eq!(a, poste_differe("/home/a/profil/index.sqlite"));
        assert_ne!(a, poste_differe("D:\\MMail\\gmartin\\index.sqlite"), "deux profils d'un même poste diffèrent");
        assert_eq!(champ_differe(b"X-MMail-Poste-Differe: abc\r\nMessage-ID: <x@y>\r\n", "message-id").as_deref(), Some("<x@y>"));
    }

    fn ligne(uid: u32, h: i64, lu: bool) -> MessageLocal {
        MessageLocal { uid, horodatage: h, lu, sujet: format!("m{uid}"), ..Default::default() }
    }

    fn empreintes(liste: &[MessageLocal]) -> Vec<(u32, i64, u64)> {
        liste.iter().map(|m| (m.uid, m.horodatage, empreinte(m))).collect()
    }

    #[test]
    fn changements_de_la_liste() {
        let avant = vec![ligne(4, 40, false), ligne(3, 30, false), ligne(2, 20, true), ligne(1, 10, false)];
        let e_avant = empreintes(&avant);
        // Rien n'a changé : trois listes vides.
        assert_eq!(
            changements(&e_avant, &avant, &e_avant).unwrap(),
            r#"{"retires":[],"ajoutes":[],"modifies":[]}"#
        );
        // 5 arrive, 2 part, 3 est lu, 1 change de date (déplacé puis revenu).
        let apres = vec![ligne(5, 50, false), ligne(4, 40, false), ligne(3, 30, true), ligne(1, 15, false)];
        let json = changements(&e_avant, &apres, &empreintes(&apres)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let uids = |cle: &str| v[cle].as_array().unwrap().iter().map(|m| m["uid"].as_u64().unwrap()).collect::<Vec<_>>();
        assert_eq!(uids("retires"), vec![2, 1]);
        assert_eq!(uids("ajoutes"), vec![5, 1]);
        assert_eq!(uids("modifies"), vec![3]);
        // La date voyage avec chaque ligne : l'interface s'en sert pour placer.
        assert_eq!(v["retires"][1]["h"], 10);
        assert_eq!(v["ajoutes"][1]["h"], 15);
    }

    /// Le défaut de la 0.6.6 : 3 est marqué « lu » à l'affichage, par sa seule
    /// ligne ; marqué « non lu » ensuite, l'index redit ce que disait la liste
    /// envoyée, et la ligne restait « lu ». Oubliée, elle est renvoyée.
    #[test]
    fn ligne_marquee_a_l_affichage_renvoyee() {
        let avant = vec![ligne(4, 40, false), ligne(3, 30, false)];
        let mut liste = ListeEnvoyee { dossier: 1, empreintes: empreintes(&avant), fin: None, version: 7 };
        assert_eq!(
            changements(&liste.empreintes, &avant, &empreintes(&avant)).unwrap(),
            r#"{"retires":[],"ajoutes":[],"modifies":[]}"#
        );
        liste.oublier(&[3]);
        assert_eq!(liste.version, -1);
        let json = changements(&liste.empreintes, &avant, &empreintes(&avant)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let modifies: Vec<u64> = v["modifies"].as_array().unwrap().iter().map(|m| m["uid"].as_u64().unwrap()).collect();
        assert_eq!(modifies, vec![3]);
        assert_eq!(v["modifies"][0]["lu"], false);
        assert!(v["ajoutes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn marquage_de_l_utilisateur_apres_l_affichage() {
        let mut marquages = HashMap::new();
        // Jamais marqué : l'affichage marque « lu ».
        assert!(!marque_depuis(&marquages, 1, 3, 5));
        // Marqué pendant l'affichage n° 5 (demande 5, puis Ctrl+U) : celui-ci
        // ne marque plus.
        marquages.insert((1, 3), 5);
        assert!(marque_depuis(&marquages, 1, 3, 5));
        // Rouvert ensuite (affichage 6) : il se marque de nouveau « lu ».
        assert!(!marque_depuis(&marquages, 1, 3, 6));
        // Un autre message, ou le même UID dans un autre dossier : sans effet.
        assert!(!marque_depuis(&marquages, 1, 4, 5));
        assert!(!marque_depuis(&marquages, 2, 3, 5));
    }

    #[test]
    fn trop_de_changements_fait_tout_relire() {
        let avant: Vec<MessageLocal> = (1..=10).map(|u| ligne(u, u as i64, false)).collect();
        let apres: Vec<MessageLocal> = (1..=CHANGEMENTS_MAX as u32 + 20).map(|u| ligne(u, u as i64, true)).collect();
        assert!(changements(&empreintes(&avant), &apres, &empreintes(&apres)).is_none());
    }

    #[test]
    fn signaux_d_idle() {
        for ligne in ["* 12 EXISTS", "* 3 EXPUNGE", "* VANISHED 7:9", "* 2 FETCH (UID 8 FLAGS (\\Seen) MODSEQ (4))", "* 5 exists"] {
            assert!(signale_un_changement(ligne), "{ligne}");
        }
        for ligne in ["* OK Still here", "* 1 RECENT", "+ idling", "m0004 OK Idle completed"] {
            assert!(!signale_un_changement(ligne), "{ligne}");
        }
    }

    #[test]
    fn file_vide() {
        assert!(regrouper(Vec::new()).0.is_empty());
    }

    #[test]
    fn uids_lus_depuis_l_interface() {
        assert_eq!(lire_uids(&QString::from("3, 5,x,0,7")), vec![3, 5, 7]);
        assert!(lire_uids(&QString::from("")).is_empty());
    }

    #[test]
    fn atelier_a_cote_de_l_index() {
        assert_eq!(atelier("/profil/index.sqlite"), PathBuf::from("/profil/operations"));
    }

    #[test]
    fn echappement_json() {
        assert_eq!(texte_json("simple"), "\"simple\"");
        assert_eq!(texte_json(r#"gui"llemet"#), r#""gui\"llemet""#);
        assert_eq!(texte_json("deux\nlignes"), r#""deux\nlignes""#);
        assert_eq!(texte_json("tab\t"), r#""tab\t""#);
        // Un caractère de contrôle casserait l'analyse côté QML : il doit sortir
        // sous sa forme échappée, et non tel quel.
        assert_eq!(texte_json("a\u{1}b"), "\"a\\u0001b\"");
        // Les accents restent tels quels : le JSON est de l'UTF-8.
        assert_eq!(texte_json("été"), "\"été\"");
    }

    #[test]
    fn dossier_serialise() {
        let json = json_dossier(
            "dossier",
            &DossierLocal {
                id: 4,
                compte_id: 2,
                chemin: "Essais/Factures".into(),
                nom: "Factures".into(),
                separateur: "/".into(),
                profondeur: 1,
                selectionnable: true,
                messages: 5,
                non_lus: 2,
                favori: Some(1),
                ..Default::default()
            },
            "a@b.fr",
            1,
            true,
        );
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains(r#""enfants":true"#));
        assert!(json.contains(r#""nom":"Factures""#));
        assert!(json.contains(r#""nonLus":2"#));
        assert!(json.contains(r#""compte":2"#));
        assert!(json.contains(r#""favori":true"#));
    }

    #[test]
    fn pieces_serialisees() {
        let json = json_pieces(&[crate::index::PieceJointe {
            indice: 1,
            nom: "outil \"x\".exe".into(),
            type_mime: "application/octet-stream".into(),
            taille: 42,
            risquee: true,
        }]);
        assert!(json.contains(r#""indice":1"#));
        assert!(json.contains(r#""risquee":true"#));
        assert!(json.contains(r#"outil \"x\".exe"#));
    }

    #[test]
    fn dossier_des_pieces_a_cote_de_l_index() {
        assert_eq!(dossier_pieces("/profil/index.sqlite"), PathBuf::from("/profil/pieces-jointes"));
    }

    #[test]
    fn messages_serialises() {
        let json = json_messages(&[MessageLocal {
            uid: 3,
            expediteur: "Service \"compta\"".into(),
            destinataires: "Dupont, Jean; Marie".into(),
            sujet: "Réunion d'équipe".into(),
            lu: true,
            ..Default::default()
        }]);
        assert!(json.contains(r#""uid":3"#));
        assert!(json.contains(r#""destinataires":"Dupont, Jean; Marie""#));
        assert!(json.contains(r#""lu":true"#));
        assert!(json.contains(r#""pieces":false"#));
        assert!(json.contains("Réunion"));
        assert!(json.contains(r#"Service \"compta\""#));
    }

    #[test]
    fn prechargement_regroupe_et_non_compte() {
        let pre = |c: &str| Commande::Precharger { chemin: c.into() };
        // Deux préchargements du même dossier n'en font qu'un ; deux dossiers
        // différents restent.
        let (file, abandonnees) = regrouper(vec![pre("INBOX"), corps(1), pre("INBOX"), pre("Sent")]);
        assert_eq!(noms(&file), vec!["corps 1", "precharger INBOX", "precharger Sent"]);
        assert_eq!(abandonnees, 0);
        assert!(!pre("INBOX").comptee());
    }

    #[test]
    fn sous_dossiers_replies() {
        let d = |chemin: &str, replie: bool| DossierLocal {
            chemin: chemin.into(),
            nom: chemin.rsplit('/').next().unwrap_or(chemin).into(),
            separateur: "/".into(),
            replie,
            ..Default::default()
        };
        let liste = [d("INBOX", false), d("Clients", true), d("Clients/A", false), d("Clients/A/x", false),
                     d("ClientsB", false), d("Archives", false), d("Archives/2025", false)];
        let vus: Vec<(&str, bool)> = arbre_visible(&liste).into_iter().map(|(d, e)| (d.chemin.as_str(), e)).collect();
        assert_eq!(vus, vec![("INBOX", false), ("Clients", true), ("ClientsB", false), ("Archives", true), ("Archives/2025", false)]);
    }

    #[test]
    fn masquer_un_dossier_masque_sa_descendance() {
        let d = |chemin: &str, masque: bool| DossierLocal {
            chemin: chemin.into(),
            separateur: "/".into(),
            masque,
            ..Default::default()
        };
        let liste = vec![d("INBOX", false), d("Archives", true), d("Archives/2025", false),
                         d("Archives/2025/Mars", false), d("ArchivesB", false)];
        let restants: Vec<String> = sans_masques(liste).into_iter().map(|d| d.chemin).collect();
        assert_eq!(restants, vec!["INBOX", "ArchivesB"]);
    }

    #[test]
    fn sous_dossiers_sous_leur_parent() {
        // L'ordre de l'index : boîte de réception, rôles, puis l'alphabet
        // binaire — qui plaçait les sous-dossiers de la boîte de réception
        // après « Eléments infectés ».
        let d = |chemin: &str, replie: bool| DossierLocal {
            chemin: chemin.into(),
            nom: chemin.rsplit('/').next().unwrap_or(chemin).into(),
            separateur: "/".into(),
            replie,
            ..Default::default()
        };
        let liste = [d("INBOX", false), d("Sent", false), d("Eléments infectés", false), d("INBOX/Banque", false),
                     d("INBOX/FRP2I", false), d("INBOX/clients", false), d("INBOX/fournisseurs", false),
                     d("INBOX/fournisseurs/3CX", false), d("INBOX/fournisseurs/Althus", false), d("Spambox", false)];
        let ordre = |l: &[DossierLocal]| -> Vec<String> { arbre_visible(l).into_iter().map(|(d, _)| d.chemin.clone()).collect() };
        assert_eq!(
            ordre(&liste),
            vec!["INBOX", "INBOX/Banque", "INBOX/clients", "INBOX/fournisseurs", "INBOX/fournisseurs/3CX",
                 "INBOX/fournisseurs/Althus", "INBOX/FRP2I", "Sent", "Eléments infectés", "Spambox"]
        );
        // Replier « fournisseurs » cache ses sous-dossiers, rien d'autre.
        let mut repliee = liste.clone();
        repliee[6].replie = true;
        assert_eq!(
            ordre(&repliee),
            vec!["INBOX", "INBOX/Banque", "INBOX/clients", "INBOX/fournisseurs", "INBOX/FRP2I", "Sent", "Eléments infectés", "Spambox"]
        );
        // Replier la boîte de réception cache tout ce qu'elle contient.
        let mut inbox = liste.clone();
        inbox[0].replie = true;
        assert_eq!(ordre(&inbox), vec!["INBOX", "Sent", "Eléments infectés", "Spambox"]);
    }

    #[test]
    fn dossiers_de_role_par_le_nom_sans_special_use() {
        // Arborescence d'une boîte d'essai chez OVH, relevée le
        // 08/10 : aucun attribut de rôle, des doublons laissés par d'autres
        // clients sous « INBOX.INBOX ».
        let d = |chemin: &str| DossierLocal {
            chemin: chemin.into(),
            nom: chemin.rsplit('.').next().unwrap_or(chemin).into(),
            profondeur: chemin.matches('.').count() as u32,
            selectionnable: true,
            ..Default::default()
        };
        let ovh = [
            d("INBOX"),
            d("INBOX.Éléments supprimés"),
            d("INBOX.Éléments envoyés"),
            d("INBOX.INBOX.Junk"),
            d("INBOX.INBOX.Trash"),
            d("INBOX.INBOX.Drafts"),
            d("INBOX.INBOX.Sent"),
            d("INBOX.Courrier indésirable"),
            d("INBOX.Brouillons"),
            d("INBOX.Trash"),
            d("INBOX.Spambox"),
            d("INBOX.Sent"),
        ];
        assert_eq!(dossier_de_role(&ovh, "Drafts").as_deref(), Some("INBOX.Brouillons"));
        assert_eq!(dossier_de_role(&ovh, "Sent").as_deref(), Some("INBOX.Sent"));
        assert_eq!(dossier_de_role(&ovh, "Trash").as_deref(), Some("INBOX.Trash"));
        assert_eq!(dossier_de_role(&ovh, "Archive"), None);
        // Seuls les noms traduits : ils suffisent.
        let traduits = [d("INBOX"), d("INBOX.Éléments envoyés"), d("INBOX.Corbeille")];
        assert_eq!(dossier_de_role(&traduits, "Sent").as_deref(), Some("INBOX.Éléments envoyés"));
        assert_eq!(dossier_de_role(&traduits, "Trash").as_deref(), Some("INBOX.Corbeille"));
        assert_eq!(dossier_de_role(&traduits, "Drafts"), None);

        // L'attribut prime sur le nom ; un dossier qui porte un autre rôle ou
        // ne s'ouvre pas n'est jamais retenu pour son nom.
        let mut noeud = d("Drafts");
        noeud.selectionnable = false;
        let mut archive = d("Sent");
        archive.role = "Archive".into();
        let mut envoyes = d("Envois");
        envoyes.role = "Sent".into();
        let mailcow = [d("INBOX"), noeud, archive, envoyes];
        assert_eq!(dossier_de_role(&mailcow, "Sent").as_deref(), Some("Envois"));
        assert_eq!(dossier_de_role(&mailcow, "Drafts"), None);

        // Et la purge reste aux seuls rôles annoncés, ou aux noms exacts
        // quand aucun ne l'est : la reconnaissance par le nom n'y change rien.
        assert_eq!(dossiers_a_vider(&ovh), vec!["INBOX.Éléments supprimés", "INBOX.INBOX.Junk", "INBOX.INBOX.Trash", "INBOX.Courrier indésirable", "INBOX.Trash"]);
    }

    #[test]
    fn image_glissee_copiee_puis_integree_a_l_envoi() {
        let racine = std::env::temp_dir().join(format!("mmail-essai-images-{}", serie_signature()));
        std::fs::create_dir_all(&racine).unwrap();
        let profil = racine.join("index.sqlite").to_string_lossy().into_owned();
        let source = racine.join("capture d'écran.png");
        std::fs::write(&source, b"\x89PNG\r\n\x1a\nessai").unwrap();
        let texte = racine.join("note.txt");
        std::fs::write(&texte, b"pas une image").unwrap();

        let url = importer_image(&profil, &crate::rendu::url_fichier(&source)).expect("image copiée");
        let copie = crate::rendu::chemin_de_url(&url).unwrap();
        assert!(copie.starts_with(dossier_images_collees(&profil)), "{copie:?}");
        // L'envoi l'intègre : elle est dans un dossier autorisé.
        let (type_mime, octets) = image_autorisee(&profil, &url).expect("image autorisée");
        assert_eq!((type_mime.as_str(), octets.len()), ("image/png", 13));
        // L'original, hors du profil, ne partirait pas.
        assert!(image_autorisee(&profil, &crate::rendu::url_fichier(&source)).is_none());
        assert!(importer_image(&profil, &crate::rendu::url_fichier(&texte)).is_none());
        assert!(importer_image(&profil, "https://exemple.fr/image.png").is_none());
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn drapeaux_poses_sur_l_origine() {
        assert_eq!(drapeaux_d_origine("repondre"), ["\\Answered"]);
        assert_eq!(drapeaux_d_origine("repondre_tous"), ["\\Answered", "$ReplyAll"]);
        assert_eq!(drapeaux_d_origine("transferer"), ["$Forwarded"]);
    }

    #[test]
    fn chemin_d_un_resultat_de_recherche() {
        assert_eq!(segments_du_chemin("INBOX/fournisseurs/3CX", "/"), vec!["INBOX", "fournisseurs", "3CX"]);
        assert_eq!(segments_du_chemin("INBOX.&AMk-l&AOk-ments envoy&AOk-s", "."), vec!["INBOX", "Éléments envoyés"]);
        assert_eq!(segments_du_chemin("Archives", ""), vec!["Archives"]);
        let t = crate::magasin::Trouve {
            message: MessageLocal { uid: 7, sujet: "Devis".into(), ..Default::default() },
            compte: 2,
            chemin: "INBOX/Clients".into(),
            nom_dossier: "Clients".into(),
            separateur: "/".into(),
            role: String::new(),
        };
        let json = json_trouves(&[t]);
        assert!(json.contains(r#""segments":["INBOX","Clients"],"role":""}]"#), "{json}");
    }

    #[test]
    fn dossier_des_indesirables() {
        let d = |chemin: &str, role: &str| DossierLocal {
            chemin: chemin.into(),
            nom: chemin.rsplit('/').next().unwrap_or(chemin).into(),
            role: role.into(),
            selectionnable: true,
            ..Default::default()
        };
        // L'attribut prime ; puis « Junk », sur lequel Mailcow apprend ; puis un nom usuel.
        assert_eq!(dossier_indesirables(&[d("Spambox", ""), d("Junk", "Junk")]).as_deref(), Some("Junk"));
        assert_eq!(dossier_indesirables(&[d("Spambox", ""), d("Junk", "")]).as_deref(), Some("Junk"));
        assert_eq!(dossier_indesirables(&[d("INBOX", ""), d("Courrier indésirable", "")]).as_deref(), Some("Courrier indésirable"));
        assert_eq!(dossier_indesirables(&[d("INBOX", ""), d("Trash", "Trash")]), None);
    }

    #[test]
    fn adresse_de_fichier() {
        use crate::rendu::{chemin_de_url, url_fichier};
        assert_eq!(url_fichier(Path::new("/home/a b/é.png")), "file:///home/a%20b/%C3%A9.png");
        assert_eq!(url_fichier(Path::new("C:\\Users\\x#1\\i0.png")), "file:///C:/Users/x%231/i0.png");
        assert_eq!(chemin_de_url("file:///home/a%20b/%C3%A9.png"), Some(PathBuf::from("/home/a b/é.png")));
        assert_eq!(chemin_de_url("file:///C:/Users/x%231/i0.png"), Some(PathBuf::from("C:/Users/x#1/i0.png")));
        assert_eq!(chemin_de_url("file://serveur/partage/x.png"), None);
        assert_eq!(chemin_de_url("https://exemple.fr/x.png"), None);
    }

    /// Un fil de compte sur le faux serveur, ses issues remises à un canal :
    /// index en mémoire, une boîte de réception, profil jetable.
    fn fil_d_essai(client: Client, nom: &str) -> (Travail, Etabli, Receiver<Issue>) {
        let magasin = Magasin::en_memoire().unwrap();
        let compte = magasin.compte("essai@exemple.fr", "h", 993, "essai@exemple.fr").unwrap();
        let inbox = crate::protocole::Dossier { chemin: "INBOX".into(), separateur: "/".into(), attributs: vec![] };
        magasin.poser_dossiers(compte, &[inbox]).unwrap();
        let racine = std::env::temp_dir().join(format!("mmail-fil-{}-{nom}", std::process::id()));
        let (envoi, issues) = mpsc::channel();
        let travail = Travail {
            fil: Destinataire::Essai(envoi),
            compte,
            generation: 1,
            attente: Arc::new(AtomicUsize::new(0)),
            atelier: racine.join("atelier"),
            profil: racine.join("index.sqlite").to_string_lossy().into_owned(),
        };
        let etabli = Etabli {
            client,
            magasin,
            annexes: HashMap::new(),
            identites: HashMap::new(),
            ouvert: None,
            menage: None,
            dernier: None,
            differes_reserves: HashMap::new(),
        };
        (travail, etabli, issues)
    }

    /// Comme `lancer` : la connexion, et les seules commandes comptées.
    fn attente_initiale(travail: &Travail, commandes: &[Commande]) -> Arc<AtomicUsize> {
        travail.attente.store(1 + commandes.iter().filter(|c| c.comptee()).count(), Ordering::SeqCst);
        travail.attente.clone()
    }

    /// Le fil d'un compte (revue du 19/10, n° 9 et 13) : la boîte de réception
    /// relue à la connexion, une commande de l'interface traitée, et le canal
    /// fermé par l'interface qui termine la session — `attente`, qui fait
    /// tourner l'indicateur d'activité, revenu à zéro.
    #[test]
    fn fil_de_travail_jusqu_a_la_fermeture_du_canal() {
        use crate::simule::{echange, session};
        let (client, journal) = session(
            "UIDPLUS",
            vec![
                echange(
                    "SELECT \"INBOX\"",
                    "* 0 EXISTS\r\n* OK [UIDVALIDITY 7] v\r\n* OK [UIDNEXT 1] n\r\n",
                    "OK [READ-WRITE] fait",
                ),
                echange("LIST \"\" \"*\"", "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n", "OK fait"),
                echange("STATUS \"INBOX\" (MESSAGES UNSEEN)", "* STATUS \"INBOX\" (MESSAGES 0 UNSEEN 0)\r\n", "OK fait"),
                echange("LOGOUT", "* BYE\r\n", "OK fait"),
            ],
        );
        let (travail, etabli, issues) = fil_d_essai(client, "fermeture");
        let commandes = vec![Commande::Arborescence];
        let attente = attente_initiale(&travail, &commandes);
        let (envoi, reception) = mpsc::channel();
        for commande in commandes {
            envoi.send(commande).unwrap();
        }
        let fil = thread::spawn(move || travail.travailler(etabli, reception));
        let mut recues = Vec::new();
        while recues.len() < 3 {
            recues.push(issues.recv_timeout(Duration::from_secs(10)).expect("issue du fil"));
        }
        // La connexion ; la boîte de réception surveillée relue la première
        // fois, d'où l'arborescence ; puis la commande de l'interface.
        assert!(matches!(recues.as_slice(), [Issue::Connecte, Issue::Arborescence, Issue::Arborescence]));
        drop(envoi);
        fil.join().expect("le fil finit sans panique");
        assert_eq!(attente.load(Ordering::SeqCst), 0);
        assert_eq!(
            journal.lock().unwrap().as_slice(),
            ["SELECT \"INBOX\"", "LIST \"\" \"*\"", "STATUS \"INBOX\" (MESSAGES UNSEEN)", "LOGOUT"]
        );
    }

    /// Session perdue en pleine commande : le fil s'arrête, les commandes qui
    /// attendaient rendent leur issue avant l'échec, et `attente` revient à
    /// zéro — une commande non comptée (préchargement) n'y laisse rien.
    #[test]
    fn fil_de_travail_coupe_en_pleine_commande() {
        use crate::simule::{coupure, session};
        let (client, journal) = session("UIDPLUS", vec![coupure("SELECT \"INBOX\"", "* 3 EXISTS\r\n")]);
        let (travail, etabli, issues) = fil_d_essai(client, "coupure");
        let commandes = vec![
            Commande::Arborescence,
            Commande::MarquerSuivi { chemin: "INBOX".into(), uids: vec![1], suivi: true },
            Commande::Precharger { chemin: "Archives".into() },
        ];
        let attente = attente_initiale(&travail, &commandes);
        assert_eq!(attente.load(Ordering::SeqCst), 3);
        let (envoi, reception) = mpsc::channel();
        for commande in commandes {
            envoi.send(commande).unwrap();
        }
        // Le fil rend la main de lui-même : la session est perdue.
        travail.travailler(etabli, reception);
        assert_eq!(attente.load(Ordering::SeqCst), 0);
        let recues: Vec<Issue> = issues.try_iter().collect();
        assert!(matches!(recues.first(), Some(Issue::Connecte)));
        assert!(matches!(recues.last(), Some(Issue::Echec { session_perdue: true, .. })));
        // Ni LOGOUT sur une connexion morte, ni commande après la coupure.
        assert_eq!(journal.lock().unwrap().as_slice(), ["SELECT \"INBOX\""]);
        drop(envoi);
    }

    // ---------------------------------------- recherche sur le serveur (0.6.16)

    /// Recherche de « devis », numéro 1, en cours.
    fn recherche(chemins: &[&str], partout: bool, patience_ms: u64, delai_ms: u64) -> Recherche {
        Recherche {
            compte: 1,
            texte: "devis".into(),
            chemins: chemins.iter().map(|c| c.to_string()).collect(),
            partout,
            numero: 1,
            courante: Arc::new(AtomicU64::new(1)),
            patience: Duration::from_millis(patience_ms),
            delai: Duration::from_millis(delai_ms),
        }
    }

    fn examen(chemin: &str) -> crate::simule::Echange {
        crate::simule::echange(
            &format!("EXAMINE \"{chemin}\""),
            "* 3 EXISTS\r\n* OK [UIDVALIDITY 7] UIDs valid\r\n",
            "OK [READ-ONLY] fait",
        )
    }

    /// Issues d'une recherche contre des sessions simulées, ouvertes l'une
    /// après l'autre, et le nombre de sessions ouvertes.
    fn issues_de(r: &Recherche, sessions: Vec<Client>) -> (Vec<String>, usize) {
        let magasin = Magasin::en_memoire().unwrap();
        let mut sessions = std::collections::VecDeque::from(sessions);
        let mut ouvertes = 0;
        let mut issues = Vec::new();
        let mut ouvrir = || {
            ouvertes += 1;
            sessions.pop_front().ok_or_else(|| Erreur::Reseau("serveur injoignable".into()))
        };
        r.executer(&mut ouvrir, &magasin, &mut |issue| {
            issues.push(match issue {
                Issue::RecherchePartout { chemin, uids, restants, inacheve, .. } => {
                    format!("{chemin} {uids:?} reste {restants} : {}", inacheve.as_deref().unwrap_or("fini"))
                }
                Issue::Recherche { chemin, uids, inacheve, .. } => {
                    format!("{chemin} {uids:?} : {}", inacheve.as_deref().unwrap_or("fini"))
                }
                _ => "autre issue".into(),
            })
        });
        (issues, ouvertes)
    }

    /// Un dossier qui tarde passe après les autres, par une nouvelle session :
    /// celle qui attendait est au milieu d'une commande. Aucune issue ne
    /// touche au compte (ni `Echec`, ni `session_perdue`).
    #[test]
    fn dossier_lent_apres_les_autres() {
        use crate::simule::{echange, lente, session, DOVECOT};
        let (s1, j1) = session(DOVECOT, vec![examen("INBOX"), lente("UID SEARCH", &[(usize::MAX, "* SEARCH 2\r\n")], "OK")]);
        let (s2, j2) = session(
            DOVECOT,
            vec![
                examen("Archives"),
                echange("UID SEARCH", "* SEARCH\r\n", "OK"),
                examen("INBOX"),
                lente("UID SEARCH", &[(20, "* SEARCH 4\r\n")], "OK"),
                echange("LOGOUT", "* BYE\r\n", "OK"),
            ],
        );
        let (issues, ouvertes) = issues_de(&recherche(&["INBOX", "Archives"], true, 30, 5_000), vec![s1, s2]);
        assert_eq!(issues, ["Archives [] reste 1 : fini", "INBOX [4] reste 0 : fini"]);
        assert_eq!(ouvertes, 2);
        assert_eq!(j1.lock().unwrap().len(), 2, "pas de LOGOUT au milieu d'une commande");
        assert_eq!(j2.lock().unwrap().last().unwrap(), "LOGOUT");
    }

    /// Un dossier seul a toute l'attente ; sans réponse, ou si Dovecot a
    /// renoncé à attendre son indexeur, le motif est rendu avec le dossier.
    #[test]
    fn dossier_sans_reponse_rendu_avec_son_motif() {
        use crate::simule::{echange, lente, session, DOVECOT};
        let (s, _) = session(DOVECOT, vec![examen("INBOX"), lente("UID SEARCH", &[(usize::MAX, "* SEARCH 2\r\n")], "OK")]);
        let (issues, _) = issues_de(&recherche(&["INBOX"], false, 30, 40), vec![s]);
        assert_eq!(issues, ["INBOX [] : pas de réponse du serveur en 0 s"]);
        let (s, _) = session(
            DOVECOT,
            vec![
                examen("INBOX"),
                echange("UID SEARCH", "", "NO [INUSE] Timeout while waiting for indexing to finish"),
                echange("LOGOUT", "* BYE\r\n", "OK"),
            ],
        );
        let (issues, _) = issues_de(&recherche(&["INBOX"], false, 30, 5_000), vec![s]);
        assert_eq!(issues, ["INBOX [] : le serveur prépare encore l'index de recherche de ce dossier"]);
        // Serveur injoignable : le dossier le dit, rien de plus.
        let (issues, ouvertes) = issues_de(&recherche(&["INBOX"], false, 30, 5_000), vec![]);
        assert_eq!(issues, ["INBOX [] : connexion impossible (réseau : serveur injoignable)"]);
        assert_eq!(ouvertes, 1);
    }

    /// Une recherche remplacée ne rend plus rien, et n'ouvre rien.
    #[test]
    fn recherche_remplacee_muette() {
        let r = recherche(&["INBOX", "Archives"], true, 30, 5_000);
        r.courante.store(2, Ordering::SeqCst);
        let (issues, ouvertes) = issues_de(&r, vec![]);
        assert!(issues.is_empty());
        assert_eq!(ouvertes, 0);
    }

    #[test]
    fn durees_lisibles() {
        assert_eq!(duree_lisible(Duration::from_secs(45)), "45 s");
        assert_eq!(duree_lisible(Duration::from_secs(300)), "5 min");
        assert_eq!(duree_lisible(Duration::from_secs(330)), "5 min 30 s");
    }
}
