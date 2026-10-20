// SPDX-License-Identifier: GPL-3.0-or-later
//! Index local : une base SQLite par profil (décision 14 du dossier de projet).
//!
//! Ce que la base porte : les comptes, l'arborescence des dossiers avec leur
//! état de synchronisation et leur état purement local (masquage, favoris),
//! l'index des messages (en-têtes décodés), et la file des déplacements entre
//! boîtes en cours (`pending_ops`, décision 16). Les corps n'y sont pas : ceux
//! de la fenêtre gardée sur le poste (décision 17) sont des fichiers à part
//! (`cache`), l'index ne notant que leur état ; les autres sont lus au besoin
//! sur le serveur.
//!
//! Le schéma reprend les noms décidés au dossier pour que la suite s'y pose
//! sans renommer : `accounts`, `folders`, `messages`, `pending_ops`.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use crate::protocole::{self, Dossier, EtatDossier, Statut};

pub type Resultat<T> = Result<T, Erreur>;

/// Un contact des carnets d'adresses, pour l'interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FicheLocale {
    pub nom: String,
    pub organisation: String,
    pub courriels: Vec<String>,
    pub telephones: Vec<String>,
    pub carnet: String,
    pub compte: String,
}

/// Lignes écrites par transaction : le verrou d'écriture de SQLite est unique,
/// et le fil de l'interface, qui écrit aussi (marquages), attend qu'il se
/// libère. Une transaction sur tout un dossier le tenait plusieurs secondes.
const LOT_ECRITURE: usize = 500;

/// Mode d'authentification d'un compte Microsoft (`accounts.auth`).
pub const AUTH_MICROSOFT: &str = "microsoft";

/// Mode d'authentification d'un compte Google (`accounts.auth`).
pub const AUTH_GOOGLE: &str = "google";

/// Version du schéma, portée par `PRAGMA user_version`.
const VERSION_SCHEMA: i32 = 17;

#[derive(Debug)]
pub struct Erreur(pub String);

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<rusqlite::Error> for Erreur {
    fn from(e: rusqlite::Error) -> Self {
        Erreur(e.to_string())
    }
}

/// Un agenda d'un compte, tel que le serveur l'a décrit, et la case qui dit
/// s'il est affiché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaLocal {
    pub id: i64,
    pub compte: i64,
    /// Adresse complète de la collection CalDAV.
    pub adresse: String,
    pub nom: String,
    /// `#RRGGBB`, ou vide.
    pub couleur: String,
    /// Étiquette du serveur à la dernière synchronisation complète.
    pub ctag: String,
    pub affiche: bool,
    /// Le serveur y accepte l'écriture (`current-user-privilege-set`).
    pub ecriture: bool,
}

/// Un objet d'un agenda tel que l'index le garde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjetLocal {
    pub id: i64,
    pub agenda: i64,
    pub href: String,
    pub etag: String,
    pub ical: String,
}

/// Un objet d'un agenda à ranger : adresse, ETag, texte iCalendar et période
/// occupée (cf. `agenda::etendue`).
pub struct ObjetAgenda {
    pub href: String,
    pub etag: String,
    pub ical: String,
    pub debut: i64,
    pub fin: i64,
}

/// Un compte tel qu'il est enregistré dans le profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompteLocal {
    pub id: i64,
    pub adresse: String,
    pub hote: String,
    pub port: u16,
    pub utilisateur: String,
    /// Serveur de soumission SMTP, quand il n'est pas celui de la lecture
    /// (Orange : `imap.orange.fr` et `smtp.orange.fr`). Vide : le même.
    pub hote_envoi: String,
    /// Replié dans l'arborescence : ses dossiers ne s'affichent pas.
    pub replie: bool,
    /// Mode d'authentification : vide pour un mot de passe, `microsoft` pour
    /// un compte Microsoft, `google` pour un compte Google (OAuth2, cf.
    /// `crate::oauth`).
    pub auth: String,
    /// Adresses des boîtes partagées que le serveur montre à ce compte
    /// (0.6.8, cf. `protocole::adresses_partagees`), relevées avec son
    /// arborescence.
    pub partagees: Vec<String>,
}

impl CompteLocal {
    /// Compte Microsoft : jeton OAuth2 au lieu d'un mot de passe, envoi par
    /// Graph.
    pub fn microsoft(&self) -> bool {
        self.auth == AUTH_MICROSOFT
    }

    /// Compte à jetons OAuth2 (Microsoft, Google) : pas de mot de passe.
    pub fn oauth(&self) -> bool {
        self.fournisseur().is_some()
    }

    /// Service d'un compte à jetons ; `None` pour un mot de passe.
    pub fn fournisseur(&self) -> Option<crate::oauth::Fournisseur> {
        crate::oauth::Fournisseur::de_auth(&self.auth)
    }

    /// Serveur auquel remettre les messages : celui de l'envoi s'il est
    /// donné, sinon celui de la lecture.
    pub fn serveur_envoi(&self) -> &str {
        if self.hote_envoi.is_empty() { &self.hote } else { &self.hote_envoi }
    }
}

/// Un dossier tel qu'il est rangé dans l'index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DossierLocal {
    pub id: i64,
    pub compte_id: i64,
    pub chemin: String,
    pub nom: String,
    pub separateur: String,
    pub profondeur: u32,
    pub role: String,
    pub selectionnable: bool,
    /// Compteurs : ceux du serveur (`STATUS`), puis ceux de l'index une fois
    /// le dossier synchronisé.
    pub messages: u32,
    pub non_lus: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_mod_seq: u64,
    /// Masqué localement (décision 4) : il existe toujours sur le serveur.
    pub masque: bool,
    /// Rang dans la rubrique Favoris (décision 3), s'il y figure.
    pub favori: Option<i64>,
    /// Sous-dossiers repliés dans l'arborescence : réglage local, comme le
    /// repli d'un compte.
    pub replie: bool,
}

/// Un message tel qu'il est rangé dans l'index : de quoi remplir une liste.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MessageLocal {
    pub uid: u32,
    pub message_id: String,
    pub expediteur: String,
    pub adresse: String,
    /// Destinataires (`To`, à défaut `Cc`), affichés à la place de
    /// l'expéditeur dans les éléments envoyés (cf. `index::destinataires`).
    pub destinataires: String,
    pub sujet: String,
    pub date: String,
    /// Date interne du serveur en secondes Unix : la clef de tri.
    pub horodatage: i64,
    pub taille: u32,
    pub lu: bool,
    pub repondu: bool,
    /// Transféré (`$Forwarded`), par MMail ou par un autre client.
    pub transfere: bool,
    /// Répondu à tous depuis MMail (`$ReplyAll`, propre à MMail).
    pub repondu_tous: bool,
    /// Pièces jointes : supposées d'après les en-têtes, puis constatées à la
    /// lecture du message.
    pub pieces: bool,
    /// Drapeau de suivi (`\Flagged`).
    pub suivi: bool,
    /// Importance annoncée : 1 haute, 0 normale, -1 basse.
    pub importance: i8,
}

/// Un message trouvé par une recherche, avec son dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trouve {
    pub message: MessageLocal,
    pub compte: i64,
    pub chemin: String,
    pub nom_dossier: String,
    /// Séparateur de hiérarchie du compte et rôle SPECIAL-USE du dossier : de
    /// quoi afficher son chemin complet, comme dans l'arborescence.
    pub separateur: String,
    pub role: String,
}

/// Étape d'un déplacement entre boîtes (décision 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Etape {
    /// Le message est lu et conservé en `.eml` ; il reste à le déposer dans la
    /// cible.
    Deposer,
    /// Le message est dans la cible ; il reste à le retirer de la source.
    Purger,
}

impl Etape {
    fn code(self) -> &'static str {
        match self {
            Etape::Deposer => "deposer",
            Etape::Purger => "purger",
        }
    }

    fn lire(code: &str) -> Etape {
        match code {
            "purger" => Etape::Purger,
            _ => Etape::Deposer,
        }
    }
}

/// Un déplacement entre boîtes en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: i64,
    pub compte_source: i64,
    pub chemin_source: String,
    pub validite_source: u32,
    pub uid_source: u32,
    pub compte_cible: i64,
    pub chemin_cible: String,
    pub message_id: String,
    /// Drapeaux et date interne d'origine, à reposer dans la cible.
    pub drapeaux: String,
    pub date_interne: String,
    /// Copie du message, le temps du déplacement : c'est elle qui garantit
    /// qu'aucune coupure ne le perd.
    pub fichier: String,
    pub etape: Etape,
    pub erreur: String,
}

pub struct Magasin {
    base: Connection,
}

impl Magasin {
    /// Ouvre — ou crée — l'index d'un profil.
    pub fn ouvrir(chemin: &str) -> Resultat<Magasin> {
        let base = Connection::open(chemin)?;
        Magasin::preparer(base)
    }

    /// Index en mémoire : sert aux tests, et à rien d'autre.
    pub fn en_memoire() -> Resultat<Magasin> {
        Magasin::preparer(Connection::open_in_memory()?)
    }

    fn preparer(base: Connection) -> Resultat<Magasin> {
        // WAL : plusieurs lectures pendant une écriture. Le profil est
        // obligatoirement sur un disque local (décision 18) — SQLite en WAL se
        // comporte mal sur un partage réseau.
        base.pragma_update(None, "journal_mode", "WAL")?;
        // En WAL, NORMAL ne met pas l'index en danger : une coupure de courant
        // peut perdre les dernières validations, que la synchronisation
        // suivante refait ; mais une validation n'attend plus le disque — le
        // préchargement en fait une par message (0.6.6).
        base.pragma_update(None, "synchronous", "NORMAL")?;
        // L'interface lit pendant que les fils de travail écrivent, chacun par
        // sa propre connexion : une lecture qui tombe sur un verrou attend
        // plutôt que d'échouer.
        base.busy_timeout(std::time::Duration::from_secs(5))?;
        base.pragma_update(None, "foreign_keys", "ON")?;
        base.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id         INTEGER PRIMARY KEY,
                adresse    TEXT NOT NULL UNIQUE,
                hote       TEXT NOT NULL,
                port       INTEGER NOT NULL DEFAULT 993,
                utilisateur TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                id              INTEGER PRIMARY KEY,
                account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin          TEXT NOT NULL,
                nom             TEXT NOT NULL,
                profondeur      INTEGER NOT NULL DEFAULT 0,
                role            TEXT NOT NULL DEFAULT '',
                uidvalidity     INTEGER NOT NULL DEFAULT 0,
                uidnext         INTEGER NOT NULL DEFAULT 0,
                highestmodseq   INTEGER NOT NULL DEFAULT 0,
                hidden_local    INTEGER NOT NULL DEFAULT 0,
                favorite_rank   INTEGER,
                UNIQUE (account_id, chemin)
            );
            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY,
                folder_id   INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                uid         INTEGER NOT NULL,
                message_id  TEXT NOT NULL DEFAULT '',
                expediteur  TEXT NOT NULL DEFAULT '',
                adresse     TEXT NOT NULL DEFAULT '',
                sujet       TEXT NOT NULL DEFAULT '',
                date        TEXT NOT NULL DEFAULT '',
                taille      INTEGER NOT NULL DEFAULT 0,
                lu          INTEGER NOT NULL DEFAULT 0,
                repondu     INTEGER NOT NULL DEFAULT 0,
                body_state  TEXT NOT NULL DEFAULT 'headers',
                UNIQUE (folder_id, uid)
            );
            -- Adresses auxquelles on a écrit : elles complètent les expéditeurs
            -- de l'index dans les propositions de saisie.
            CREATE TABLE IF NOT EXISTS correspondants (
                adresse     TEXT PRIMARY KEY COLLATE NOCASE,
                nom         TEXT NOT NULL DEFAULT '',
                envois      INTEGER NOT NULL DEFAULT 0,
                dernier     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS pending_ops (
                id              INTEGER PRIMARY KEY,
                nature          TEXT NOT NULL DEFAULT 'deplacer',
                compte_source   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin_source   TEXT NOT NULL,
                validite_source INTEGER NOT NULL,
                uid_source      INTEGER NOT NULL,
                compte_cible    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin_cible    TEXT NOT NULL,
                message_id      TEXT NOT NULL DEFAULT '',
                drapeaux        TEXT NOT NULL DEFAULT '',
                date_interne    TEXT NOT NULL DEFAULT '',
                fichier         TEXT NOT NULL DEFAULT '',
                etape           TEXT NOT NULL DEFAULT 'deposer',
                erreur          TEXT NOT NULL DEFAULT '',
                cree            INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            "#,
        )?;
        let magasin = Magasin { base };
        magasin.migrer()?;
        magasin.poser_versions()?;
        Ok(magasin)
    }

    /// Numéro de version de chaque dossier, augmenté par l'index lui-même à
    /// toute écriture de ses messages, quelle que soit la connexion qui écrit :
    /// la liste affichée se sait à jour sans relire le dossier (0.6.6). Table et
    /// déclencheurs se posent à chaque ouverture, hors du schéma numéroté — un
    /// index déjà migré les reçoit sans changer de version.
    fn poser_versions(&self) -> Resultat<()> {
        self.base.execute_batch(
            "CREATE TABLE IF NOT EXISTS versions_dossiers (
                 folder_id INTEGER PRIMARY KEY,
                 version   INTEGER NOT NULL DEFAULT 0
             );
             CREATE TRIGGER IF NOT EXISTS version_ajout AFTER INSERT ON messages BEGIN
                 INSERT OR IGNORE INTO versions_dossiers (folder_id) VALUES (NEW.folder_id);
                 UPDATE versions_dossiers SET version = version + 1 WHERE folder_id = NEW.folder_id;
             END;
             CREATE TRIGGER IF NOT EXISTS version_modif AFTER UPDATE ON messages BEGIN
                 INSERT OR IGNORE INTO versions_dossiers (folder_id) VALUES (OLD.folder_id), (NEW.folder_id);
                 UPDATE versions_dossiers SET version = version + 1
                  WHERE folder_id IN (OLD.folder_id, NEW.folder_id);
             END;
             CREATE TRIGGER IF NOT EXISTS version_retrait AFTER DELETE ON messages BEGIN
                 INSERT OR IGNORE INTO versions_dossiers (folder_id) VALUES (OLD.folder_id);
                 UPDATE versions_dossiers SET version = version + 1 WHERE folder_id = OLD.folder_id;
             END;",
        )?;
        Ok(())
    }

    /// Version d'un dossier (cf. `poser_versions`) : elle change à chaque
    /// écriture de ses messages, et à elle seule.
    pub fn version_dossier(&self, dossier: i64) -> Resultat<i64> {
        Ok(self
            .base
            .prepare_cached("SELECT version FROM versions_dossiers WHERE folder_id = ?1")?
            .query_row(params![dossier], |l| l.get(0))
            .optional()?
            .unwrap_or(0))
    }

    /// Porte un index d'une version antérieure du schéma à la version courante.
    /// Les colonnes s'ajoutent ; rien ne se perd.
    fn migrer(&self) -> Resultat<()> {
        let version: i32 = self.base.pragma_query_value(None, "user_version", |l| l.get(0))?;
        if version >= VERSION_SCHEMA {
            return Ok(());
        }
        self.ajouter_colonne("accounts", "replie", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("folders", "separateur", "TEXT NOT NULL DEFAULT '/'")?;
        self.ajouter_colonne("folders", "selectionnable", "INTEGER NOT NULL DEFAULT 1")?;
        self.ajouter_colonne("folders", "nb_messages", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("folders", "nb_non_lus", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("messages", "horodatage", "INTEGER NOT NULL DEFAULT 0")?;
        self.base.execute_batch(
            "DROP INDEX IF EXISTS messages_par_date;
             CREATE INDEX IF NOT EXISTS messages_par_horodatage
                 ON messages(folder_id, horodatage DESC, uid DESC);",
        )?;
        // Version 3 : le marqueur de pièces jointes. Les en-têtes déjà indexés
        // ont été lus sans Content-Type : UIDVALIDITY remise à zéro, chaque
        // dossier est relu en entier à sa prochaine ouverture — en-têtes
        // seulement, l'état local des dossiers (favoris, masquage) restant.
        if version < 3 {
            self.ajouter_colonne("messages", "pieces", "INTEGER NOT NULL DEFAULT 0")?;
            self.ajouter_colonne("messages", "pieces_certain", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute("UPDATE folders SET uidvalidity = 0", [])?;
        }
        // Version 4 : suivi et importance ; même relecture, pour l'importance,
        // qui vient d'en-têtes jusque-là non demandés.
        if version < 4 {
            self.ajouter_colonne("messages", "suivi", "INTEGER NOT NULL DEFAULT 0")?;
            self.ajouter_colonne("messages", "importance", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute("UPDATE folders SET uidvalidity = 0", [])?;
        }
        // Version 5 : l'ordre des comptes dans l'arborescence, choisi par
        // l'utilisateur ; il part de l'ordre d'ajout.
        if version < 5 {
            self.ajouter_colonne("accounts", "rang", "INTEGER")?;
            self.base.execute("UPDATE accounts SET rang = id WHERE rang IS NULL", [])?;
            self.ajouter_colonne("folders", "replie_local", "INTEGER NOT NULL DEFAULT 0")?;
        }
        // Version 6 : les expéditeurs rencontrés, de quoi proposer une adresse
        // à la saisie sans parcourir tous les messages à chaque frappe ; et un
        // index des messages gardés sur le poste, que le ménage du cache
        // parcourait en entier.
        if version < 6 {
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS expediteurs (
                     adresse TEXT PRIMARY KEY,
                     nom     TEXT NOT NULL DEFAULT '',
                     nb      INTEGER NOT NULL DEFAULT 0,
                     dernier INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT OR IGNORE INTO expediteurs (adresse, nom, nb, dernier)
                     SELECT lower(adresse), MAX(expediteur), COUNT(*), MAX(horodatage) FROM messages
                     WHERE adresse <> '' GROUP BY lower(adresse);
                 CREATE INDEX IF NOT EXISTS messages_gardes ON messages(horodatage) WHERE body_state = 'full';",
            )?;
        }
        // Version 7 : l'agenda. Pour chaque compte, l'adresse CalDAV trouvée
        // et l'heure du dernier essai infructueux ; les agendas ; le texte de
        // chaque objet et la période qu'il occupe — de quoi afficher une
        // semaine hors connexion sans relire tous les objets.
        if version < 7 {
            self.ajouter_colonne("accounts", "caldav", "TEXT NOT NULL DEFAULT ''")?;
            self.ajouter_colonne("accounts", "caldav_essai", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS agendas (
                     id          INTEGER PRIMARY KEY,
                     account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     adresse     TEXT NOT NULL,
                     nom         TEXT NOT NULL DEFAULT '',
                     couleur     TEXT NOT NULL DEFAULT '',
                     ctag        TEXT NOT NULL DEFAULT '',
                     affiche     INTEGER NOT NULL DEFAULT 1,
                     UNIQUE (account_id, adresse)
                 );
                 CREATE TABLE IF NOT EXISTS evenements (
                     id          INTEGER PRIMARY KEY,
                     agenda_id   INTEGER NOT NULL REFERENCES agendas(id) ON DELETE CASCADE,
                     href        TEXT NOT NULL,
                     etag        TEXT NOT NULL,
                     ical        TEXT NOT NULL,
                     debut       INTEGER NOT NULL,
                     fin         INTEGER NOT NULL,
                     UNIQUE (agenda_id, href)
                 );
                 CREATE INDEX IF NOT EXISTS evenements_par_debut ON evenements(debut);",
            )?;
        }
        // Version 8 : l'écriture dans l'agenda. Le droit d'écriture de chaque
        // agenda, et l'état des rappels — vus, ou repoussés jusqu'à une heure.
        if version < 8 {
            self.ajouter_colonne("agendas", "ecriture", "INTEGER NOT NULL DEFAULT 1")?;
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS rappels (
                     cle       TEXT PRIMARY KEY,
                     vu        INTEGER NOT NULL DEFAULT 0,
                     repousse  INTEGER NOT NULL DEFAULT 0,
                     pose      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
                 );",
            )?;
        }
        // Version 9 : les invitations. Le serveur d'agenda du compte les
        // distribue-t-il lui-même (`calendar-auto-schedule`) ? -1 : pas encore
        // demandé.
        if version < 9 {
            self.ajouter_colonne("accounts", "caldav_planification", "INTEGER NOT NULL DEFAULT -1")?;
        }
        // Version 10 : « transféré » (`$Forwarded`), jusque-là ni lu ni gardé.
        // Les `HIGHESTMODSEQ` sont oubliés : chaque dossier relit tous ses
        // drapeaux à sa prochaine ouverture (une commande), pas ses en-têtes.
        if version < 10 {
            self.ajouter_colonne("messages", "transfere", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute("UPDATE folders SET highestmodseq = 0", [])?;
        }
        // Version 11 : « répondu à tous » (`$ReplyAll`, posé par MMail à partir
        // de la 0.5.5) ; aucun message ne le porte encore, rien à relire.
        if version < 11 {
            self.ajouter_colonne("messages", "repondu_tous", "INTEGER NOT NULL DEFAULT 0")?;
        }
        // Version 12 : un serveur d'envoi distinct de celui de la lecture,
        // vide pour tous les comptes déjà enregistrés — ils gardent le même.
        if version < 12 {
            self.ajouter_colonne("accounts", "hote_envoi", "TEXT NOT NULL DEFAULT ''")?;
        }
        // Version 13 : le mode d'authentification — vide, le mot de passe,
        // pour tous les comptes déjà enregistrés.
        if version < 13 {
            self.ajouter_colonne("accounts", "auth", "TEXT NOT NULL DEFAULT ''")?;
        }
        // Version 14 : les filtres (0.6.0, cf. `filtres`) — leur liste ; le
        // dernier message de chaque boîte de réception qu'ils ont lu ; les
        // messages qu'ils ont touchés, qu'un retour à la main ne refiltre pas.
        if version < 14 {
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS filtres (cle TEXT PRIMARY KEY, valeur TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS filtres_etat (
                     account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     chemin      TEXT NOT NULL,
                     uidvalidity INTEGER NOT NULL,
                     uid         INTEGER NOT NULL,
                     PRIMARY KEY (account_id, chemin)
                 );
                 CREATE TABLE IF NOT EXISTS filtres_vus (
                     account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     message_id  TEXT NOT NULL,
                     quand       INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                     PRIMARY KEY (account_id, message_id)
                 );",
            )?;
        }
        // Version 15 : les carnets d'adresses (0.6.1, cf. `carddav`) — la
        // collection de chaque compte, ses carnets, leurs fiches et les
        // adresses de courriel de chacune.
        if version < 15 {
            self.ajouter_colonne("accounts", "carddav", "TEXT NOT NULL DEFAULT ''")?;
            self.ajouter_colonne("accounts", "carddav_essai", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS carnets (
                     id         INTEGER PRIMARY KEY,
                     account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     url        TEXT NOT NULL,
                     nom        TEXT NOT NULL DEFAULT '',
                     ctag       TEXT NOT NULL DEFAULT '',
                     UNIQUE (account_id, url)
                 );
                 CREATE TABLE IF NOT EXISTS fiches (
                     id           INTEGER PRIMARY KEY,
                     carnet_id    INTEGER NOT NULL REFERENCES carnets(id) ON DELETE CASCADE,
                     href         TEXT NOT NULL,
                     etag         TEXT NOT NULL,
                     nom          TEXT NOT NULL DEFAULT '',
                     organisation TEXT NOT NULL DEFAULT '',
                     telephones   TEXT NOT NULL DEFAULT '',
                     UNIQUE (carnet_id, href)
                 );
                 CREATE TABLE IF NOT EXISTS fiches_courriels (
                     fiche_id INTEGER NOT NULL REFERENCES fiches(id) ON DELETE CASCADE,
                     adresse  TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS fiches_courriels_fiche ON fiches_courriels(fiche_id);",
            )?;
        }
        // Version 16 : les destinataires, que la liste des éléments envoyés
        // montre à la place de l'expéditeur (demande de Fabienne, 18/10). Les
        // dossiers d'envoyés sont relus à leur prochaine ouverture, en-têtes
        // seulement ; reconnus à leur rôle ou, faute de SPECIAL-USE (OVH), à
        // leur nom — un dossier pris à tort n'y perd qu'une relecture.
        if version < 16 {
            self.ajouter_colonne("messages", "destinataires", "TEXT NOT NULL DEFAULT ''")?;
            self.base.execute(
                "UPDATE folders SET uidvalidity = 0
                 WHERE role = 'Sent' OR nom LIKE '%sent%' OR nom LIKE '%envoy%'",
                [],
            )?;
        }
        // Version 17 : les adresses des boîtes partagées de chaque compte
        // (0.6.8), une par ligne ; vides jusqu'à la relecture suivante de
        // l'arborescence.
        if version < 17 {
            self.ajouter_colonne("accounts", "partagees", "TEXT NOT NULL DEFAULT ''")?;
        }
        self.base.pragma_update(None, "user_version", VERSION_SCHEMA)?;
        Ok(())
    }

    fn ajouter_colonne(&self, table: &str, colonne: &str, definition: &str) -> Resultat<()> {
        let existe: bool = self
            .base
            .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?
            .exists([colonne])?;
        if !existe {
            self.base
                .execute(&format!("ALTER TABLE {table} ADD COLUMN {colonne} {definition}"), [])?;
        }
        Ok(())
    }

    // ------------------------------------------------------------- comptes

    /// Identifiant d'un compte déjà enregistré, sans rien créer.
    /// Liste des filtres du profil, en JSON (cf. `filtres`) ; vide s'il n'y
    /// en a pas.
    pub fn filtres(&self) -> Resultat<String> {
        Ok(self
            .base
            .query_row("SELECT valeur FROM filtres WHERE cle = 'liste'", [], |l| l.get(0))
            .optional()?
            .unwrap_or_default())
    }

    pub fn poser_filtres(&self, json: &str) -> Resultat<()> {
        self.base.execute(
            "INSERT INTO filtres (cle, valeur) VALUES ('liste', ?1)
             ON CONFLICT (cle) DO UPDATE SET valeur = excluded.valeur",
            [json],
        )?;
        Ok(())
    }

    /// Dernier message du dossier `chemin` du compte que les filtres ont lu,
    /// et l'`UIDVALIDITY` sous laquelle ; `None` : jamais.
    pub fn filtres_etat(&self, compte: i64, chemin: &str) -> Resultat<Option<(u32, u32)>> {
        Ok(self
            .base
            .query_row(
                "SELECT uidvalidity, uid FROM filtres_etat WHERE account_id = ?1 AND chemin = ?2",
                params![compte, chemin],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .optional()?)
    }

    pub fn poser_filtres_etat(&self, compte: i64, chemin: &str, uidvalidity: u32, uid: u32) -> Resultat<()> {
        self.base.execute(
            "INSERT INTO filtres_etat (account_id, chemin, uidvalidity, uid) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account_id, chemin) DO UPDATE SET uidvalidity = excluded.uidvalidity, uid = excluded.uid",
            params![compte, chemin, uidvalidity, uid],
        )?;
        Ok(())
    }

    /// Messages du dossier `chemin` du compte au-delà de l'UID `apres`, dans
    /// l'ordre d'arrivée, et l'`UIDVALIDITY` du dossier — 0 s'il n'est pas
    /// encore indexé.
    pub fn recus_apres(&self, compte: i64, chemin: &str, apres: u32) -> Resultat<(u32, Vec<crate::filtres::Recu>)> {
        let dossier: Option<(i64, u32)> = self
            .base
            .query_row(
                "SELECT id, uidvalidity FROM folders WHERE account_id = ?1 AND chemin = ?2",
                params![compte, chemin],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .optional()?;
        let Some((dossier, uidvalidity)) = dossier else {
            return Ok((0, Vec::new()));
        };
        let mut requete = self.base.prepare(
            "SELECT uid, message_id, expediteur, adresse, sujet, lu FROM messages
             WHERE folder_id = ?1 AND uid > ?2 ORDER BY uid",
        )?;
        let recus = requete
            .query_map(params![dossier, apres], |l| {
                Ok(crate::filtres::Recu {
                    uid: l.get(0)?,
                    message_id: l.get(1)?,
                    expediteur: l.get(2)?,
                    adresse: l.get(3)?,
                    sujet: l.get(4)?,
                    lu: l.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((uidvalidity, recus))
    }

    /// `UIDVALIDITY` du dossier `chemin` du compte et son plus haut UID
    /// indexé ; `(0, 0)` s'il n'est pas encore indexé.
    pub fn uid_max(&self, compte: i64, chemin: &str) -> Resultat<(u32, u32)> {
        Ok(self
            .base
            .query_row(
                "SELECT f.uidvalidity, COALESCE(MAX(m.uid), 0) FROM folders f
                 LEFT JOIN messages m ON m.folder_id = f.id
                 WHERE f.account_id = ?1 AND f.chemin = ?2 GROUP BY f.id",
                params![compte, chemin],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .optional()?
            .unwrap_or((0, 0)))
    }

    /// Ceux des `Message-ID` donnés qu'un filtre a déjà touchés sur ce compte.
    pub fn filtres_vus(&self, compte: i64, ids: &[String]) -> Resultat<HashSet<String>> {
        let mut requete =
            self.base.prepare("SELECT 1 FROM filtres_vus WHERE account_id = ?1 AND message_id = ?2")?;
        let mut vus = HashSet::new();
        for id in ids.iter().filter(|id| !id.is_empty()) {
            if requete.exists(params![compte, id])? {
                vus.insert(id.clone());
            }
        }
        Ok(vus)
    }

    /// Note les messages touchés par un filtre ; oublie ceux d'il y a plus
    /// de 90 jours.
    pub fn noter_filtres_vus(&self, compte: i64, ids: &[String]) -> Resultat<()> {
        for id in ids.iter().filter(|id| !id.is_empty()) {
            self.base.execute(
                "INSERT OR REPLACE INTO filtres_vus (account_id, message_id) VALUES (?1, ?2)",
                params![compte, id],
            )?;
        }
        self.base.execute("DELETE FROM filtres_vus WHERE quand < strftime('%s','now') - 90 * 86400", [])?;
        Ok(())
    }

    pub fn compte_connu(&self, adresse: &str) -> Resultat<Option<i64>> {
        Ok(self
            .base
            .query_row("SELECT id FROM accounts WHERE adresse = ?1", [adresse], |l| l.get(0))
            .optional()?)
    }

    /// Enregistre un compte, ou le retrouve s'il est déjà connu.
    pub fn compte(&self, adresse: &str, hote: &str, port: u16, utilisateur: &str) -> Resultat<i64> {
        self.base.execute(
            "INSERT INTO accounts (adresse, hote, port, utilisateur) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(adresse) DO UPDATE SET hote = ?2, port = ?3, utilisateur = ?4",
            params![adresse, hote, port, utilisateur],
        )?;
        // Un compte nouveau prend place à la fin.
        self.base.execute(
            "UPDATE accounts SET rang = (SELECT COALESCE(MAX(rang), 0) + 1 FROM accounts)
             WHERE adresse = ?1 AND rang IS NULL",
            params![adresse],
        )?;
        Ok(self.base.query_row(
            "SELECT id FROM accounts WHERE adresse = ?1",
            params![adresse],
            |l| l.get(0),
        )?)
    }

    /// Pose le serveur d'envoi d'un compte ; vide, c'est celui de la lecture.
    /// Le compte doit exister : rien n'est créé.
    pub fn poser_serveur_envoi(&self, adresse: &str, hote: &str) -> Resultat<bool> {
        let n = self.base.execute(
            "UPDATE accounts SET hote_envoi = ?2 WHERE adresse = ?1",
            params![adresse, hote.trim().to_ascii_lowercase()],
        )?;
        Ok(n > 0)
    }

    /// Pose les adresses des boîtes partagées d'un compte. Vrai si elles ont
    /// changé.
    pub fn poser_partagees(&self, compte: i64, adresses: &[String]) -> Resultat<bool> {
        let n = self.base.execute(
            "UPDATE accounts SET partagees = ?2 WHERE id = ?1 AND partagees <> ?2",
            params![compte, adresses.join("\n")],
        )?;
        Ok(n > 0)
    }

    /// Pose le mode d'authentification d'un compte (`AUTH_MICROSOFT`,
    /// `AUTH_GOOGLE`, ou
    /// vide pour un mot de passe). Le compte doit exister : rien n'est créé.
    pub fn poser_auth(&self, adresse: &str, auth: &str) -> Resultat<bool> {
        let n = self.base.execute("UPDATE accounts SET auth = ?2 WHERE adresse = ?1", params![adresse, auth])?;
        Ok(n > 0)
    }

    /// Comptes du profil, dans l'ordre choisi par l'utilisateur — à défaut,
    /// celui où ils ont été ajoutés.
    pub fn comptes(&self) -> Resultat<Vec<CompteLocal>> {
        let mut requete = self.base.prepare(
            "SELECT id, adresse, hote, port, utilisateur, replie, hote_envoi, auth, partagees FROM accounts
             ORDER BY rang IS NULL, rang, id",
        )?;
        let comptes = requete
            .query_map([], |l| {
                Ok(CompteLocal {
                    id: l.get(0)?,
                    adresse: l.get(1)?,
                    hote: l.get(2)?,
                    port: l.get(3)?,
                    utilisateur: l.get(4)?,
                    replie: l.get::<_, i32>(5)? != 0,
                    hote_envoi: l.get(6)?,
                    auth: l.get(7)?,
                    partagees: l
                        .get::<_, String>(8)?
                        .lines()
                        .filter(|a| !a.is_empty())
                        .map(str::to_string)
                        .collect(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(comptes)
    }

    pub fn compte_par_id(&self, id: i64) -> Resultat<Option<CompteLocal>> {
        Ok(self.comptes()?.into_iter().find(|c| c.id == id))
    }

    /// Retire un compte du profil, avec son index. Rien n'est touché sur le
    /// serveur.
    pub fn retirer_compte(&self, id: i64) -> Resultat<()> {
        self.base.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Place un compte juste avant un autre, ou à la fin si `avant` est absent.
    pub fn placer_compte(&self, id: i64, avant: Option<i64>) -> Resultat<()> {
        let mut ordre: Vec<i64> = self.comptes()?.into_iter().map(|c| c.id).filter(|&c| c != id).collect();
        let place = avant.and_then(|a| ordre.iter().position(|&c| c == a)).unwrap_or(ordre.len());
        ordre.insert(place, id);
        self.ranger_comptes(&ordre)
    }

    /// Monte (`sens` négatif) ou descend un compte d'une place.
    pub fn decaler_compte(&self, id: i64, sens: i32) -> Resultat<()> {
        let mut ordre: Vec<i64> = self.comptes()?.into_iter().map(|c| c.id).collect();
        let Some(i) = ordre.iter().position(|&c| c == id) else {
            return Ok(());
        };
        let j = if sens < 0 { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < ordre.len()) };
        if let Some(j) = j {
            ordre.swap(i, j);
            self.ranger_comptes(&ordre)?;
        }
        Ok(())
    }

    fn ranger_comptes(&self, ordre: &[i64]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (rang, id) in ordre.iter().enumerate() {
            transaction.execute("UPDATE accounts SET rang = ?2 WHERE id = ?1", params![id, rang as i64 + 1])?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn replier_compte(&self, id: i64, replie: bool) -> Resultat<()> {
        self.base
            .execute("UPDATE accounts SET replie = ?2 WHERE id = ?1", params![id, replie as i32])?;
        Ok(())
    }

    /// Range les comptes dans l'ordre de ces adresses ; ceux qu'elles ne
    /// nomment pas suivent, dans leur ordre.
    pub fn ordonner_comptes(&self, adresses: &[String]) -> Resultat<()> {
        let comptes = self.comptes()?;
        let mut ordre: Vec<i64> = adresses
            .iter()
            .filter_map(|a| comptes.iter().find(|c| &c.adresse == a).map(|c| c.id))
            .collect();
        ordre.extend(comptes.iter().map(|c| c.id).filter(|id| !ordre.contains(id)).collect::<Vec<_>>());
        self.ranger_comptes(&ordre)
    }

    /// Remet l'état local d'un dossier venu d'une sauvegarde : masqué,
    /// favori, replié. Un dossier pas encore connu est créé ; la
    /// synchronisation le complète, ou le retire s'il n'existe plus sur le
    /// serveur.
    pub fn restaurer_dossier(&self, compte: i64, d: &DossierLocal) -> Resultat<()> {
        self.base.execute(
            "INSERT INTO folders (account_id, chemin, nom, profondeur, role, separateur, selectionnable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(account_id, chemin) DO NOTHING",
            params![compte, d.chemin, d.nom, d.profondeur, d.role, d.separateur, d.selectionnable as i32],
        )?;
        self.base.execute(
            "UPDATE folders SET hidden_local = ?3, favorite_rank = ?4, replie_local = ?5
             WHERE account_id = ?1 AND chemin = ?2",
            params![compte, d.chemin, d.masque as i32, d.favori, d.replie as i32],
        )?;
        Ok(())
    }

    /// Remet la case « affiché » d'un agenda venu d'une sauvegarde. Un agenda
    /// pas encore connu est créé ; la synchronisation le complète, ou le
    /// retire si le serveur ne le décrit plus.
    pub fn restaurer_agenda(&self, a: &AgendaLocal) -> Resultat<()> {
        self.base.execute(
            "INSERT INTO agendas (account_id, adresse, nom, couleur, ecriture, affiche)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, adresse) DO UPDATE SET affiche = ?6",
            params![a.compte, a.adresse, a.nom, a.couleur, a.ecriture as i32, a.affiche as i32],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------ dossiers

    /// Remplace l'arborescence d'un compte par celle que le serveur annonce.
    ///
    /// Les dossiers disparus côté serveur s'en vont avec leurs messages ; ceux
    /// qui restent gardent leur état local (masquage, favoris) — c'est tout
    /// l'intérêt de mettre à jour plutôt que de tout réécrire.
    pub fn poser_dossiers(&self, compte: i64, dossiers: &[Dossier]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        let chemins: Vec<&str> = dossiers.iter().map(|d| d.chemin.as_str()).collect();
        for dossier in dossiers {
            transaction.execute(
                "INSERT INTO folders (account_id, chemin, nom, profondeur, role, separateur, selectionnable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(account_id, chemin)
                 DO UPDATE SET nom = ?3, profondeur = ?4, role = ?5, separateur = ?6,
                               selectionnable = ?7",
                params![
                    compte,
                    dossier.chemin,
                    dossier.nom(),
                    dossier.profondeur() as u32,
                    dossier.role().unwrap_or(""),
                    dossier.separateur,
                    dossier.selectionnable() as i32
                ],
            )?;
        }
        let disparus: Vec<i64> = {
            let mut connus =
                transaction.prepare("SELECT id, chemin FROM folders WHERE account_id = ?1")?;
            let lignes = connus
                .query_map(params![compte], |l| Ok((l.get::<_, i64>(0)?, l.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            lignes
                .into_iter()
                .filter(|(_, chemin)| !chemins.contains(&chemin.as_str()))
                .map(|(id, _)| id)
                .collect()
        };
        for id in disparus {
            transaction.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Note les compteurs que le serveur annonce pour les dossiers d'un compte.
    pub fn poser_compteurs(&self, compte: i64, statuts: &[Statut]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for statut in statuts {
            transaction.execute(
                "UPDATE folders SET nb_messages = ?3, nb_non_lus = ?4
                 WHERE account_id = ?1 AND chemin = ?2",
                params![compte, statut.chemin, statut.messages, statut.non_lus],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Recompte un dossier d'après son index, une fois celui-ci à jour.
    pub fn recompter(&self, dossier: i64) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET
                nb_messages = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1),
                nb_non_lus  = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1 AND lu = 0)
             WHERE id = ?1",
            params![dossier],
        )?;
        Ok(())
    }

    /// Identifiant interne d'un dossier.
    pub fn dossier_id(&self, compte: i64, chemin: &str) -> Resultat<i64> {
        Ok(self.base.query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND chemin = ?2",
            params![compte, chemin],
            |l| l.get(0),
        )?)
    }

    /// Un dossier par son identifiant interne.
    pub fn dossier(&self, id: i64) -> Resultat<Option<DossierLocal>> {
        let mut requete = self.base.prepare(&format!("{SELECT_DOSSIER} WHERE f.id = ?1"))?;
        Ok(requete.query_row(params![id], lire_dossier).optional()?)
    }

    /// Oublie le `HIGHESTMODSEQ` d'un dossier : la prochaine synchronisation
    /// relira tous ses drapeaux au lieu de demander ce qui a changé.
    pub fn oublier_modseq(&self, dossier: i64) -> Resultat<()> {
        self.base.execute("UPDATE folders SET highestmodseq = 0 WHERE id = ?1", params![dossier])?;
        Ok(())
    }

    /// Note l'état de synchronisation d'un dossier après `SELECT`/`EXAMINE`.
    ///
    /// Rend vrai si l'`UIDVALIDITY` a changé : le contenu local du dossier est
    /// alors caduc et doit être relu intégralement (décision 15).
    pub fn poser_etat(&self, dossier: i64, etat: &EtatDossier) -> Resultat<bool> {
        let ancienne: u32 = self
            .base
            .query_row("SELECT uidvalidity FROM folders WHERE id = ?1", params![dossier], |l| {
                l.get(0)
            })
            .optional()?
            .unwrap_or(0);
        let rupture = ancienne != 0 && ancienne != etat.uid_validity;
        if rupture {
            self.base
                .execute("DELETE FROM messages WHERE folder_id = ?1", params![dossier])?;
        }
        self.base.execute(
            "UPDATE folders SET uidvalidity = ?2, uidnext = ?3, highestmodseq = ?4 WHERE id = ?1",
            params![dossier, etat.uid_validity, etat.uid_next, etat.highest_mod_seq],
        )?;
        Ok(rupture)
    }

    /// Dossiers d'un compte, dans l'ordre d'affichage, masqués compris.
    ///
    /// INBOX d'abord — c'est là qu'on va — puis les dossiers à rôle, puis les
    /// autres par ordre alphabétique, chacun sous son parent.
    pub fn dossiers(&self, compte: i64) -> Resultat<Vec<DossierLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_DOSSIER} WHERE f.account_id = ?1
             ORDER BY (f.chemin = 'INBOX') DESC, (f.role != '') DESC, f.chemin"
        ))?;
        let dossiers = requete
            .query_map(params![compte], lire_dossier)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(dossiers)
    }

    /// Dossiers de la rubrique Favoris, tous comptes confondus, dans leur ordre.
    pub fn favoris(&self) -> Resultat<Vec<DossierLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_DOSSIER} WHERE f.favorite_rank IS NOT NULL ORDER BY f.favorite_rank, f.id"
        ))?;
        let dossiers = requete
            .query_map([], lire_dossier)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(dossiers)
    }

    /// Replie — ou déplie — les sous-dossiers d'un dossier dans l'arborescence.
    pub fn replier_dossier(&self, dossier: i64, replie: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET replie_local = ?2 WHERE id = ?1",
            params![dossier, replie as i32],
        )?;
        Ok(())
    }

    /// Masque — ou réaffiche — un dossier dans l'arborescence (décision 4).
    pub fn masquer(&self, dossier: i64, masque: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET hidden_local = ?2 WHERE id = ?1",
            params![dossier, masque as i32],
        )?;
        Ok(())
    }

    /// Épingle un dossier en fin de rubrique Favoris, ou l'en retire.
    pub fn epingler(&self, dossier: i64, favori: bool) -> Resultat<()> {
        if favori {
            self.base.execute(
                "UPDATE folders SET favorite_rank =
                    (SELECT COALESCE(MAX(favorite_rank), 0) + 1 FROM folders)
                 WHERE id = ?1 AND favorite_rank IS NULL",
                params![dossier],
            )?;
        } else {
            self.base
                .execute("UPDATE folders SET favorite_rank = NULL WHERE id = ?1", params![dossier])?;
        }
        Ok(())
    }

    /// Place un dossier dans la rubrique Favoris, juste avant un autre favori,
    /// ou à la fin si `avant` est absent. Un dossier déjà favori change de
    /// place ; les rangs des autres se décalent pour lui faire de la place.
    pub fn placer_favori(&self, dossier: i64, avant: Option<i64>) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        transaction
            .execute("UPDATE folders SET favorite_rank = NULL WHERE id = ?1", params![dossier])?;
        let rang: Option<i64> = match avant {
            Some(autre) if autre != dossier => transaction
                .query_row(
                    "SELECT favorite_rank FROM folders WHERE id = ?1",
                    params![autre],
                    |l| l.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten(),
            _ => None,
        };
        match rang {
            Some(rang) => {
                transaction.execute(
                    "UPDATE folders SET favorite_rank = favorite_rank + 1
                     WHERE favorite_rank IS NOT NULL AND favorite_rank >= ?1",
                    params![rang],
                )?;
                transaction.execute(
                    "UPDATE folders SET favorite_rank = ?2 WHERE id = ?1",
                    params![dossier, rang],
                )?;
            }
            None => {
                transaction.execute(
                    "UPDATE folders SET favorite_rank =
                        (SELECT COALESCE(MAX(favorite_rank), 0) + 1 FROM folders)
                     WHERE id = ?1",
                    params![dossier],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------ messages

    /// Range les messages d'un dossier. Un message déjà connu ne voit changer
    /// que ses drapeaux.
    pub fn poser_messages(&self, dossier: i64, messages: &[MessageLocal]) -> Resultat<()> {
        for lot in messages.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            {
                let mut insertion = transaction.prepare(
                    "INSERT OR IGNORE INTO messages
                       (folder_id, uid, message_id, expediteur, adresse, sujet, date, horodatage,
                        taille, lu, repondu, pieces, suivi, importance, transfere, repondu_tous, destinataires)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                )?;
                let mut mise_a_jour = transaction.prepare(
                    "UPDATE messages SET lu = ?3, repondu = ?4, suivi = ?6, importance = ?7, transfere = ?8, repondu_tous = ?9,
                        pieces = CASE WHEN pieces_certain = 1 THEN pieces ELSE ?5 END, destinataires = ?10
                     WHERE folder_id = ?1 AND uid = ?2",
                )?;
                // Un expéditeur compte une fois par message nouveau dans l'index.
                let mut expediteur = transaction.prepare(
                    "INSERT INTO expediteurs (adresse, nom, nb, dernier) VALUES (lower(?1), ?2, 1, ?3)
                     ON CONFLICT(adresse) DO UPDATE SET nb = nb + 1, dernier = MAX(dernier, excluded.dernier),
                        nom = CASE WHEN excluded.nom <> '' THEN excluded.nom ELSE nom END",
                )?;
                for m in lot {
                    let nouveau = insertion.execute(params![
                        dossier,
                        m.uid,
                        m.message_id,
                        m.expediteur,
                        m.adresse,
                        m.sujet,
                        m.date,
                        m.horodatage,
                        m.taille,
                        m.lu as i32,
                        m.repondu as i32,
                        m.pieces as i32,
                        m.suivi as i32,
                        m.importance as i32,
                        m.transfere as i32,
                        m.repondu_tous as i32,
                        m.destinataires
                    ])? == 1;
                    if nouveau {
                        if !m.adresse.is_empty() {
                            expediteur.execute(params![m.adresse, m.expediteur, m.horodatage])?;
                        }
                    } else {
                        mise_a_jour.execute(params![
                            dossier,
                            m.uid,
                            m.lu as i32,
                            m.repondu as i32,
                            m.pieces as i32,
                            m.suivi as i32,
                            m.importance as i32,
                            m.transfere as i32,
                            m.repondu_tous as i32,
                            m.destinataires
                        ])?;
                    }
                }
            }
            transaction.commit()?;
        }
        Ok(())
    }

    /// Applique des drapeaux rendus par le serveur à des messages connus.
    /// Rend le nombre de messages dont un drapeau a réellement changé : sans
    /// QRESYNC, chaque veille repasse les drapeaux de tout le dossier, et une
    /// ligne inchangée n'est pas réécrite (ni sa version, ni le disque).
    pub fn poser_drapeaux(&self, dossier: i64, drapeaux: &[(u32, Vec<String>)]) -> Resultat<usize> {
        let mut changes = 0;
        for lot in drapeaux.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            {
                let mut requete = transaction.prepare_cached(
                    "UPDATE messages SET lu = ?3, repondu = ?4, suivi = ?5, transfere = ?6, repondu_tous = ?7
                     WHERE folder_id = ?1 AND uid = ?2
                       AND (lu, repondu, suivi, transfere, repondu_tous) IS NOT (?3, ?4, ?5, ?6, ?7)",
                )?;
                for (uid, liste) in lot {
                    let lu = liste.iter().any(|d| d.eq_ignore_ascii_case("\\Seen"));
                    let repondu = protocole::porte(liste, protocole::REPONDU);
                    let transfere = protocole::porte(liste, protocole::TRANSFERE);
                    let repondu_tous = protocole::porte(liste, protocole::REPONDU_TOUS);
                    let suivi = liste.iter().any(|d| d.eq_ignore_ascii_case("\\Flagged"));
                    changes += requete.execute(params![
                        dossier,
                        uid,
                        lu as i32,
                        repondu as i32,
                        suivi as i32,
                        transfere as i32,
                        repondu_tous as i32
                    ])?;
                }
            }
            transaction.commit()?;
        }
        Ok(changes)
    }

    // ------------------------------------------------------ correspondants

    /// Note les destinataires d'un message envoyé : un nom connu n'est pas
    /// effacé par une adresse saisie sans nom.
    pub fn noter_correspondants(&self, adresses: &[(String, String)], quand: i64) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (nom, adresse) in adresses {
            transaction.execute(
                "INSERT INTO correspondants (adresse, nom, envois, dernier) VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT(adresse) DO UPDATE SET
                    envois = envois + 1, dernier = ?3,
                    nom = CASE WHEN ?2 <> '' THEN ?2 ELSE nom END",
                params![adresse, nom, quand],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Adresses proposées à la saisie : celles à qui l'on a écrit, puis les
    /// expéditeurs de l'index, dont l'adresse ou le nom contient `filtre`. Les
    /// plus fréquentes d'abord.
    pub fn contacts(&self, filtre: &str, limite: usize) -> Resultat<Vec<(String, String)>> {
        let motif = format!(
            "%{}%",
            filtre.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        );
        // Un seul agrégat par groupe : SQLite rend alors le nom et l'adresse de
        // la ligne qui le porte (un correspondant avant un expéditeur, le plus
        // récent à poids égal). Avec deux, le nom venait d'une ligne quelconque.
        let mut requete = self.base.prepare(
            "SELECT nom, adresse, MAX(poids * 10000000000 + dernier) AS cle FROM (
                SELECT f.nom, c.adresse, 2000000 AS poids, 0 AS dernier
                  FROM fiches_courriels c JOIN fiches f ON f.id = c.fiche_id
                 WHERE c.adresse LIKE ?1 ESCAPE '\\' OR f.nom LIKE ?1 ESCAPE '\\'
                UNION ALL
                SELECT nom, adresse, 1000000 + envois AS poids, dernier FROM correspondants
                 WHERE adresse LIKE ?1 ESCAPE '\\' OR nom LIKE ?1 ESCAPE '\\'
                UNION ALL
                SELECT nom, adresse, nb, dernier FROM expediteurs
                 WHERE adresse LIKE ?1 ESCAPE '\\' OR nom LIKE ?1 ESCAPE '\\'
             )
             GROUP BY lower(adresse)
             ORDER BY cle DESC
             LIMIT ?2",
        )?;
        let lignes = requete
            .query_map(params![motif, limite as i64], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        Ok(lignes)
    }

    /// Pièces jointes constatées à la lecture d'un message : elles priment
    /// désormais sur ce que les en-têtes laissaient supposer.
    pub fn poser_pieces(&self, dossier: i64, uid: u32, pieces: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE messages SET pieces = ?3, pieces_certain = 1
             WHERE folder_id = ?1 AND uid = ?2 AND (pieces, pieces_certain) IS NOT (?3, 1)",
            params![dossier, uid, pieces as i32],
        )?;
        Ok(())
    }

    /// Pose ou retire localement le drapeau de suivi.
    pub fn marquer_suivi(&self, dossier: i64, uids: &[u32], suivi: bool) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for uid in uids {
            transaction.execute(
                "UPDATE messages SET suivi = ?3 WHERE folder_id = ?1 AND uid = ?2 AND suivi IS NOT ?3",
                params![dossier, uid, suivi as i32],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Marque localement des messages comme lus ou non lus.
    pub fn marquer_lu(&self, dossier: i64, uids: &[u32], lu: bool) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for uid in uids {
            transaction.execute(
                "UPDATE messages SET lu = ?3 WHERE folder_id = ?1 AND uid = ?2 AND lu IS NOT ?3",
                params![dossier, uid, lu as i32],
            )?;
        }
        transaction.commit()?;
        self.recompter(dossier)
    }

    /// Retire de l'index des intervalles d'UID (`VANISHED`).
    pub fn retirer_intervalles(&self, dossier: i64, intervalles: &[(u32, u32)]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (debut, fin) in intervalles {
            transaction.execute(
                "DELETE FROM messages WHERE folder_id = ?1 AND uid BETWEEN ?2 AND ?3",
                params![dossier, debut, fin],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Retire des messages désignés de l'index.
    pub fn retirer(&self, dossier: i64, uids: &[u32]) -> Resultat<()> {
        let intervalles: Vec<(u32, u32)> = uids.iter().map(|&u| (u, u)).collect();
        self.retirer_intervalles(dossier, &intervalles)
    }

    /// UID connus d'un dossier.
    pub fn uids(&self, dossier: i64) -> Resultat<Vec<u32>> {
        let mut requete = self.base.prepare("SELECT uid FROM messages WHERE folder_id = ?1")?;
        let uids = requete
            .query_map(params![dossier], |l| l.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(uids)
    }

    /// Messages dont chaque mot de `texte` figure dans l'objet, le nom ou
    /// l'adresse de l'expéditeur : d'un dossier, ou de tous les comptes
    /// (`dossier` absent), les plus récents d'abord, `limite` au plus.
    pub fn chercher(&self, dossier: Option<i64>, texte: &str, limite: usize) -> Resultat<Vec<Trouve>> {
        let mots: Vec<String> = texte
            .split_whitespace()
            .map(|m| format!("%{}%", m.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
            .collect();
        if mots.is_empty() {
            return Ok(Vec::new());
        }
        let mut conditions: Vec<String> = (0..mots.len())
            .map(|i| {
                let n = i + 2;
                format!(
                    "(m.sujet LIKE ?{n} ESCAPE '\\' OR m.expediteur LIKE ?{n} ESCAPE '\\' OR m.adresse LIKE ?{n} ESCAPE '\\')"
                )
            })
            .collect();
        if dossier.is_some() {
            conditions.push("m.folder_id = ?1".into());
        }
        let sql = format!(
            "{SELECT_TROUVE} WHERE {} ORDER BY m.horodatage DESC, m.uid DESC LIMIT {limite}",
            conditions.join(" AND ")
        );
        let mut valeurs: Vec<rusqlite::types::Value> = vec![dossier.unwrap_or(0).into()];
        valeurs.extend(mots.into_iter().map(rusqlite::types::Value::from));
        let mut requete = self.base.prepare(&sql)?;
        let trouves = requete
            .query_map(rusqlite::params_from_iter(valeurs), lire_trouve)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(trouves)
    }

    /// Messages non lus des boîtes de réception de tous les comptes : la
    /// pastille de la barre des tâches. Compteurs des dossiers — ceux du
    /// serveur, puis de l'index une fois le dossier synchronisé.
    pub fn non_lus_receptions(&self) -> Resultat<u32> {
        Ok(self.base.query_row(
            "SELECT COALESCE(SUM(nb_non_lus), 0) FROM folders WHERE chemin = 'INBOX'",
            [],
            |l| l.get(0),
        )?)
    }

    /// Messages des boîtes de réception de tous les comptes, du plus récent au
    /// plus ancien : la boîte de réception unifiée (« toutes les BàL »).
    pub fn boites_de_reception(&self, limite: usize) -> Resultat<Vec<Trouve>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_TROUVE} WHERE f.chemin = 'INBOX' ORDER BY m.horodatage DESC, m.uid DESC LIMIT {limite}"
        ))?;
        let trouves = requete.query_map([], lire_trouve)?.collect::<Result<Vec<_>, _>>()?;
        Ok(trouves)
    }

    /// Messages d'un dossier désignés par leur UID, du plus récent au plus
    /// ancien.
    pub fn messages_par_uid(&self, dossier: i64, uids: &[u32]) -> Resultat<Vec<MessageLocal>> {
        let mut trouves = Vec::new();
        let mut requete = self.base.prepare(&format!("{SELECT_MESSAGE} WHERE folder_id = ?1 AND uid = ?2"))?;
        for uid in uids {
            if let Some(m) = requete.query_row(params![dossier, uid], lire_message).optional()? {
                trouves.push(m);
            }
        }
        trouves.sort_by(|a, b| (b.horodatage, b.uid).cmp(&(a.horodatage, a.uid)));
        Ok(trouves)
    }

    /// Comme `messages_par_uid`, avec le compte et le dossier de chaque
    /// message : les résultats du serveur dans une recherche sur toutes les
    /// boîtes.
    pub fn trouves_par_uid(&self, dossier: i64, uids: &[u32]) -> Resultat<Vec<Trouve>> {
        let mut trouves = Vec::new();
        let mut requete = self.base.prepare(&format!("{SELECT_TROUVE} WHERE m.folder_id = ?1 AND m.uid = ?2"))?;
        for uid in uids {
            if let Some(t) = requete.query_row(params![dossier, uid], lire_trouve).optional()? {
                trouves.push(t);
            }
        }
        trouves.sort_by(|a, b| {
            (b.message.horodatage, b.message.uid).cmp(&(a.message.horodatage, a.message.uid))
        });
        Ok(trouves)
    }

    /// Messages d'un dossier, du plus récent au plus ancien — par date de
    /// réception, que le déplacement entre boîtes conserve, et non par UID,
    /// qu'il renouvelle.
    pub fn messages(&self, dossier: i64) -> Resultat<Vec<MessageLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_MESSAGE} WHERE folder_id = ?1 ORDER BY horodatage DESC, uid DESC"
        ))?;
        let messages = requete
            .query_map(params![dossier], lire_message)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    /// Une page de la liste d'un dossier, du plus récent au plus ancien : les
    /// `limite` messages qui suivent `apres` (date, UID) dans cet ordre — ou les
    /// premiers. Lecture par clé, sans `OFFSET` : l'index
    /// `messages_par_horodatage` y mène directement, quelle que soit la page.
    pub fn messages_page(&self, dossier: i64, apres: Option<(i64, u32)>, limite: usize) -> Resultat<Vec<MessageLocal>> {
        let limite = limite as i64;
        let messages = match apres {
            None => self
                .base
                .prepare(&format!(
                    "{SELECT_MESSAGE} WHERE folder_id = ?1 ORDER BY horodatage DESC, uid DESC LIMIT ?2"
                ))?
                .query_map(params![dossier, limite], lire_message)?
                .collect::<Result<Vec<_>, _>>()?,
            Some((h, uid)) => self
                .base
                .prepare(&format!(
                    "{SELECT_MESSAGE} WHERE folder_id = ?1 AND (horodatage, uid) < (?2, ?3)
                     ORDER BY horodatage DESC, uid DESC LIMIT ?4"
                ))?
                .query_map(params![dossier, h, uid, limite], lire_message)?
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(messages)
    }

    /// Messages d'un dossier jusqu'à `borne` (date, UID) comprise, du plus
    /// récent au plus ancien ; tous sans borne.
    pub fn messages_jusqua(&self, dossier: i64, borne: Option<(i64, u32)>) -> Resultat<Vec<MessageLocal>> {
        let Some((h, uid)) = borne else {
            return self.messages(dossier);
        };
        let mut requete = self.base.prepare(&format!(
            "{SELECT_MESSAGE} WHERE folder_id = ?1 AND (horodatage, uid) >= (?2, ?3)
             ORDER BY horodatage DESC, uid DESC"
        ))?;
        let messages = requete
            .query_map(params![dossier, h, uid], lire_message)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    /// Un message de l'index.
    pub fn message(&self, dossier: i64, uid: u32) -> Resultat<Option<MessageLocal>> {
        let mut requete = self
            .base
            .prepare(&format!("{SELECT_MESSAGE} WHERE folder_id = ?1 AND uid = ?2"))?;
        Ok(requete.query_row(params![dossier, uid], lire_message).optional()?)
    }

    // -------------------------------------------- messages gardés sur le poste

    /// Identifiant dans l'index, date de réception et état « lu » d'un message.
    pub fn identite_message(&self, dossier: i64, uid: u32) -> Resultat<Option<(i64, i64, bool)>> {
        Ok(self
            .base
            .query_row(
                "SELECT id, horodatage, lu FROM messages WHERE folder_id = ?1 AND uid = ?2",
                params![dossier, uid],
                |l| Ok((l.get(0)?, l.get(1)?, l.get::<_, i32>(2)? != 0)),
            )
            .optional()?)
    }

    /// Identifiant et état « lu » d'un message gardé sur le poste (`full`) ;
    /// `None` s'il ne l'est pas, même si un fichier porte son identifiant.
    pub fn garde_sur_le_poste(&self, dossier: i64, uid: u32) -> Resultat<Option<(i64, bool)>> {
        Ok(self
            .base
            .query_row(
                "SELECT id, lu FROM messages WHERE folder_id = ?1 AND uid = ?2 AND body_state = 'full'",
                params![dossier, uid],
                |l| Ok((l.get(0)?, l.get::<_, i32>(1)? != 0)),
            )
            .optional()?)
    }

    /// Note qu'un message est gardé sur le poste (`full`), ne l'est plus
    /// (`headers`), ou n'a pas pu être lu (`echec` : il n'est pas réessayé).
    pub fn poser_etat_corps(&self, id: i64, etat: &str) -> Resultat<()> {
        self.base.execute("UPDATE messages SET body_state = ?2 WHERE id = ?1", params![id, etat])?;
        Ok(())
    }

    /// Messages d'un dossier reçus depuis `depuis` et pas encore gardés, du
    /// plus récent au plus ancien : identifiant et UID.
    pub fn a_garder(&self, dossier: i64, depuis: i64, nombre: usize, taille_max: i64) -> Resultat<Vec<(i64, u32)>> {
        let mut requete = self.base.prepare_cached(
            "SELECT id, uid FROM messages
             WHERE folder_id = ?1 AND body_state = 'headers' AND horodatage >= ?2 AND taille <= ?4
             ORDER BY horodatage DESC, uid DESC LIMIT ?3",
        )?;
        let liste = requete
            .query_map(params![dossier, depuis, nombre as i64, taille_max], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(liste)
    }

    /// Messages gardés sur le poste mais reçus avant `avant` : sortis de la
    /// fenêtre.
    pub fn gardes_perimes(&self, avant: i64) -> Resultat<Vec<i64>> {
        let mut requete =
            self.base.prepare("SELECT id FROM messages WHERE body_state = 'full' AND horodatage < ?1")?;
        let liste = requete.query_map(params![avant], |l| l.get(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(liste)
    }

    /// Identifiants de tous les messages gardés sur le poste.
    pub fn gardes(&self) -> Resultat<HashSet<i64>> {
        let mut requete = self.base.prepare("SELECT id FROM messages WHERE body_state = 'full'")?;
        let liste = requete.query_map([], |l| l.get(0))?.collect::<Result<HashSet<i64>, _>>()?;
        Ok(liste)
    }

    // ---------------------------------------------- déplacements en cours

    /// Inscrit un déplacement entre boîtes, une fois le message lu et copié.
    pub fn inscrire_operation(&self, op: &Operation) -> Resultat<i64> {
        self.base.execute(
            "INSERT INTO pending_ops
               (compte_source, chemin_source, validite_source, uid_source, compte_cible,
                chemin_cible, message_id, drapeaux, date_interne, fichier, etape)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                op.compte_source,
                op.chemin_source,
                op.validite_source,
                op.uid_source,
                op.compte_cible,
                op.chemin_cible,
                op.message_id,
                op.drapeaux,
                op.date_interne,
                op.fichier,
                op.etape.code()
            ],
        )?;
        Ok(self.base.last_insert_rowid())
    }

    pub fn avancer_operation(&self, id: i64, etape: Etape) -> Resultat<()> {
        self.base.execute(
            "UPDATE pending_ops SET etape = ?2, erreur = '' WHERE id = ?1",
            params![id, etape.code()],
        )?;
        Ok(())
    }

    pub fn noter_echec_operation(&self, id: i64, erreur: &str) -> Resultat<()> {
        self.base
            .execute("UPDATE pending_ops SET erreur = ?2 WHERE id = ?1", params![id, erreur])?;
        Ok(())
    }

    /// Solde un déplacement terminé.
    pub fn clore_operation(&self, id: i64) -> Resultat<()> {
        self.base.execute("DELETE FROM pending_ops WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Déplacements en cours dont un compte est la source.
    pub fn operations(&self, compte_source: i64) -> Resultat<Vec<Operation>> {
        let mut requete = self.base.prepare(
            "SELECT id, compte_source, chemin_source, validite_source, uid_source, compte_cible,
                    chemin_cible, message_id, drapeaux, date_interne, fichier, etape, erreur
             FROM pending_ops WHERE compte_source = ?1 ORDER BY id",
        )?;
        let operations = requete
            .query_map(params![compte_source], |l| {
                Ok(Operation {
                    id: l.get(0)?,
                    compte_source: l.get(1)?,
                    chemin_source: l.get(2)?,
                    validite_source: l.get(3)?,
                    uid_source: l.get(4)?,
                    compte_cible: l.get(5)?,
                    chemin_cible: l.get(6)?,
                    message_id: l.get(7)?,
                    drapeaux: l.get(8)?,
                    date_interne: l.get(9)?,
                    fichier: l.get(10)?,
                    etape: Etape::lire(&l.get::<_, String>(11)?),
                    erreur: l.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(operations)
    }

    /// Nombre de déplacements en cours, tous comptes confondus.
    pub fn nombre_operations(&self) -> Resultat<u32> {
        Ok(self.base.query_row("SELECT COUNT(*) FROM pending_ops", [], |l| l.get(0))?)
    }

    // ------------------------------------------------------------- agenda

    /// Adresse CalDAV d'un compte (vide si elle n'est pas connue) et heure du
    /// dernier essai de découverte infructueux.
    // ------------------------------------------------------ carnets d'adresses

    pub fn carddav(&self, compte: i64) -> Resultat<(String, i64)> {
        Ok(self
            .base
            .query_row("SELECT carddav, carddav_essai FROM accounts WHERE id = ?1", params![compte], |l| {
                Ok((l.get(0)?, l.get(1)?))
            })
            .optional()?
            .unwrap_or_default())
    }

    pub fn poser_carddav(&self, compte: i64, adresse: &str, essai: i64) -> Resultat<()> {
        self.base.execute(
            "UPDATE accounts SET carddav = ?2, carddav_essai = ?3 WHERE id = ?1",
            params![compte, adresse, essai],
        )?;
        Ok(())
    }

    /// Pose les carnets d'un compte : `(adresse, nom)`. Ceux qui ont disparu
    /// sont retirés, avec leurs fiches. Rend vrai si quelque chose a changé.
    pub fn poser_carnets(&self, compte: i64, carnets: &[(String, String)]) -> Resultat<bool> {
        let presents: HashSet<&str> = carnets.iter().map(|(u, _)| u.as_str()).collect();
        let mut change = false;
        let anciens: Vec<(i64, String)> = {
            let mut requete = self.base.prepare("SELECT id, url FROM carnets WHERE account_id = ?1")?;
            let lignes = requete.query_map(params![compte], |l| Ok((l.get(0)?, l.get(1)?)))?;
            lignes.collect::<Result<_, _>>()?
        };
        for (id, url) in &anciens {
            if !presents.contains(url.as_str()) {
                self.base.execute("DELETE FROM carnets WHERE id = ?1", params![id])?;
                change = true;
            }
        }
        for (url, nom) in carnets {
            let n = self.base.execute(
                "INSERT INTO carnets (account_id, url, nom) VALUES (?1, ?2, ?3)
                 ON CONFLICT (account_id, url) DO UPDATE SET nom = excluded.nom WHERE nom <> excluded.nom",
                params![compte, url, nom],
            )?;
            change |= n > 0;
        }
        Ok(change)
    }

    /// Carnets d'un compte : `(id, adresse, étiquette)`.
    pub fn carnets(&self, compte: i64) -> Resultat<Vec<(i64, String, String)>> {
        let mut requete = self.base.prepare("SELECT id, url, ctag FROM carnets WHERE account_id = ?1 ORDER BY nom")?;
        let lignes = requete.query_map(params![compte], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)))?;
        Ok(lignes.collect::<Result<_, _>>()?)
    }

    pub fn poser_ctag_carnet(&self, carnet: i64, ctag: &str) -> Resultat<()> {
        self.base.execute("UPDATE carnets SET ctag = ?2 WHERE id = ?1", params![carnet, ctag])?;
        Ok(())
    }

    /// ETag des fiches d'un carnet, par href.
    pub fn etags_contacts(&self, carnet: i64) -> Resultat<HashMap<String, String>> {
        let mut requete = self.base.prepare("SELECT href, etag FROM fiches WHERE carnet_id = ?1")?;
        let lignes = requete.query_map(params![carnet], |l| Ok((l.get(0)?, l.get(1)?)))?;
        Ok(lignes.collect::<Result<_, _>>()?)
    }

    pub fn retirer_contacts(&self, carnet: i64, hrefs: &[String]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for href in hrefs {
            transaction.execute("DELETE FROM fiches WHERE carnet_id = ?1 AND href = ?2", params![carnet, href])?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Range des fiches lues : `(href, etag, contact)`.
    pub fn poser_contacts(&self, carnet: i64, fiches: &[(String, String, crate::carddav::Contact)]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (href, etag, c) in fiches {
            transaction.execute(
                "INSERT INTO fiches (carnet_id, href, etag, nom, organisation, telephones) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (carnet_id, href) DO UPDATE SET etag = excluded.etag, nom = excluded.nom,
                     organisation = excluded.organisation, telephones = excluded.telephones",
                params![carnet, href, etag, c.nom, c.organisation, c.telephones.join("\n")],
            )?;
            let fiche: i64 = transaction.query_row(
                "SELECT id FROM fiches WHERE carnet_id = ?1 AND href = ?2",
                params![carnet, href],
                |l| l.get(0),
            )?;
            transaction.execute("DELETE FROM fiches_courriels WHERE fiche_id = ?1", params![fiche])?;
            for adresse in &c.courriels {
                transaction.execute(
                    "INSERT INTO fiches_courriels (fiche_id, adresse) VALUES (?1, ?2)",
                    params![fiche, adresse],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Contacts des carnets dont le nom, l'organisation ou une adresse
    /// contient `filtre`, par nom : `(nom, organisation, courriels,
    /// téléphones, carnet, compte)`.
    pub fn fiches_contacts(&self, filtre: &str, limite: usize) -> Resultat<Vec<FicheLocale>> {
        let motif = format!("%{}%", filtre.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut requete = self.base.prepare(
            "SELECT f.id, f.nom, f.organisation, f.telephones, k.nom, a.adresse,
                    (SELECT group_concat(adresse, char(10)) FROM fiches_courriels WHERE fiche_id = f.id)
               FROM fiches f JOIN carnets k ON k.id = f.carnet_id JOIN accounts a ON a.id = k.account_id
              WHERE f.nom LIKE ?1 ESCAPE '\\' OR f.organisation LIKE ?1 ESCAPE '\\'
                 OR EXISTS (SELECT 1 FROM fiches_courriels c WHERE c.fiche_id = f.id AND c.adresse LIKE ?1 ESCAPE '\\')
              ORDER BY f.nom COLLATE NOCASE
              LIMIT ?2",
        )?;
        let lignes = requete.query_map(params![motif, limite as i64], |l| {
            let telephones: String = l.get(3)?;
            let courriels: Option<String> = l.get(6)?;
            Ok(FicheLocale {
                nom: l.get(1)?,
                organisation: l.get(2)?,
                telephones: telephones.split('\n').filter(|t| !t.is_empty()).map(String::from).collect(),
                carnet: l.get(4)?,
                compte: l.get(5)?,
                courriels: courriels.unwrap_or_default().split('\n').filter(|t| !t.is_empty()).map(String::from).collect(),
            })
        })?;
        Ok(lignes.collect::<Result<_, _>>()?)
    }

    pub fn caldav(&self, compte: i64) -> Resultat<(String, i64)> {
        Ok(self
            .base
            .query_row("SELECT caldav, caldav_essai FROM accounts WHERE id = ?1", params![compte], |l| {
                Ok((l.get(0)?, l.get(1)?))
            })
            .optional()?
            .unwrap_or_default())
    }

    pub fn poser_caldav(&self, compte: i64, adresse: &str, essai: i64) -> Resultat<()> {
        self.base.execute(
            "UPDATE accounts SET caldav = ?2, caldav_essai = ?3 WHERE id = ?1",
            params![compte, adresse, essai],
        )?;
        Ok(())
    }

    /// Tous les agendas, dans l'ordre des comptes puis par nom.
    pub fn agendas(&self) -> Resultat<Vec<AgendaLocal>> {
        let mut requete = self.base.prepare(
            "SELECT g.id, g.account_id, g.adresse, g.nom, g.couleur, g.ctag, g.affiche, g.ecriture
             FROM agendas g JOIN accounts a ON a.id = g.account_id
             ORDER BY a.rang IS NULL, a.rang, a.id, g.nom COLLATE NOCASE, g.id",
        )?;
        let agendas = requete
            .query_map([], |l| {
                Ok(AgendaLocal {
                    id: l.get(0)?,
                    compte: l.get(1)?,
                    adresse: l.get(2)?,
                    nom: l.get(3)?,
                    couleur: l.get(4)?,
                    ctag: l.get(5)?,
                    affiche: l.get::<_, i32>(6)? != 0,
                    ecriture: l.get::<_, i32>(7)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(agendas)
    }

    /// Pose les agendas d'un compte tels que le serveur les décrit — adresse,
    /// nom, couleur, droit d'écriture — et retire ceux qu'il ne décrit plus,
    /// objets compris. La case « affiché » des agendas déjà connus ne bouge
    /// pas. Rend vrai si quelque chose a changé.
    pub fn poser_agendas(&self, compte: i64, distants: &[(String, String, String, bool)]) -> Resultat<bool> {
        let avant: Vec<AgendaLocal> = self.agendas()?.into_iter().filter(|a| a.compte == compte).collect();
        let transaction = self.base.unchecked_transaction()?;
        let mut change = false;
        for ancien in &avant {
            if !distants.iter().any(|(adresse, _, _, _)| adresse == &ancien.adresse) {
                transaction.execute("DELETE FROM agendas WHERE id = ?1", params![ancien.id])?;
                change = true;
            }
        }
        for (adresse, nom, couleur, ecriture) in distants {
            match avant.iter().find(|a| &a.adresse == adresse) {
                Some(a) if &a.nom == nom && &a.couleur == couleur && a.ecriture == *ecriture => {}
                Some(a) => {
                    transaction.execute(
                        "UPDATE agendas SET nom = ?2, couleur = ?3, ecriture = ?4 WHERE id = ?1",
                        params![a.id, nom, couleur, *ecriture as i32],
                    )?;
                    change = true;
                }
                None => {
                    transaction.execute(
                        "INSERT INTO agendas (account_id, adresse, nom, couleur, ecriture) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![compte, adresse, nom, couleur, *ecriture as i32],
                    )?;
                    change = true;
                }
            }
        }
        transaction.commit()?;
        Ok(change)
    }

    pub fn poser_ctag(&self, agenda: i64, ctag: &str) -> Resultat<()> {
        self.base.execute("UPDATE agendas SET ctag = ?2 WHERE id = ?1", params![agenda, ctag])?;
        Ok(())
    }

    pub fn afficher_agenda(&self, agenda: i64, affiche: bool) -> Resultat<()> {
        self.base.execute("UPDATE agendas SET affiche = ?2 WHERE id = ?1", params![agenda, affiche as i32])?;
        Ok(())
    }

    /// ETag de chaque objet d'un agenda, par adresse.
    pub fn etags(&self, agenda: i64) -> Resultat<HashMap<String, String>> {
        let mut requete = self.base.prepare("SELECT href, etag FROM evenements WHERE agenda_id = ?1")?;
        let etags = requete
            .query_map(params![agenda], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(etags)
    }

    /// Range des objets d'un agenda, nouveaux ou modifiés.
    pub fn poser_evenements(&self, agenda: i64, objets: &[ObjetAgenda]) -> Resultat<()> {
        for lot in objets.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            for o in lot {
                transaction.execute(
                    "INSERT INTO evenements (agenda_id, href, etag, ical, debut, fin)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (agenda_id, href) DO UPDATE SET
                         etag = excluded.etag, ical = excluded.ical,
                         debut = excluded.debut, fin = excluded.fin",
                    params![agenda, o.href, o.etag, o.ical, o.debut, o.fin],
                )?;
            }
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn retirer_evenements(&self, agenda: i64, hrefs: &[String]) -> Resultat<()> {
        for lot in hrefs.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            for href in lot {
                transaction.execute("DELETE FROM evenements WHERE agenda_id = ?1 AND href = ?2", params![agenda, href])?;
            }
            transaction.commit()?;
        }
        Ok(())
    }

    /// Texte des objets des agendas affichés qui touchent `[de, a)` (secondes
    /// Unix), avec l'identifiant et l'agenda de chacun.
    pub fn evenements_periode(&self, de: i64, a: i64) -> Resultat<Vec<(i64, i64, String)>> {
        let mut requete = self.base.prepare(
            "SELECT e.id, e.agenda_id, e.ical FROM evenements e JOIN agendas g ON g.id = e.agenda_id
             WHERE g.affiche = 1 AND e.debut < ?2 AND e.fin >= ?1",
        )?;
        let objets = requete
            .query_map(params![de, a], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(objets)
    }

    /// Le serveur d'agenda du compte distribue-t-il les invitations ? `None` :
    /// pas encore demandé.
    pub fn planification(&self, compte: i64) -> Resultat<Option<bool>> {
        let valeur: Option<i64> = self
            .base
            .query_row("SELECT caldav_planification FROM accounts WHERE id = ?1", params![compte], |l| l.get(0))
            .optional()?;
        Ok(valeur.filter(|v| *v >= 0).map(|v| v != 0))
    }

    pub fn poser_planification(&self, compte: i64, planification: Option<bool>) -> Resultat<()> {
        let valeur = planification.map_or(-1, |p| p as i64);
        self.base.execute("UPDATE accounts SET caldav_planification = ?2 WHERE id = ?1", params![compte, valeur])?;
        Ok(())
    }

    /// Objet des agendas d'un compte qui porte l'UID `uid`. Le texte est filtré
    /// sur un morceau de l'UID — une ligne longue peut être pliée plus loin —,
    /// puis l'UID de chaque candidat vérifié.
    pub fn objet_par_uid(&self, compte: i64, uid: &str) -> Resultat<Option<ObjetLocal>> {
        if uid.is_empty() {
            return Ok(None);
        }
        let morceau: String = uid.chars().take(40).collect();
        let mut requete = self.base.prepare(
            "SELECT e.id, e.agenda_id, e.href, e.etag, e.ical FROM evenements e JOIN agendas g ON g.id = e.agenda_id
             WHERE g.account_id = ?1 AND instr(e.ical, ?2) > 0",
        )?;
        let candidats = requete
            .query_map(params![compte, morceau], |l| {
                Ok(ObjetLocal { id: l.get(0)?, agenda: l.get(1)?, href: l.get(2)?, etag: l.get(3)?, ical: l.get(4)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(candidats.into_iter().find(|o| crate::invitation::identite(&o.ical).is_some_and(|(u, _)| u == uid)))
    }

    pub fn objet(&self, id: i64) -> Resultat<Option<ObjetLocal>> {
        Ok(self
            .base
            .query_row("SELECT id, agenda_id, href, etag, ical FROM evenements WHERE id = ?1", params![id], |l| {
                Ok(ObjetLocal { id: l.get(0)?, agenda: l.get(1)?, href: l.get(2)?, etag: l.get(3)?, ical: l.get(4)? })
            })
            .optional()?)
    }

    /// État des rappels connus : clé → (vu, repoussé jusqu'à).
    pub fn etats_rappels(&self) -> Resultat<HashMap<String, (bool, i64)>> {
        let mut requete = self.base.prepare("SELECT cle, vu, repousse FROM rappels")?;
        let etats = requete
            .query_map([], |l| Ok((l.get(0)?, (l.get::<_, i32>(1)? != 0, l.get(2)?))))?
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(etats)
    }

    /// Un rappel vu ne revient pas ; un rappel repoussé revient à l'heure dite.
    /// Les états de plus d'un mois sont oubliés au passage.
    pub fn poser_rappel(&self, cle: &str, vu: bool, repousse: i64, maintenant: i64) -> Resultat<()> {
        self.base.execute(
            "INSERT INTO rappels (cle, vu, repousse, pose) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (cle) DO UPDATE SET vu = excluded.vu, repousse = excluded.repousse, pose = excluded.pose",
            params![cle, vu as i32, repousse, maintenant],
        )?;
        self.base.execute("DELETE FROM rappels WHERE pose < ?1", params![maintenant - 31 * 86400])?;
        Ok(())
    }
}

const SELECT_DOSSIER: &str = "SELECT f.id, f.account_id, f.chemin, f.nom, f.separateur, f.profondeur,
        f.role, f.selectionnable, f.nb_messages, f.nb_non_lus, f.uidvalidity, f.uidnext,
        f.highestmodseq, f.hidden_local, f.favorite_rank, f.replie_local
     FROM folders f";

fn lire_dossier(l: &rusqlite::Row<'_>) -> rusqlite::Result<DossierLocal> {
    Ok(DossierLocal {
        id: l.get(0)?,
        compte_id: l.get(1)?,
        chemin: l.get(2)?,
        nom: l.get(3)?,
        separateur: l.get(4)?,
        profondeur: l.get(5)?,
        role: l.get(6)?,
        selectionnable: l.get::<_, i32>(7)? != 0,
        messages: l.get(8)?,
        non_lus: l.get(9)?,
        uid_validity: l.get(10)?,
        uid_next: l.get(11)?,
        highest_mod_seq: l.get(12)?,
        masque: l.get::<_, i32>(13)? != 0,
        favori: l.get(14)?,
        replie: l.get::<_, i32>(15)? != 0,
    })
}

/// Un message avec son dossier : résultats d'une recherche dans toutes les
/// boîtes, boîte de réception unifiée.
const SELECT_TROUVE: &str = "SELECT m.uid, m.message_id, m.expediteur, m.adresse, m.sujet, m.date, m.horodatage,
        m.taille, m.lu, m.repondu, m.pieces, m.suivi, m.importance, m.transfere, m.repondu_tous,
        m.destinataires, f.account_id, f.chemin, f.nom, f.separateur, f.role
     FROM messages m JOIN folders f ON f.id = m.folder_id";

fn lire_trouve(l: &rusqlite::Row<'_>) -> rusqlite::Result<Trouve> {
    Ok(Trouve {
        message: lire_message(l)?,
        compte: l.get(16)?,
        chemin: l.get(17)?,
        nom_dossier: l.get(18)?,
        separateur: l.get(19)?,
        role: l.get(20)?,
    })
}

const SELECT_MESSAGE: &str = "SELECT uid, message_id, expediteur, adresse, sujet, date,
        horodatage, taille, lu, repondu, pieces, suivi, importance, transfere, repondu_tous,
        destinataires
     FROM messages";

fn lire_message(l: &rusqlite::Row<'_>) -> rusqlite::Result<MessageLocal> {
    Ok(MessageLocal {
        uid: l.get(0)?,
        message_id: l.get(1)?,
        expediteur: l.get(2)?,
        adresse: l.get(3)?,
        sujet: l.get(4)?,
        date: l.get(5)?,
        horodatage: l.get(6)?,
        taille: l.get(7)?,
        lu: l.get::<_, i32>(8)? != 0,
        repondu: l.get::<_, i32>(9)? != 0,
        pieces: l.get::<_, i32>(10)? != 0,
        suivi: l.get::<_, i32>(11)? != 0,
        importance: l.get::<_, i32>(12)?.clamp(-1, 1) as i8,
        transfere: l.get::<_, i32>(13)? != 0,
        repondu_tous: l.get::<_, i32>(14)? != 0,
        destinataires: l.get(15)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(chemin: &str, attributs: &[&str]) -> Dossier {
        Dossier {
            chemin: chemin.to_string(),
            separateur: "/".to_string(),
            attributs: attributs.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn message(uid: u32, horodatage: i64, lu: bool) -> MessageLocal {
        MessageLocal { uid, horodatage, lu, sujet: format!("m{uid}"), ..Default::default() }
    }

    #[test]
    fn version_du_dossier_a_chaque_ecriture_de_ses_messages() {
        let (m, compte, id) = boite();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Autre", &[])]).unwrap();
        let autre = m.dossier_id(compte, "Autre").unwrap();
        assert_eq!(m.version_dossier(id).unwrap(), 0);
        let msg = |uid: u32| MessageLocal { uid, horodatage: uid as i64, ..Default::default() };
        m.poser_messages(id, &[msg(1), msg(2)]).unwrap();
        let v1 = m.version_dossier(id).unwrap();
        assert!(v1 > 0);
        // Les drapeaux tels quels : rien n'est écrit, la version ne bouge pas.
        assert_eq!(m.poser_drapeaux(id, &[(1, vec![]), (2, vec![])]).unwrap(), 0);
        assert_eq!(m.version_dossier(id).unwrap(), v1);
        assert_eq!(m.poser_drapeaux(id, &[(1, vec!["\\Seen".into()]), (2, vec![])]).unwrap(), 1);
        let v2 = m.version_dossier(id).unwrap();
        assert!(v2 > v1);
        m.retirer(id, &[2]).unwrap();
        assert!(m.version_dossier(id).unwrap() > v2);
        // L'autre dossier n'a pas bougé.
        assert_eq!(m.version_dossier(autre).unwrap(), 0);
        // Une seconde connexion voit les écritures de la première (index sur
        // disque : la version vit dans l'index, pas dans la connexion).
        let dir = std::env::temp_dir().join(format!("mmail-version-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let chemin = dir.join("index.sqlite");
        let a = Magasin::ouvrir(chemin.to_str().unwrap()).unwrap();
        let b = Magasin::ouvrir(chemin.to_str().unwrap()).unwrap();
        let c = a.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        a.poser_dossiers(c, &[dossier("INBOX", &[])]).unwrap();
        let d = a.dossier_id(c, "INBOX").unwrap();
        let avant = b.version_dossier(d).unwrap();
        a.poser_messages(d, &[msg(7)]).unwrap();
        assert!(b.version_dossier(d).unwrap() > avant);
        drop((a, b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn boite() -> (Magasin, i64, i64) {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        (m, compte, id)
    }

    #[test]
    fn messages_gardes_sur_le_poste() {
        let m = Magasin::en_memoire().unwrap();
        let c = m.compte("a@exemple.fr", "imap.exemple.fr", 993, "a").unwrap();
        m.poser_dossiers(c, &[Dossier { chemin: "INBOX".into(), separateur: "/".into(), attributs: vec![] }])
            .unwrap();
        let d = m.dossier_id(c, "INBOX").unwrap();
        let msg = |uid: u32, horodatage: i64| MessageLocal { uid, horodatage, ..Default::default() };
        m.poser_messages(d, &[msg(1, 100), msg(2, 5_000), msg(3, 9_000)]).unwrap();
        let ids: Vec<i64> = m.a_garder(d, 1_000, 10, i64::MAX).unwrap().into_iter().map(|(id, _)| id).collect();
        let uids: Vec<u32> = m.a_garder(d, 1_000, 10, i64::MAX).unwrap().into_iter().map(|(_, u)| u).collect();
        // Les plus récents d'abord, rien d'avant la fenêtre.
        assert_eq!(uids, vec![3, 2]);
        m.poser_etat_corps(ids[0], "full").unwrap();
        m.poser_etat_corps(ids[1], "echec").unwrap();
        assert!(m.a_garder(d, 1_000, 10, i64::MAX).unwrap().is_empty());
        assert_eq!(m.gardes().unwrap(), HashSet::from([ids[0]]));
        // Une synchronisation ne remet pas l'état à zéro.
        m.poser_messages(d, &[msg(3, 9_000)]).unwrap();
        assert_eq!(m.gardes().unwrap(), HashSet::from([ids[0]]));
        assert_eq!(m.identite_message(d, 3).unwrap().map(|(id, h, _)| (id, h)), Some((ids[0], 9_000)));
        // Au-delà du plafond de taille, un message n'est pas gardé.
        let gros = MessageLocal { uid: 4, horodatage: 9_500, taille: 30_000_000, ..Default::default() };
        m.poser_messages(d, &[gros, MessageLocal { uid: 5, horodatage: 9_400, taille: 2_000, ..Default::default() }])
            .unwrap();
        let uids: Vec<u32> = m.a_garder(d, 1_000, 10, 10 * 1024 * 1024).unwrap().into_iter().map(|(_, u)| u).collect();
        assert_eq!(uids, vec![5]);
        assert!(m.gardes_perimes(9_000).unwrap().is_empty());
        assert_eq!(m.gardes_perimes(9_001).unwrap(), vec![ids[0]]);
    }

    #[test]
    fn liste_par_pages() {
        let (m, _, d) = boite();
        // Deux messages à la même date : l'UID les départage.
        m.poser_messages(d, &[message(1, 100, true), message(2, 300, true), message(3, 300, true), message(4, 200, true), message(5, 50, true)])
            .unwrap();
        let uids = |l: Vec<MessageLocal>| l.into_iter().map(|x| x.uid).collect::<Vec<_>>();
        assert_eq!(uids(m.messages(d).unwrap()), vec![3, 2, 4, 1, 5]);
        assert_eq!(uids(m.messages_page(d, None, 2).unwrap()), vec![3, 2]);
        assert_eq!(uids(m.messages_page(d, Some((300, 2)), 2).unwrap()), vec![4, 1]);
        assert_eq!(uids(m.messages_page(d, Some((100, 1)), 2).unwrap()), vec![5]);
        assert!(m.messages_page(d, Some((50, 5)), 2).unwrap().is_empty());
        // Jusqu'à la borne comprise : ce qui a déjà été envoyé.
        assert_eq!(uids(m.messages_jusqua(d, Some((300, 2))).unwrap()), vec![3, 2]);
        assert_eq!(uids(m.messages_jusqua(d, Some((200, 4))).unwrap()), vec![3, 2, 4]);
        assert_eq!(uids(m.messages_jusqua(d, None).unwrap()), vec![3, 2, 4, 1, 5]);
    }

    #[test]
    fn ordre_des_comptes() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@exemple.fr", "imap.exemple.fr", 993, "a").unwrap();
        let b = m.compte("b@exemple.fr", "imap.exemple.fr", 993, "b").unwrap();
        let c = m.compte("c@exemple.fr", "imap.exemple.fr", 993, "c").unwrap();
        let ordre = |m: &Magasin| m.comptes().unwrap().into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ordre(&m), vec![a, b, c]);
        m.placer_compte(c, Some(a)).unwrap();
        assert_eq!(ordre(&m), vec![c, a, b]);
        m.placer_compte(c, None).unwrap();
        assert_eq!(ordre(&m), vec![a, b, c]);
        m.decaler_compte(b, -1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        m.decaler_compte(b, -1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        m.decaler_compte(c, 1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        // Un compte réenregistré garde sa place ; un nouveau va à la fin.
        m.compte("b@exemple.fr", "autre.exemple.fr", 993, "b").unwrap();
        let d = m.compte("d@exemple.fr", "imap.exemple.fr", 993, "d").unwrap();
        assert_eq!(ordre(&m), vec![b, a, c, d]);
    }

    #[test]
    fn comptes_et_arborescence() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("essai@exemple.fr", "imap.exemple.fr", 993, "essai@exemple.fr").unwrap();
        assert!(compte > 0);
        // Deux fois le même compte ne fait pas deux lignes.
        let encore = m.compte("essai@exemple.fr", "imap.exemple.fr", 993, "essai@exemple.fr").unwrap();
        assert_eq!(compte, encore);
        assert_eq!(m.comptes().unwrap().len(), 1);

        m.poser_dossiers(
            compte,
            &[
                dossier("INBOX", &[]),
                dossier("Sent", &["\\Sent"]),
                dossier("Essais/Factures", &[]),
                dossier("Parent", &["\\Noselect"]),
            ],
        )
        .unwrap();
        let dossiers = m.dossiers(compte).unwrap();
        assert_eq!(dossiers.len(), 4);
        // INBOX en tête, puis le dossier à rôle.
        assert_eq!(dossiers[0].chemin, "INBOX");
        assert_eq!(dossiers[1].role, "Sent");
        assert_eq!(dossiers[2].nom, "Factures");
        assert_eq!(dossiers[2].profondeur, 1);
        assert!(!dossiers[3].selectionnable);
    }

    #[test]
    fn dossier_disparu_du_serveur_est_retire() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Vieux", &[])]).unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let dossiers = m.dossiers(compte).unwrap();
        assert_eq!(dossiers.len(), 1);
        assert_eq!(dossiers[0].chemin, "INBOX");
    }

    #[test]
    fn etat_local_survit_a_la_relecture_de_l_arborescence() {
        let (m, compte, id) = boite();
        m.masquer(id, true).unwrap();
        m.epingler(id, true).unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Autre", &[])]).unwrap();
        let inbox = m.dossier(id).unwrap().unwrap();
        assert!(inbox.masque);
        assert_eq!(inbox.favori, Some(1));
        assert_eq!(m.favoris().unwrap().len(), 1);
        m.epingler(id, false).unwrap();
        assert!(m.favoris().unwrap().is_empty());
    }

    #[test]
    fn favoris_places_et_deplaces() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("A", &[]), dossier("B", &[]), dossier("C", &[])]).unwrap();
        let id = |c: &str| m.dossier_id(compte, c).unwrap();
        let ordre = || m.favoris().unwrap().iter().map(|d| d.chemin.clone()).collect::<Vec<_>>();

        m.placer_favori(id("A"), None).unwrap();
        m.placer_favori(id("B"), None).unwrap();
        assert_eq!(ordre(), vec!["A", "B"]);
        // Déposé sur « A » : se glisse avant lui.
        m.placer_favori(id("C"), Some(id("A"))).unwrap();
        assert_eq!(ordre(), vec!["C", "A", "B"]);
        // Un favori déplacé ne se dédouble pas.
        m.placer_favori(id("B"), Some(id("C"))).unwrap();
        assert_eq!(ordre(), vec!["B", "C", "A"]);
        // Déposé sur lui-même ou sur la rubrique : à la fin.
        m.placer_favori(id("B"), Some(id("B"))).unwrap();
        assert_eq!(ordre(), vec!["C", "A", "B"]);
        // « avant » qui n'est pas un favori : à la fin aussi.
        m.epingler(id("A"), false).unwrap();
        m.placer_favori(id("C"), Some(id("A"))).unwrap();
        assert_eq!(ordre(), vec!["B", "C"]);
    }

    #[test]
    fn messages_tries_par_date_de_reception() {
        let (m, _, id) = boite();
        // Un message déplacé depuis une autre boîte reçoit un grand UID mais
        // garde sa date : il doit rester à sa place dans la liste.
        m.poser_messages(id, &[message(1, 100, true), message(2, 300, false), message(50, 200, false)])
            .unwrap();
        let uids: Vec<u32> = m.messages(id).unwrap().iter().map(|m| m.uid).collect();
        assert_eq!(uids, vec![2, 50, 1]);
    }

    #[test]
    fn compteurs_du_serveur_puis_de_l_index() {
        let (m, compte, id) = boite();
        m.poser_compteurs(compte, &[Statut { chemin: "INBOX".into(), messages: 10, non_lus: 4 }])
            .unwrap();
        let d = m.dossier(id).unwrap().unwrap();
        assert_eq!((d.messages, d.non_lus), (10, 4));

        m.poser_messages(id, &[message(1, 1, true), message(2, 2, false)]).unwrap();
        m.recompter(id).unwrap();
        let d = m.dossier(id).unwrap().unwrap();
        assert_eq!((d.messages, d.non_lus), (2, 1));

        m.marquer_lu(id, &[2], true).unwrap();
        assert_eq!(m.dossier(id).unwrap().unwrap().non_lus, 0);
    }

    #[test]
    fn changement_d_uidvalidity_vide_le_dossier() {
        let (m, _, id) = boite();
        let etat = EtatDossier { messages: 1, uid_validity: 100, uid_next: 2, highest_mod_seq: 5 };
        assert!(!m.poser_etat(id, &etat).unwrap(), "premier état : pas de rupture");
        m.poser_messages(id, &[MessageLocal { uid: 1, ..Default::default() }]).unwrap();

        // Même UIDVALIDITY : l'index reste.
        assert!(!m.poser_etat(id, &etat).unwrap());
        assert_eq!(m.messages(id).unwrap().len(), 1);

        // UIDVALIDITY changée : les UID d'avant ne désignent plus rien.
        let apres = EtatDossier { uid_validity: 101, ..etat.clone() };
        assert!(m.poser_etat(id, &apres).unwrap(), "rupture attendue");
        assert!(m.messages(id).unwrap().is_empty());
    }

    #[test]
    fn relire_un_message_met_a_jour_ses_drapeaux() {
        let (m, _, id) = boite();
        m.poser_messages(id, &[MessageLocal { uid: 1, sujet: "Objet".into(), lu: false, ..Default::default() }]).unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, sujet: "Objet".into(), lu: true, ..Default::default() }]).unwrap();
        let messages = m.messages(id).unwrap();
        assert_eq!(messages.len(), 1, "le même UID ne doit pas se dédoubler");
        assert!(messages[0].lu);
    }

    #[test]
    fn vanished_et_drapeaux_appliques() {
        let (m, _, id) = boite();
        m.poser_messages(id, &(1..=6).map(|u| message(u, u as i64, false)).collect::<Vec<_>>())
            .unwrap();
        m.retirer_intervalles(id, &[(2, 3), (6, 100)]).unwrap();
        let mut uids = m.uids(id).unwrap();
        uids.sort();
        assert_eq!(uids, vec![1, 4, 5]);
        m.poser_drapeaux(id, &[(4, vec!["\\Seen".into(), "\\Answered".into()])]).unwrap();
        let m4 = m.message(id, 4).unwrap().unwrap();
        assert!(m4.lu && m4.repondu);
        m.retirer(id, &[1]).unwrap();
        assert!(m.message(id, 1).unwrap().is_none());
    }

    #[test]
    fn file_des_deplacements() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        let b = m.compte("c@d.fr", "h", 993, "c@d.fr").unwrap();
        let op = Operation {
            id: 0,
            compte_source: a,
            chemin_source: "INBOX".into(),
            validite_source: 7,
            uid_source: 3,
            compte_cible: b,
            chemin_cible: "Archives".into(),
            message_id: "<x@y>".into(),
            drapeaux: "\\Seen".into(),
            date_interne: "16-Sep-2026 18:00:00 +0200".into(),
            fichier: "/tmp/x.eml".into(),
            etape: Etape::Deposer,
            erreur: String::new(),
        };
        let id = m.inscrire_operation(&op).unwrap();
        assert_eq!(m.nombre_operations().unwrap(), 1);
        m.noter_echec_operation(id, "coupure").unwrap();
        m.avancer_operation(id, Etape::Purger).unwrap();
        let ops = m.operations(a).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].etape, Etape::Purger);
        assert!(ops[0].erreur.is_empty(), "une étape franchie efface l'échec précédent");
        assert!(m.operations(b).unwrap().is_empty());
        m.clore_operation(id).unwrap();
        assert_eq!(m.nombre_operations().unwrap(), 0);
    }

    #[test]
    fn retirer_un_compte_emporte_son_index() {
        let (m, compte, id) = boite();
        m.poser_messages(id, &[message(1, 1, true)]).unwrap();
        m.retirer_compte(compte).unwrap();
        assert!(m.comptes().unwrap().is_empty());
        assert!(m.dossier(id).unwrap().is_none());
    }

    #[test]
    fn migration_d_un_index_de_version_1() {
        // Un index de première génération : sans les colonnes de la version 2.
        let base = Connection::open_in_memory().unwrap();
        base.execute_batch(
            "CREATE TABLE accounts (id INTEGER PRIMARY KEY, adresse TEXT NOT NULL UNIQUE,
                hote TEXT NOT NULL, port INTEGER NOT NULL DEFAULT 993, utilisateur TEXT NOT NULL);
             CREATE TABLE folders (id INTEGER PRIMARY KEY, account_id INTEGER NOT NULL,
                chemin TEXT NOT NULL, nom TEXT NOT NULL, profondeur INTEGER NOT NULL DEFAULT 0,
                role TEXT NOT NULL DEFAULT '', uidvalidity INTEGER NOT NULL DEFAULT 0,
                uidnext INTEGER NOT NULL DEFAULT 0, highestmodseq INTEGER NOT NULL DEFAULT 0,
                hidden_local INTEGER NOT NULL DEFAULT 0, favorite_rank INTEGER,
                UNIQUE (account_id, chemin));
             CREATE TABLE messages (id INTEGER PRIMARY KEY, folder_id INTEGER NOT NULL,
                uid INTEGER NOT NULL, message_id TEXT NOT NULL DEFAULT '',
                expediteur TEXT NOT NULL DEFAULT '', adresse TEXT NOT NULL DEFAULT '',
                sujet TEXT NOT NULL DEFAULT '', date TEXT NOT NULL DEFAULT '',
                taille INTEGER NOT NULL DEFAULT 0, lu INTEGER NOT NULL DEFAULT 0,
                repondu INTEGER NOT NULL DEFAULT 0, body_state TEXT NOT NULL DEFAULT 'headers',
                UNIQUE (folder_id, uid));
             INSERT INTO accounts (adresse, hote, utilisateur) VALUES ('a@b.fr', 'h', 'a@b.fr');
             INSERT INTO folders (account_id, chemin, nom) VALUES (1, 'INBOX', 'INBOX');
             INSERT INTO messages (folder_id, uid, sujet) VALUES (1, 1, 'ancien');",
        )
        .unwrap();
        let m = Magasin::preparer(base).unwrap();
        let messages = m.messages(1).unwrap();
        assert_eq!(messages.len(), 1, "l'index existant est conservé");
        assert_eq!(messages[0].sujet, "ancien");
        assert!(!messages[0].pieces);
        assert!(m.dossier(1).unwrap().unwrap().selectionnable);
    }

    #[test]
    fn migration_version_3_fait_relire_les_en_tetes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_etat(id, &EtatDossier { uid_validity: 42, ..Default::default() }).unwrap();
        // Retour à un index de version 2, sans les colonnes de pièces jointes.
        m.base
            .execute_batch(
                "ALTER TABLE messages DROP COLUMN pieces;
                 ALTER TABLE messages DROP COLUMN pieces_certain;
                 ALTER TABLE messages DROP COLUMN suivi;
                 ALTER TABLE messages DROP COLUMN importance;
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        m.migrer().unwrap();
        assert_eq!(m.dossier(id).unwrap().unwrap().uid_validity, 0, "dossier à relire");
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        assert!(m.messages(id).unwrap()[0].pieces);
    }

    #[test]
    fn suivi_et_importance_indexes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, importance: 1, ..Default::default() }]).unwrap();
        assert_eq!(m.messages(id).unwrap()[0].importance, 1);
        m.marquer_suivi(id, &[1], true).unwrap();
        assert!(m.messages(id).unwrap()[0].suivi);
        // Les drapeaux relus du serveur font foi.
        m.poser_drapeaux(id, &[(1, vec!["\\Seen".into()])]).unwrap();
        assert!(!m.messages(id).unwrap()[0].suivi);
        m.poser_drapeaux(id, &[(1, vec!["\\Flagged".into()])]).unwrap();
        assert!(m.messages(id).unwrap()[0].suivi);
    }

    #[test]
    fn repondu_et_transfere_poses_par_un_autre_logiciel() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_messages(id, &[1, 2].map(|uid| MessageLocal { uid, sujet: format!("m{uid}"), ..Default::default() }))
            .unwrap();
        // Posés sur le téléphone : relus du serveur, sans égard à la casse.
        m.poser_drapeaux(id, &[(1, vec!["\\Seen".into(), "$Forwarded".into()]), (2, vec!["$answered".into()])])
            .unwrap();
        let (m1, m2) = (m.message(id, 1).unwrap().unwrap(), m.message(id, 2).unwrap().unwrap());
        assert!(m1.transfere && !m1.repondu);
        assert!(m2.repondu && !m2.transfere);
        // La recherche les rend aussi.
        assert!(m.chercher(Some(id), "m1", 10).unwrap()[0].message.transfere);
        // Retirés ailleurs, ils s'effacent ici.
        m.poser_drapeaux(id, &[(1, vec![])]).unwrap();
        assert!(!m.message(id, 1).unwrap().unwrap().transfere);
        // Réponse à tous depuis MMail : `$ReplyAll` en plus de `\Answered`.
        m.poser_drapeaux(id, &[(1, vec!["\\Answered".into(), "$ReplyAll".into()])]).unwrap();
        let m1 = m.message(id, 1).unwrap().unwrap();
        assert!(m1.repondu && m1.repondu_tous);
        // Un message relu en entier garde ce que portent ses en-têtes.
        m.poser_messages(id, &[MessageLocal { uid: 2, transfere: true, ..Default::default() }]).unwrap();
        assert!(m.message(id, 2).unwrap().unwrap().transfere);
    }

    #[test]
    fn migration_version_10_fait_relire_les_drapeaux() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_etat(id, &EtatDossier { messages: 1, uid_validity: 42, uid_next: 2, highest_mod_seq: 99 }).unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, sujet: "ancien".into(), ..Default::default() }]).unwrap();
        // Retour à un index de version 9, sans la colonne « transfere ».
        m.base
            .execute_batch(
                "ALTER TABLE messages DROP COLUMN transfere; ALTER TABLE messages DROP COLUMN repondu_tous;
                 PRAGMA user_version = 9;",
            )
            .unwrap();
        m.migrer().unwrap();
        let d = m.dossier(id).unwrap().unwrap();
        assert_eq!(d.highest_mod_seq, 0, "drapeaux à relire");
        assert_eq!(d.uid_validity, 42, "en-têtes gardés");
        assert_eq!(m.messages(id).unwrap()[0].sujet, "ancien");
        m.poser_drapeaux(id, &[(1, vec!["$Forwarded".into()])]).unwrap();
        assert!(m.message(id, 1).unwrap().unwrap().transfere);
    }

    #[test]
    fn migration_version_11_sans_relecture() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_etat(id, &EtatDossier { messages: 1, uid_validity: 42, uid_next: 2, highest_mod_seq: 99 }).unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, transfere: true, ..Default::default() }]).unwrap();
        // Retour à un index de version 10 : la colonne « repondu_tous » manque.
        m.base.execute_batch("ALTER TABLE messages DROP COLUMN repondu_tous; PRAGMA user_version = 10;").unwrap();
        m.migrer().unwrap();
        assert_eq!(m.dossier(id).unwrap().unwrap().highest_mod_seq, 99, "rien à relire");
        let m1 = m.message(id, 1).unwrap().unwrap();
        assert!(m1.transfere && !m1.repondu_tous);
    }

    #[test]
    fn serveur_d_envoi_distinct() {
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@orange.fr", "imap.orange.fr", 993, "a@orange.fr").unwrap();
        let c = &m.comptes().unwrap()[0];
        assert_eq!(c.hote_envoi, "");
        assert_eq!(c.serveur_envoi(), "imap.orange.fr", "vide : le serveur de lecture");
        assert!(m.poser_serveur_envoi("a@orange.fr", " SMTP.Orange.fr ").unwrap());
        let c = &m.comptes().unwrap()[0];
        assert_eq!(c.serveur_envoi(), "smtp.orange.fr");
        // Une reconnexion réécrit le serveur de lecture, pas celui de l'envoi.
        m.compte("a@orange.fr", "imap.orange.fr", 993, "a@orange.fr").unwrap();
        assert_eq!(m.comptes().unwrap()[0].hote_envoi, "smtp.orange.fr");
        assert!(m.poser_serveur_envoi("a@orange.fr", "").unwrap());
        assert_eq!(m.comptes().unwrap()[0].serveur_envoi(), "imap.orange.fr");
        assert!(!m.poser_serveur_envoi("inconnu@b.fr", "h").unwrap(), "rien n'est créé");
    }

    #[test]
    fn migration_version_12_serveur_d_envoi_vide() {
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.base.execute_batch("ALTER TABLE accounts DROP COLUMN hote_envoi; PRAGMA user_version = 11;").unwrap();
        m.migrer().unwrap();
        let c = &m.comptes().unwrap()[0];
        assert_eq!((c.hote_envoi.as_str(), c.serveur_envoi()), ("", "h"));
    }

    #[test]
    fn migration_version_13_authentification() {
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.base.execute_batch("ALTER TABLE accounts DROP COLUMN auth; PRAGMA user_version = 12;").unwrap();
        m.migrer().unwrap();
        let c = &m.comptes().unwrap()[0];
        assert_eq!(c.auth, "", "mot de passe pour les comptes déjà là");
        assert!(!c.microsoft());
    }

    #[test]
    fn adresses_partagees_d_un_compte() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        let b = m.compte("b@b.fr", "h", 993, "b@b.fr").unwrap();
        assert!(m.comptes().unwrap()[0].partagees.is_empty());
        let adresses = vec!["commandes@b.fr".to_string(), "contact@b.fr".to_string()];
        assert!(m.poser_partagees(a, &adresses).unwrap());
        assert!(!m.poser_partagees(a, &adresses).unwrap(), "inchangées");
        let comptes = m.comptes().unwrap();
        assert_eq!(comptes.iter().find(|c| c.id == a).unwrap().partagees, adresses);
        assert!(comptes.iter().find(|c| c.id == b).unwrap().partagees.is_empty());
        assert!(m.poser_partagees(a, &[]).unwrap(), "partage retiré");
        assert!(m.compte_par_id(a).unwrap().unwrap().partagees.is_empty());
    }

    #[test]
    fn migration_version_17_partagees_vides() {
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.base.execute_batch("ALTER TABLE accounts DROP COLUMN partagees; PRAGMA user_version = 16;").unwrap();
        m.migrer().unwrap();
        assert!(m.comptes().unwrap()[0].partagees.is_empty());
    }

    #[test]
    fn migration_version_16_fait_relire_les_envoyes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        // Un dossier d'envoyés désigné par SPECIAL-USE, un autre par son seul
        // nom (OVH), et une boîte de réception qui n'a rien à relire.
        m.poser_dossiers(
            compte,
            &[dossier("INBOX", &[]), dossier("Copies", &["\\Sent"]), dossier("Éléments envoyés", &[])],
        )
        .unwrap();
        let ids: Vec<i64> =
            ["INBOX", "Copies", "Éléments envoyés"].iter().map(|c| m.dossier_id(compte, c).unwrap()).collect();
        for &id in &ids {
            m.poser_etat(id, &EtatDossier { uid_validity: 42, ..Default::default() }).unwrap();
            m.poser_messages(id, &[MessageLocal { uid: 1, ..Default::default() }]).unwrap();
        }
        // Retour à un index de version 15, sans les destinataires.
        m.base.execute_batch("ALTER TABLE messages DROP COLUMN destinataires; PRAGMA user_version = 15;").unwrap();
        m.migrer().unwrap();
        let validites: Vec<u32> = ids.iter().map(|&id| m.dossier(id).unwrap().unwrap().uid_validity).collect();
        assert_eq!(validites, vec![42, 0, 0], "seuls les envoyés sont relus");
        // La relecture des en-têtes complète un message déjà connu.
        m.poser_messages(ids[1], &[MessageLocal { uid: 1, destinataires: "Jean".into(), ..Default::default() }])
            .unwrap();
        assert_eq!(m.message(ids[1], 1).unwrap().unwrap().destinataires, "Jean");
    }

    #[test]
    fn compte_microsoft() {
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@contoso.fr", crate::microsoft::HOTE_IMAP, 993, "a@contoso.fr").unwrap();
        assert!(m.poser_auth("a@contoso.fr", AUTH_MICROSOFT).unwrap());
        assert!(m.comptes().unwrap()[0].microsoft());
        // Une reconnexion réécrit le serveur, pas le mode d'authentification.
        m.compte("a@contoso.fr", crate::microsoft::HOTE_IMAP, 993, "a@contoso.fr").unwrap();
        assert!(m.comptes().unwrap()[0].microsoft());
        assert!(!m.poser_auth("inconnu@b.fr", AUTH_MICROSOFT).unwrap(), "rien n'est créé");
    }

    #[test]
    fn compte_google() {
        use crate::oauth::Fournisseur;
        let m = Magasin::en_memoire().unwrap();
        m.compte("a@gmail.com", crate::google::HOTE_IMAP, 993, "a@gmail.com").unwrap();
        m.compte("b@b.fr", "h", 993, "b@b.fr").unwrap();
        assert!(m.poser_auth("a@gmail.com", AUTH_GOOGLE).unwrap());
        let comptes = m.comptes().unwrap();
        assert!(comptes[0].oauth() && !comptes[0].microsoft());
        assert_eq!(comptes[0].fournisseur(), Some(Fournisseur::Google));
        assert!(!comptes[1].oauth() && comptes[1].fournisseur().is_none());
    }

    #[test]
    fn boites_de_reception_de_tous_les_comptes() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        let b = m.compte("c@d.fr", "h", 993, "c@d.fr").unwrap();
        m.poser_dossiers(a, &[dossier("INBOX", &[]), dossier("Archives", &[])]).unwrap();
        m.poser_dossiers(b, &[dossier("INBOX", &[])]).unwrap();
        let ligne = |uid: u32, h: i64| MessageLocal { uid, horodatage: h, sujet: format!("m{uid}"), ..Default::default() };
        m.poser_messages(m.dossier_id(a, "INBOX").unwrap(), &[ligne(1, 10), ligne(2, 30)]).unwrap();
        m.poser_messages(m.dossier_id(a, "Archives").unwrap(), &[ligne(3, 40)]).unwrap();
        m.poser_messages(m.dossier_id(b, "INBOX").unwrap(), &[ligne(1, 20)]).unwrap();
        let toutes = m.boites_de_reception(10).unwrap();
        // Les deux réceptions, pas les archives ; du plus récent au plus ancien.
        let vu: Vec<(i64, u32)> = toutes.iter().map(|t| (t.compte, t.message.uid)).collect();
        assert_eq!(vu, vec![(a, 2), (b, 1), (a, 1)]);
        assert!(toutes.iter().all(|t| t.chemin == "INBOX"));
        assert_eq!(m.boites_de_reception(2).unwrap().len(), 2);
        // Non-lus des réceptions, tous comptes : pas ceux des archives.
        m.recompter(m.dossier_id(a, "INBOX").unwrap()).unwrap();
        m.recompter(m.dossier_id(a, "Archives").unwrap()).unwrap();
        m.recompter(m.dossier_id(b, "INBOX").unwrap()).unwrap();
        assert_eq!(m.non_lus_receptions().unwrap(), 3);
    }

    #[test]
    fn contacts_proposes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        let recu = |uid, nom: &str, adresse: &str| MessageLocal {
            uid,
            expediteur: nom.into(),
            adresse: adresse.into(),
            ..Default::default()
        };
        m.poser_messages(
            id,
            &[
                recu(1, "Hélène Martin", "helene@exemple.fr"),
                recu(2, "Hélène Martin", "HELENE@exemple.fr"),
                recu(3, "Noël Durand", "noel@exemple.fr"),
                recu(4, "Promo", "promo_50%@exemple.fr"),
            ],
        )
        .unwrap();
        m.noter_correspondants(&[("".into(), "ecrit@ailleurs.fr".into())], 10).unwrap();
        m.noter_correspondants(&[("Écrit Ailleurs".into(), "ecrit@ailleurs.fr".into())], 20).unwrap();
        m.noter_correspondants(&[("".into(), "ecrit@ailleurs.fr".into())], 30).unwrap();

        let tous = m.contacts("", 10).unwrap();
        assert_eq!(tous[0], ("Écrit Ailleurs".into(), "ecrit@ailleurs.fr".into()), "à qui l'on écrit d'abord, nom gardé");
        assert_eq!(tous.len(), 4, "une adresse, une ligne, quelle que soit la casse");
        assert_eq!(m.contacts("hél", 10).unwrap()[0].1.to_lowercase(), "helene@exemple.fr");
        assert_eq!(m.contacts("DURAND", 10).unwrap(), vec![("Noël Durand".into(), "noel@exemple.fr".into())]);
        assert_eq!(m.contacts("50%", 10).unwrap().len(), 1, "% cherché tel quel");
        assert!(m.contacts("zzz", 10).unwrap().is_empty());
    }

    #[test]
    fn expediteurs_comptes_une_fois_par_message() {
        let (m, _, id) = boite();
        let recu = |uid, nom: &str, adresse: &str| MessageLocal {
            uid,
            expediteur: nom.into(),
            adresse: adresse.into(),
            horodatage: uid as i64,
            ..Default::default()
        };
        let lot = [recu(1, "Rare", "rare@exemple.fr"), recu(2, "Fréquent", "f@exemple.fr"), recu(3, "Fréquent", "F@exemple.fr")];
        m.poser_messages(id, &lot).unwrap();
        // Relu (resynchronisation) : rien ne se compte deux fois.
        m.poser_messages(id, &lot).unwrap();
        m.poser_messages(id, &[recu(4, "Fréquent", "f@exemple.fr")]).unwrap();
        let nb: i64 = m.base.query_row("SELECT nb FROM expediteurs WHERE adresse = 'f@exemple.fr'", [], |l| l.get(0)).unwrap();
        assert_eq!(nb, 3);
        assert_eq!(m.contacts("exemple", 10).unwrap()[0], ("Fréquent".into(), "f@exemple.fr".into()));
    }

    #[test]
    fn ecriture_par_lots() {
        // Plus d'un lot : tout est écrit, et les mises à jour suivent.
        let (m, _, id) = boite();
        let lignes: Vec<MessageLocal> = (1..=1203).map(|uid| message(uid, uid as i64, false)).collect();
        m.poser_messages(id, &lignes).unwrap();
        assert_eq!(m.uids(id).unwrap().len(), 1203);
        let drapeaux: Vec<(u32, Vec<String>)> = (1..=1203).map(|uid| (uid, vec!["\\Seen".to_string()])).collect();
        m.poser_drapeaux(id, &drapeaux).unwrap();
        assert!(m.messages(id).unwrap().iter().all(|l| l.lu));
    }

    #[test]
    fn recherche_dans_l_index() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@exemple.fr", "h", 993, "a@exemple.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Archives", &[])]).unwrap();
        let inbox = m.dossier_id(compte, "INBOX").unwrap();
        let archives = m.dossier_id(compte, "Archives").unwrap();
        let recu = |uid, h, nom: &str, sujet: &str| MessageLocal {
            uid,
            horodatage: h,
            expediteur: nom.into(),
            adresse: format!("{}@exemple.fr", nom.to_lowercase()),
            sujet: sujet.into(),
            ..Default::default()
        };
        m.poser_messages(inbox, &[recu(1, 10, "Martin", "Facture de mars"), recu(2, 20, "Durand", "Réunion"), recu(3, 30, "Promo", "Remise 50%_ici")])
            .unwrap();
        m.poser_messages(archives, &[recu(1, 5, "Martin", "Facture de février")]).unwrap();
        let sujets = |t: &[Trouve]| t.iter().map(|x| x.message.sujet.clone()).collect::<Vec<_>>();
        // Dossier ouvert seulement ; chaque mot doit figurer quelque part.
        assert_eq!(sujets(&m.chercher(Some(inbox), "facture", 50).unwrap()), vec!["Facture de mars"]);
        assert_eq!(sujets(&m.chercher(Some(inbox), "martin mars", 50).unwrap()), vec!["Facture de mars"]);
        assert!(m.chercher(Some(inbox), "martin réunion", 50).unwrap().is_empty());
        // Partout, les plus récents d'abord, avec leur dossier.
        let partout = m.chercher(None, "FACTURE", 50).unwrap();
        assert_eq!(sujets(&partout), vec!["Facture de mars", "Facture de février"]);
        assert_eq!(partout[1].chemin, "Archives");
        assert_eq!(partout[1].compte, compte);
        // « % » et « _ » cherchés tels quels ; texte vide : rien.
        assert_eq!(m.chercher(Some(inbox), "50%_", 50).unwrap().len(), 1);
        assert!(m.chercher(Some(inbox), "5_%", 50).unwrap().is_empty());
        assert!(m.chercher(None, "   ", 50).unwrap().is_empty());
        // Par UID, du plus récent au plus ancien.
        let par_uid = m.messages_par_uid(inbox, &[1, 3, 9]).unwrap();
        assert_eq!(par_uid.iter().map(|l| l.uid).collect::<Vec<_>>(), vec![3, 1]);
        // Résultats du serveur d'un autre dossier : avec leur compte et chemin.
        let ailleurs = m.trouves_par_uid(archives, &[1, 9]).unwrap();
        assert_eq!(sujets(&ailleurs), vec!["Facture de février"]);
        assert_eq!((ailleurs[0].compte, ailleurs[0].chemin.as_str()), (compte, "Archives"));
    }

    #[test]
    fn pieces_constatees_priment_sur_les_en_tetes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        // Supposé d'après un multipart/mixed, démenti à la lecture.
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        m.poser_pieces(id, 1, false).unwrap();
        // Une relecture des en-têtes ne rétablit pas la supposition.
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        assert!(!m.messages(id).unwrap()[0].pieces);
    }
}
