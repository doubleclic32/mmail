// SPDX-License-Identifier: GPL-3.0-or-later
//! Client IMAP : lecture, synchronisation et tri.
//!
//! Écrit ici plutôt que pris d'une bibliothèque : le sous-ensemble utile est
//! petit, et la synchronisation incrémentale à venir (QRESYNC, décision 15 du
//! dossier de projet) n'est couverte par aucune crate. Le serveur visé est
//! Dovecot, et lui seul — il annonce QRESYNC, CONDSTORE, UIDPLUS, MOVE et
//! SPECIAL-USE.
//!
//! TLS par `rustls` : pas d'OpenSSL à compiler pour Android. Les autorités de
//! certification sont celles du système sur le bureau, celles de Mozilla sous
//! Android (cf. `configuration_tls`).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use crate::protocole::{self, Dossier, Entete, EtatDossier, Statut, MARQUEUR};

/// Délai au-delà duquel une opération réseau est abandonnée.
pub(crate) const DELAI: Duration = Duration::from_secs(30);

/// Délai d'établissement de la connexion : un serveur injoignable doit être
/// signalé vite, pas au bout des deux minutes du système.
const DELAI_CONNEXION: Duration = Duration::from_secs(15);

/// Ce que rend l'ouverture d'un dossier : son état, et — en reprise QRESYNC —
/// ce qui a changé depuis la dernière visite.
#[derive(Debug, Default)]
pub struct Selection {
    pub etat: EtatDossier,
    /// Intervalles d'UID retirés depuis la dernière visite (`VANISHED`).
    pub disparus: Vec<(u32, u32)>,
    /// Drapeaux modifiés depuis la dernière visite : (UID, drapeaux).
    pub drapeaux: Vec<(u32, Vec<String>)>,
    /// Vrai si le serveur a répondu en reprise QRESYNC : `disparus` et
    /// `drapeaux` sont alors complets.
    pub reprise: bool,
}

/// Un message entier, avec ce qu'il faut pour le reposer ailleurs à l'identique.
#[derive(Debug, Clone)]
pub struct MessageComplet {
    pub drapeaux: Vec<String>,
    pub date_interne: String,
    pub octets: Vec<u8>,
}

/// En-têtes demandés pour l'index : de quoi afficher une liste de messages.
/// `CONTENT-TYPE` : de quoi supposer des pièces jointes sans lire le corps.
/// `IMPORTANCE`, `X-PRIORITY`, `PRIORITY` : l'importance annoncée par l'expéditeur.
const CHAMPS_ENTETE: &str = "FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES CONTENT-TYPE \
     IMPORTANCE X-PRIORITY PRIORITY";

pub type Resultat<T> = Result<T, Erreur>;

#[derive(Debug)]
pub enum Erreur {
    Reseau(String),
    Protocole(String),
    Refuse(String),
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erreur::Reseau(m) => write!(f, "réseau : {m}"),
            Erreur::Protocole(m) => write!(f, "protocole : {m}"),
            Erreur::Refuse(m) => write!(f, "refusé par le serveur : {m}"),
        }
    }
}

impl From<std::io::Error> for Erreur {
    fn from(e: std::io::Error) -> Self {
        Erreur::Reseau(e.to_string())
    }
}

/// Flux d'une session : TLS vers le serveur en service, faux serveur dans
/// les tests (`crate::simule`).
pub(crate) trait Flux: Read + Write + Send {
    /// Délai d'attente d'une lecture : court pendant `IDLE`, pour revenir
    /// voir si l'interface a demandé quelque chose.
    fn delai_lecture(&mut self, _delai: Duration) -> std::io::Result<()> {
        Ok(())
    }
}

/// Session TLS sur TCP, celle du service.
type Tls = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

impl Flux for Tls {
    fn delai_lecture(&mut self, delai: Duration) -> std::io::Result<()> {
        self.sock.set_read_timeout(Some(delai))
    }
}

/// Session IMAP ouverte sur un serveur.
pub struct Client {
    flux: Box<dyn Flux>,
    /// Octets lus mais pas encore consommés.
    reste: Vec<u8>,
    compteur: u32,
    capacites: Vec<String>,
    /// QRESYNC activé par `ENABLE` : le serveur rend alors `VANISHED`.
    qresync: bool,
    /// Dossier sélectionné, s'il y en a un.
    selection: Option<String>,
    /// Étiquette de la commande `IDLE` en cours, s'il y en a une.
    idle: Option<String>,
}

impl Client {
    /// Ouvre une connexion IMAPS et lit la salutation du serveur.
    pub fn connecter(hote: &str, port: u16) -> Resultat<Client> {
        let config = configuration_tls()?;
        let nom = rustls::pki_types::ServerName::try_from(hote.to_string())
            .map_err(|_| Erreur::Reseau(format!("nom de serveur invalide : {hote}")))?;
        let connexion = rustls::ClientConnection::new(Arc::new(config), nom)
            .map_err(|e| Erreur::Reseau(e.to_string()))?;
        let tcp = joindre(hote, port)?;
        tcp.set_read_timeout(Some(DELAI))?;
        tcp.set_write_timeout(Some(DELAI))?;
        Client::sur(Box::new(rustls::StreamOwned::new(connexion, tcp)))
    }

    /// Session sur un flux déjà ouvert, dont on lit la salutation.
    pub(crate) fn sur(flux: Box<dyn Flux>) -> Resultat<Client> {
        let mut client = Client {
            flux,
            reste: Vec::new(),
            compteur: 0,
            capacites: Vec::new(),
            qresync: false,
            selection: None,
            idle: None,
        };
        // Salutation : une seule ligne non étiquetée.
        let salutation = client.lire_ligne()?;
        if !salutation.starts_with("* OK") {
            return Err(Erreur::Protocole(format!("salutation inattendue : {salutation}")));
        }
        Ok(client)
    }

    /// Ouvre la session et relève les capacités annoncées après connexion.
    ///
    /// Elles diffèrent de celles d'avant connexion : c'est là que Dovecot
    /// annonce QRESYNC, CONDSTORE et UIDPLUS.
    pub fn ouvrir_session(&mut self, utilisateur: &str, mot_de_passe: &str) -> Resultat<()> {
        self.commande(&format!(
            "LOGIN {} {}",
            citer(utilisateur),
            citer(mot_de_passe)
        ))?;
        self.apres_authentification()
    }

    /// Comme `ouvrir_session`, par un jeton d'accès OAuth2 (`AUTHENTICATE
    /// XOAUTH2`, comptes Microsoft et Google) : la chaîne SASL part avec la commande
    /// (SASL-IR, que les serveurs de Microsoft annoncent).
    pub fn ouvrir_session_jeton(&mut self, utilisateur: &str, jeton: &str) -> Resultat<()> {
        self.authentifier("XOAUTH2", &crate::microsoft::xoauth2(utilisateur, jeton))?;
        self.apres_authentification()
    }

    /// `AUTHENTICATE` avec réponse initiale. Un refus peut d'abord venir en
    /// invite (« + » suivi du motif en base64), à laquelle le client répond
    /// par une ligne vide pour recevoir le `NO` final (RFC 7628 § 3.2.3).
    fn authentifier(&mut self, mecanisme: &str, reponse: &str) -> Resultat<()> {
        self.compteur += 1;
        let etiquette = format!("m{:04}", self.compteur);
        self.flux
            .write_all(format!("{etiquette} AUTHENTICATE {mecanisme} {reponse}\r\n").as_bytes())?;
        self.flux.flush()?;
        let mut motif = String::new();
        loop {
            let ligne = self.lire_ligne()?;
            if let Some(invite) = ligne.strip_prefix('+') {
                motif = crate::rendu::base64(invite.trim())
                    .map(|o| String::from_utf8_lossy(&o).into_owned())
                    .unwrap_or_default();
                self.flux.write_all(b"\r\n")?;
                self.flux.flush()?;
                continue;
            }
            if let Some(fin) = ligne.strip_prefix(&etiquette) {
                let fin = fin.trim_start();
                return if fin.starts_with("OK") {
                    Ok(())
                } else if motif.is_empty() {
                    Err(Erreur::Refuse(fin.to_string()))
                } else {
                    Err(Erreur::Refuse(format!("{fin} ({motif})")))
                };
            }
        }
    }

    /// Ce qui suit l'authentification, quelle qu'elle soit : capacités,
    /// présentation, QRESYNC.
    fn apres_authentification(&mut self) -> Resultat<()> {
        let (texte, _) = self.commande("CAPABILITY")?;
        self.capacites = texte
            .lines()
            .filter_map(|l| l.trim().strip_prefix("* CAPABILITY "))
            .flat_map(|l| l.split_whitespace().map(str::to_string))
            .collect();
        // Présentation au serveur (RFC 2971) : de quoi compter, dans son
        // journal, les postes où MMail fonctionne. Après l'authentification —
        // avant, Dovecot la journalise sans l'utilisateur. Un refus n'empêche
        // pas la session.
        if self.sait("ID") {
            let _ = self.commande(&format!("ID {}", presentation()));
        }
        // QRESYNC n'est actif qu'une fois demandé (RFC 7162 § 3.2.3). Il
        // emporte CONDSTORE, et fait rendre `VANISHED` plutôt que des
        // `EXPUNGE` numérotés, qu'un index par UID ne sait pas appliquer.
        if self.sait("QRESYNC") {
            self.commande("ENABLE QRESYNC")?;
            self.qresync = true;
        }
        Ok(())
    }

    pub fn qresync(&self) -> bool {
        self.qresync
    }

    pub fn capacites(&self) -> &[String] {
        &self.capacites
    }

    pub fn sait(&self, capacite: &str) -> bool {
        self.capacites.iter().any(|c| c == capacite)
    }

    /// Vrai si le serveur tient un `MODSEQ` par message (CONDSTORE, que
    /// QRESYNC emporte) : de quoi poser un drapeau sous condition.
    pub fn condstore(&self) -> bool {
        self.qresync || self.sait("CONDSTORE")
    }

    /// Arborescence complète des dossiers.
    pub fn dossiers(&mut self) -> Resultat<Vec<Dossier>> {
        let (texte, _) = self.commande("LIST \"\" \"*\"")?;
        Ok(texte.lines().filter_map(protocole::analyser_list).collect())
    }

    /// Arborescence et compteurs de chaque dossier, en une commande quand le
    /// serveur sait LIST-STATUS (RFC 5819), dossier par dossier sinon.
    pub fn dossiers_et_compteurs(&mut self) -> Resultat<(Vec<Dossier>, Vec<Statut>)> {
        if self.sait("LIST-STATUS") {
            // Une LIST étendue ne rend les attributs SPECIAL-USE que s'ils
            // sont demandés (RFC 6154 § 2) : sans « SPECIAL-USE », « Envoyés »
            // et « Corbeille » perdraient leur rôle.
            let retour = if self.sait("SPECIAL-USE") {
                "RETURN (SPECIAL-USE STATUS (MESSAGES UNSEEN))"
            } else {
                "RETURN (STATUS (MESSAGES UNSEEN))"
            };
            let (texte, _) = self.commande(&format!("LIST \"\" \"*\" {retour}"))?;
            let dossiers = texte.lines().filter_map(protocole::analyser_list).collect();
            return Ok((dossiers, protocole::analyser_status(&texte)));
        }
        let dossiers = self.dossiers()?;
        let mut statuts = Vec::new();
        for dossier in dossiers.iter().filter(|d| d.selectionnable()) {
            let (texte, _) = self.commande(&format!(
                "STATUS {} (MESSAGES UNSEEN)",
                citer(&dossier.chemin)
            ))?;
            statuts.extend(protocole::analyser_status(&texte));
        }
        Ok((dossiers, statuts))
    }

    /// Ouvre un dossier en lecture seule et rend son état de synchronisation.
    pub fn examiner(&mut self, chemin: &str) -> Resultat<EtatDossier> {
        let (texte, _) = self.commande(&format!("EXAMINE {}", citer(chemin)))?;
        // Lecture seule : un marquage ou un déplacement exigera une vraie
        // sélection, d'où l'absence de dossier « sélectionné » ici.
        self.selection = None;
        Ok(protocole::analyser_select(&texte))
    }

    /// Ouvre un dossier en lecture et écriture.
    ///
    /// `reprise` porte l'`UIDVALIDITY` et le `HIGHESTMODSEQ` de la dernière
    /// visite : le serveur rend alors, en plus de l'état, les UID retirés et
    /// les drapeaux changés depuis (décision 15). Sans QRESYNC, ou sans visite
    /// antérieure, seul l'état est rendu.
    pub fn selectionner(&mut self, chemin: &str, reprise: Option<(u32, u64)>) -> Resultat<Selection> {
        let commande = match reprise {
            Some((validite, modseq)) if self.qresync && validite != 0 && modseq != 0 => format!(
                "SELECT {} (QRESYNC ({validite} {modseq}))",
                citer(chemin)
            ),
            _ => format!("SELECT {}", citer(chemin)),
        };
        let reprise_demandee = commande.contains("QRESYNC");
        // Un SELECT refusé laisse le serveur sans dossier sélectionné (RFC 3501
        // § 6.3.1) : l'ancienne sélection ne vaut plus, quoi qu'il arrive.
        self.selection = None;
        let (texte, litteraux) = self.commande(&commande)?;
        self.selection = Some(chemin.to_string());
        let etat = protocole::analyser_select(&texte);
        let validite_intacte = reprise.map(|(v, _)| v == etat.uid_validity).unwrap_or(false);
        Ok(Selection {
            disparus: protocole::analyser_vanished(&texte),
            drapeaux: protocole::analyser_fetch(&texte, &litteraux)
                .into_iter()
                .map(|e| (e.uid, e.drapeaux))
                .collect(),
            reprise: reprise_demandee && validite_intacte,
            etat,
        })
    }

    /// Dossier sélectionné, s'il y en a un.
    pub fn selection(&self) -> Option<&str> {
        self.selection.as_deref()
    }

    /// UID et drapeaux de tous les messages du dossier sélectionné : la
    /// synchronisation complète, pour un serveur sans QRESYNC.
    pub fn tous_les_drapeaux(&mut self) -> Resultat<Vec<(u32, Vec<String>)>> {
        let (texte, litteraux) = self.commande("UID FETCH 1:* (UID FLAGS)")?;
        Ok(protocole::analyser_fetch(&texte, &litteraux)
            .into_iter()
            .map(|e| (e.uid, e.drapeaux))
            .collect())
    }

    /// En-têtes des messages d'un intervalle d'UID (`1:*` pour tout le dossier).
    pub fn entetes(&mut self, intervalle: &str) -> Resultat<Vec<Entete>> {
        let (texte, litteraux) = self.commande(&format!(
            "UID FETCH {intervalle} (UID FLAGS INTERNALDATE RFC822.SIZE \
             BODY.PEEK[HEADER.FIELDS ({CHAMPS_ENTETE})])"
        ))?;
        Ok(protocole::analyser_fetch(&texte, &litteraux))
    }

    /// Quelques champs d'en-tête, désignés, des messages d'un intervalle, avec
    /// le `MODSEQ` de chacun quand le serveur en tient (cf. `reserver`).
    pub fn champs(&mut self, intervalle: &str, champs: &str) -> Resultat<Vec<Entete>> {
        let modseq = if self.condstore() { " MODSEQ" } else { "" };
        let (texte, litteraux) = self.commande(&format!(
            "UID FETCH {intervalle} (UID FLAGS INTERNALDATE RFC822.SIZE{modseq} BODY.PEEK[HEADER.FIELDS ({champs})])"
        ))?;
        Ok(protocole::analyser_fetch(&texte, &litteraux))
    }

    /// Crée un dossier (nom en UTF-7 modifié). Un dossier qui existe déjà
    /// n'est pas une erreur.
    pub fn creer(&mut self, chemin: &str) -> Resultat<()> {
        match self.commande(&format!("CREATE {}", citer(chemin))) {
            Ok(_) => Ok(()),
            Err(Erreur::Refuse(m)) if m.to_ascii_uppercase().contains("ALREADYEXISTS") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Abonne la session à un dossier : les logiciels qui n'affichent que les
    /// dossiers abonnés — webmail, Outlook — le montrent aussi.
    pub fn abonner(&mut self, chemin: &str) -> Resultat<()> {
        self.commande(&format!("SUBSCRIBE {}", citer(chemin))).map(|_| ())
    }

    /// Supprime un dossier : sert aux épreuves contre un vrai serveur, qui
    /// retirent ce qu'elles ont créé.
    #[cfg(test)]
    pub fn supprimer_dossier(&mut self, chemin: &str) -> Resultat<()> {
        let _ = self.commande(&format!("UNSUBSCRIBE {}", citer(chemin)));
        self.commande(&format!("DELETE {}", citer(chemin))).map(|_| ())
    }

    /// Réponse texte d'une commande quelconque, pour les épreuves.
    #[cfg(test)]
    pub fn brute(&mut self, commande: &str) -> Resultat<String> {
        self.commande(commande).map(|(texte, _)| texte)
    }

    /// Message entier, tel que le serveur le conserve (décision 6 du dossier).
    ///
    /// `BODY.PEEK` et non `BODY` : lire un message ne doit pas le marquer comme
    /// lu à l'insu de l'utilisateur.
    pub fn corps(&mut self, uid: u32) -> Resultat<Vec<u8>> {
        let (_, litteraux) = self.commande(&format!("UID FETCH {uid} (BODY.PEEK[])"))?;
        litteraux
            .into_iter()
            .next()
            .ok_or_else(|| Erreur::Protocole(format!("aucun corps rendu pour l'UID {uid}")))
    }

    /// Message entier avec ses drapeaux et sa date interne : ce qu'il faut pour
    /// le reposer à l'identique dans une autre boîte. `None` si le message a
    /// disparu entre-temps.
    pub fn message_complet(&mut self, uid: u32) -> Resultat<Option<MessageComplet>> {
        let (texte, litteraux) =
            self.commande(&format!("UID FETCH {uid} (UID FLAGS INTERNALDATE BODY.PEEK[])"))?;
        Ok(protocole::analyser_fetch(&texte, &litteraux)
            .into_iter()
            .find(|e| e.uid == uid && !e.brut.is_empty())
            .map(|e| MessageComplet {
                drapeaux: e.drapeaux,
                date_interne: e.date_interne,
                octets: e.brut,
            }))
    }

    /// Ajoute ou retire un drapeau sur des messages du dossier sélectionné.
    pub fn marquer(&mut self, uids: &[u32], drapeau: &str, present: bool) -> Resultat<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let signe = if present { '+' } else { '-' };
        self.commande(&format!(
            "UID STORE {} {signe}FLAGS.SILENT ({drapeau})",
            protocole::ensemble_uid(uids)
        ))?;
        Ok(())
    }

    /// Réserve un message du dossier sélectionné en le marquant `\Deleted`,
    /// à condition que personne ne l'ait modifié depuis le `MODSEQ` lu
    /// (`UNCHANGEDSINCE`, RFC 7162 § 3.1.3). Rend faux si un autre client l'a
    /// touché entre-temps — il l'a sans doute réservé lui-même. Sans CONDSTORE,
    /// ou sans `MODSEQ` connu, le marquage se fait sans condition.
    pub fn reserver(&mut self, uid: u32, modseq: u64) -> Resultat<bool> {
        let condition =
            if self.condstore() && modseq > 0 { format!(" (UNCHANGEDSINCE {modseq})") } else { String::new() };
        let (texte, _) = self.commande(&format!("UID STORE {uid}{condition} +FLAGS.SILENT (\\Deleted)"))?;
        // Échec de la condition : « OK [MODIFIED 7] », la commande réussit
        // sans rien marquer.
        Ok(!texte.lines().any(|l| l.contains("[MODIFIED")))
    }

    /// Déplace des messages du dossier sélectionné vers un autre dossier de la
    /// même boîte. `MOVE` (RFC 6851) quand le serveur le sait : l'opération est
    /// alors atomique ; à défaut copie, marquage et purge ciblée.
    pub fn deplacer(&mut self, uids: &[u32], cible: &str) -> Resultat<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let ensemble = protocole::ensemble_uid(uids);
        if self.sait("MOVE") {
            self.commande(&format!("UID MOVE {ensemble} {}", citer(cible)))?;
            return Ok(());
        }
        self.commande(&format!("UID COPY {ensemble} {}", citer(cible)))?;
        self.supprimer(uids)
    }

    /// Retire définitivement des messages du dossier sélectionné : marquage
    /// `\Deleted`, puis purge de ces seuls UID (`UID EXPUNGE`, UIDPLUS). Sans
    /// UIDPLUS, un `EXPUNGE` purgerait aussi ce qu'un autre client a marqué :
    /// l'opération est alors refusée plutôt que de risquer ce dégât.
    pub fn supprimer(&mut self, uids: &[u32]) -> Resultat<()> {
        if uids.is_empty() {
            return Ok(());
        }
        if !self.sait("UIDPLUS") {
            return Err(Erreur::Refuse(
                "le serveur ne sait pas purger des messages désignés (UIDPLUS absent)".into(),
            ));
        }
        let ensemble = protocole::ensemble_uid(uids);
        self.commande(&format!("UID STORE {ensemble} +FLAGS.SILENT (\\Deleted)"))?;
        self.commande(&format!("UID EXPUNGE {ensemble}"))?;
        Ok(())
    }

    /// Vide entièrement un dossier : sélection, marquage `\Deleted` de tous
    /// ses messages (par numéro de séquence, `1:*`), puis `EXPUNGE`. Rend le
    /// nombre de messages purgés. Contrairement à `supprimer`, l'`EXPUNGE` est
    /// volontairement global : on veut le dossier vide, pas quelques UID — c'est
    /// le geste attendu pour une corbeille ou un dossier d'indésirables.
    pub fn vider(&mut self, chemin: &str) -> Resultat<u32> {
        let selection = self.selectionner(chemin, None)?;
        let nombre = selection.etat.messages;
        if nombre == 0 {
            return Ok(0);
        }
        self.commande("STORE 1:* +FLAGS.SILENT (\\Deleted)")?;
        self.commande("EXPUNGE")?;
        Ok(nombre)
    }

    /// Dépose un message dans un dossier, avec ses drapeaux et sa date interne
    /// d'origine (décision 16). Rend l'UID attribué quand le serveur le dit
    /// (UIDPLUS).
    pub fn deposer(
        &mut self,
        chemin: &str,
        drapeaux: &[String],
        date_interne: &str,
        octets: &[u8],
    ) -> Resultat<Option<u32>> {
        // \Recent est posé par le serveur et ne peut pas l'être par un client.
        let drapeaux: Vec<&str> = drapeaux
            .iter()
            .map(String::as_str)
            .filter(|d| !d.eq_ignore_ascii_case("\\Recent"))
            .collect();
        let date = if date_interne.is_empty() { String::new() } else { format!(" {}", citer(date_interne)) };
        self.compteur += 1;
        let etiquette = format!("m{:04}", self.compteur);
        self.flux.write_all(
            format!(
                "{etiquette} APPEND {} ({}){date} {{{}}}\r\n",
                citer(chemin),
                drapeaux.join(" "),
                octets.len()
            )
            .as_bytes(),
        )?;
        self.flux.flush()?;
        // Littéral synchronisé : le serveur doit d'abord dire qu'il accepte.
        // Des réponses non étiquetées peuvent précéder l'invite.
        loop {
            let ligne = self.lire_ligne()?;
            if ligne.starts_with('+') {
                break;
            }
            if let Some(refus) = ligne.strip_prefix(&etiquette) {
                return Err(Erreur::Refuse(refus.trim().to_string()));
            }
        }
        self.flux.write_all(octets)?;
        self.flux.write_all(b"\r\n")?;
        self.flux.flush()?;
        let (texte, _) = self.lire_reponse(&etiquette)?;
        Ok(protocole::analyser_appenduid(&texte).map(|(_, uid)| uid))
    }

    /// UID des messages du dossier sélectionné dont le texte — en-têtes et
    /// corps — contient chacun des mots : un critère `TEXT` par mot, en UTF-8.
    /// Les mots partent en littéraux non synchronisés (LITERAL+) ; sans eux,
    /// seul un mot ASCII peut s'écrire entre guillemets.
    pub fn chercher_texte(&mut self, texte: &str) -> Resultat<Vec<u32>> {
        let mots: Vec<&str> = texte.split_whitespace().collect();
        if mots.is_empty() {
            return Ok(Vec::new());
        }
        let litteraux = self.sait("LITERAL+");
        let mut criteres = String::new();
        for mot in mots {
            if litteraux {
                criteres.push_str(&format!(" TEXT {{{}+}}\r\n{mot}", mot.len()));
            } else if mot.is_ascii() {
                criteres.push_str(&format!(" TEXT {}", citer(mot)));
            } else {
                return Err(Erreur::Refuse("le serveur ne sait pas chercher ce texte".into()));
            }
        }
        let (texte, _) = self.commande(&format!("UID SEARCH CHARSET UTF-8{criteres}"))?;
        Ok(protocole::analyser_search(&texte))
    }

    /// UID des messages du dossier sélectionné portant un `Message-ID` donné :
    /// sert à reprendre un déplacement interrompu sans créer de doublon.
    pub fn chercher_message_id(&mut self, message_id: &str) -> Resultat<Vec<u32>> {
        let (texte, _) = self.commande(&format!(
            "UID SEARCH HEADER Message-ID {}",
            citer(message_id)
        ))?;
        Ok(protocole::analyser_search(&texte))
    }

    /// Entre en `IDLE` (RFC 2177) sur le dossier sélectionné : le serveur
    /// signale alors de lui-même messages arrivés, retirés et drapeaux changés.
    pub fn idle_commencer(&mut self) -> Resultat<()> {
        self.compteur += 1;
        let etiquette = format!("m{:04}", self.compteur);
        self.flux.write_all(format!("{etiquette} IDLE\r\n").as_bytes())?;
        self.flux.flush()?;
        loop {
            let ligne = self.lire_ligne()?;
            if ligne.starts_with('+') {
                self.idle = Some(etiquette);
                return Ok(());
            }
            if let Some(refus) = ligne.strip_prefix(&etiquette) {
                return Err(Erreur::Refuse(refus.trim().to_string()));
            }
        }
    }

    /// Attend au plus `delai` une ligne du serveur pendant `IDLE`. `None` :
    /// rien n'est venu — c'est le cas courant, la session reste en `IDLE`.
    pub fn idle_attendre(&mut self, delai: Duration) -> Resultat<Option<String>> {
        if let Some(ligne) = self.ligne_en_attente() {
            return Ok(Some(ligne));
        }
        self.flux.delai_lecture(delai)?;
        let mut tampon = [0u8; 16384];
        let lu = self.flux.read(&mut tampon);
        self.flux.delai_lecture(DELAI)?;
        match lu {
            Ok(0) => Err(Erreur::Reseau("connexion fermée par le serveur".into())),
            Ok(n) => {
                self.reste.extend_from_slice(&tampon[..n]);
                Ok(self.ligne_en_attente())
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Sort d'`IDLE` (`DONE`) et rend les réponses arrivées entre-temps.
    pub fn idle_terminer(&mut self) -> Resultat<String> {
        let Some(etiquette) = self.idle.take() else {
            return Ok(String::new());
        };
        self.flux.write_all(b"DONE\r\n")?;
        self.flux.flush()?;
        Ok(self.lire_reponse(&etiquette)?.0)
    }

    /// Maintient la session et laisse le serveur signaler ses changements.
    pub fn noop(&mut self) -> Resultat<()> {
        self.commande("NOOP")?;
        Ok(())
    }

    pub fn fermer(&mut self) {
        let _ = self.commande("LOGOUT");
    }

    // ----------------------------------------------------------------- privé

    /// Envoie une commande et rend la réponse : le texte, littéraux remplacés
    /// par leur marqueur, et les littéraux eux-mêmes dans l'ordre de lecture.
    fn commande(&mut self, commande: &str) -> Resultat<(String, Vec<Vec<u8>>)> {
        self.compteur += 1;
        let etiquette = format!("m{:04}", self.compteur);
        self.flux
            .write_all(format!("{etiquette} {commande}\r\n").as_bytes())?;
        self.flux.flush()?;
        self.lire_reponse(&etiquette)
    }

    /// Lit jusqu'à la ligne étiquetée qui clôt la commande.
    fn lire_reponse(&mut self, etiquette: &str) -> Resultat<(String, Vec<Vec<u8>>)> {
        let mut texte = String::new();
        let mut litteraux: Vec<Vec<u8>> = Vec::new();
        loop {
            let mut ligne = self.lire_ligne()?;
            // Un littéral suspend la ligne : « … {123} » puis 123 octets bruts,
            // puis la suite de la même ligne logique.
            while let Some(taille) = taille_litteral(&ligne) {
                let donnees = self.lire_octets(taille)?;
                let indice = litteraux.len();
                litteraux.push(donnees);
                ligne = format!(
                    "{}{MARQUEUR}{indice}{MARQUEUR}",
                    &ligne[..ligne.rfind('{').unwrap_or(ligne.len())]
                );
                ligne.push_str(&self.lire_ligne()?);
            }
            let terminee = ligne.starts_with(etiquette);
            texte.push_str(&ligne);
            texte.push('\n');
            if terminee {
                let sans_etiquette = ligne[etiquette.len()..].trim_start();
                return if sans_etiquette.starts_with("OK") {
                    Ok((texte, litteraux))
                } else {
                    Err(Erreur::Refuse(sans_etiquette.to_string()))
                };
            }
        }
    }

    /// Ligne complète déjà reçue, s'il y en a une.
    fn ligne_en_attente(&mut self) -> Option<String> {
        let fin = trouver(&self.reste, b"\r\n")?;
        let ligne = self.reste.drain(..fin + 2).collect::<Vec<_>>();
        Some(String::from_utf8_lossy(&ligne[..fin]).into_owned())
    }

    /// Lit une ligne terminée par CRLF, sans le CRLF.
    fn lire_ligne(&mut self) -> Resultat<String> {
        loop {
            if let Some(fin) = trouver(&self.reste, b"\r\n") {
                let ligne = self.reste.drain(..fin + 2).collect::<Vec<_>>();
                return Ok(String::from_utf8_lossy(&ligne[..fin]).into_owned());
            }
            self.remplir()?;
        }
    }

    /// Lit exactement `taille` octets — le contenu d'un littéral.
    fn lire_octets(&mut self, taille: usize) -> Resultat<Vec<u8>> {
        while self.reste.len() < taille {
            self.remplir()?;
        }
        Ok(self.reste.drain(..taille).collect())
    }

    fn remplir(&mut self) -> Resultat<()> {
        let mut tampon = [0u8; 16384];
        let lus = self.flux.read(&mut tampon)?;
        if lus == 0 {
            return Err(Erreur::Reseau("connexion fermée par le serveur".into()));
        }
        self.reste.extend_from_slice(&tampon[..lus]);
        Ok(())
    }
}

/// Configuration TLS du bureau : le vérificateur de la plateforme, qui lit le
/// magasin de certificats du système (Windows, Linux, macOS).
#[cfg(not(target_os = "android"))]
pub(crate) fn configuration_tls() -> Resultat<rustls::ClientConfig> {
    use rustls_platform_verifier::ConfigVerifierExt;
    rustls::ClientConfig::with_platform_verifier().map_err(|e| Erreur::Reseau(e.to_string()))
}

/// Configuration TLS d'Android : les autorités de Mozilla, compilées avec le
/// programme. Le vérificateur de plateforme y exigerait un composant Java et une
/// initialisation JNI que l'APK n'embarque pas — toute connexion échouerait.
#[cfg(target_os = "android")]
pub(crate) fn configuration_tls() -> Resultat<rustls::ClientConfig> {
    let racines = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    Ok(rustls::ClientConfig::builder().with_root_certificates(racines).with_no_client_auth())
}

/// Établit la connexion TCP, adresse par adresse, avec un délai borné.
pub(crate) fn joindre(hote: &str, port: u16) -> Resultat<TcpStream> {
    let adresses = (hote, port)
        .to_socket_addrs()
        .map_err(|e| Erreur::Reseau(format!("{hote} : {e}")))?;
    let mut derniere = Erreur::Reseau(format!("{hote} : aucune adresse"));
    for adresse in adresses {
        match TcpStream::connect_timeout(&adresse, DELAI_CONNEXION) {
            Ok(tcp) => return Ok(tcp),
            Err(e) => derniere = Erreur::Reseau(format!("{adresse} : {e}")),
        }
    }
    Err(derniere)
}

/// Taille annoncée par un littéral en fin de ligne : `… {123}`.
fn taille_litteral(ligne: &str) -> Option<usize> {
    let ligne = ligne.trim_end();
    let debut = ligne.rfind('{')?;
    let contenu = ligne[debut + 1..].strip_suffix('}')?;
    // « {123+} » : littéral non synchronisé (LITERAL+), même taille.
    contenu.trim_end_matches('+').parse().ok()
}

/// Met une chaîne entre guillemets IMAP, en protégeant les caractères réservés.
fn citer(valeur: &str) -> String {
    let echappe = valeur.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{echappe}\"")
}

/// Liste de la commande `ID` : programme, version, système, poste. `x-poste`
/// n'est pas un champ de la RFC 2971, qui en admet d'autres que les siens.
fn presentation() -> String {
    let champs = [
        ("name", "MMail".to_string()),
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("os", crate::poste::famille().to_string()),
        ("os-version", crate::poste::systeme()),
        ("x-poste", crate::poste::nom()),
    ];
    let paires: Vec<String> = champs
        .iter()
        .map(|(nom, valeur)| format!("{} {}", citer(nom), citer(&valeur_id(valeur))))
        .collect();
    format!("({})", paires.join(" "))
}

/// Valeur admise dans une chaîne entre guillemets : ASCII imprimable, 1 024
/// octets au plus (RFC 2971 § 3.3).
fn valeur_id(valeur: &str) -> String {
    let propre: String = valeur
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(1024)
        .collect();
    if propre.is_empty() { "?".to_string() } else { propre }
}

fn trouver(foin: &[u8], aiguille: &[u8]) -> Option<usize> {
    foin.windows(aiguille.len()).position(|f| f == aiguille)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_de_presentation() {
        assert_eq!(valeur_id("Windows 11 Version 24H2"), "Windows 11 Version 24H2");
        assert_eq!(valeur_id("PC-ÉLODIE\r\n"), "PC-?LODIE??");
        assert_eq!(valeur_id(""), "?");
        assert_eq!(valeur_id(&"x".repeat(2000)).len(), 1024);
        // Guillemets et barres obliques passent, protégés par `citer`.
        assert_eq!(citer(&valeur_id("a\"b\\c")), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn litteraux_reconnus() {
        assert_eq!(taille_litteral("* 1 FETCH (BODY[] {412}"), Some(412));
        assert_eq!(taille_litteral("* 1 FETCH (BODY[] {412+}"), Some(412));
        assert_eq!(taille_litteral("m0001 OK done"), None);
        // Une accolade en fin de sujet ne doit pas passer pour un littéral.
        assert_eq!(taille_litteral("* 1 FETCH (SUBJECT \"a}\""), None);
    }

    #[test]
    fn citation_des_identifiants() {
        assert_eq!(citer("simple"), "\"simple\"");
        // Un mot de passe peut contenir des caractères réservés du protocole.
        assert_eq!(citer(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    use crate::simule::{echange, session, DOVECOT};

    #[test]
    fn reservation_sous_condition() {
        let (mut client, journal) = session(
            DOVECOT,
            vec![
                echange("UID STORE 7 (UNCHANGEDSINCE 120) +FLAGS.SILENT (\\Deleted)", "", "OK fait"),
                echange("UID STORE 8 (UNCHANGEDSINCE 90) +FLAGS.SILENT (\\Deleted)", "", "OK [MODIFIED 8] Conditional STORE failed"),
            ],
        );
        assert!(client.reserver(7, 120).unwrap());
        // Un autre client l'a touché depuis le relevé : il n'est pas à nous.
        assert!(!client.reserver(8, 90).unwrap());
        assert_eq!(journal.lock().unwrap().len(), 2);
    }

    #[test]
    fn reservation_sans_condstore() {
        let (mut client, _) =
            session("UIDPLUS", vec![echange("UID STORE 7 +FLAGS.SILENT (\\Deleted)", "", "OK fait")]);
        assert!(client.reserver(7, 120).unwrap());
    }

    #[test]
    fn select_refuse_efface_la_selection() {
        let (mut client, journal) = session(
            DOVECOT,
            vec![
                echange("SELECT \"INBOX\"", "* 3 EXISTS\r\n* OK [UIDVALIDITY 1] v\r\n", "OK [READ-WRITE] fait"),
                echange("SELECT \"Disparu\"", "", "NO Mailbox doesn't exist: Disparu"),
                echange("SELECT \"INBOX\"", "* 3 EXISTS\r\n", "OK [READ-WRITE] fait"),
            ],
        );
        client.selectionner("INBOX", None).unwrap();
        assert!(client.selectionner("Disparu", None).is_err());
        assert_eq!(client.selection(), None);
        // Revenir à INBOX refait le SELECT au lieu de croire qu'il tient encore.
        crate::synchro::assurer_selection(&mut client, "INBOX").unwrap();
        assert_eq!(journal.lock().unwrap().len(), 3);
    }

    #[test]
    fn recherche_par_mots_en_utf8() {
        let (mut client, journal) = session(
            &format!("{DOVECOT} LITERAL+"),
            vec![echange("UID SEARCH CHARSET UTF-8 TEXT {7 octets}facture", "* SEARCH 4 9\r\n", "OK fait")],
        );
        assert_eq!(client.chercher_texte(" facture  été ").unwrap(), vec![4, 9]);
        assert_eq!(
            journal.lock().unwrap()[0],
            "UID SEARCH CHARSET UTF-8 TEXT {7 octets}facture TEXT {5 octets}été"
        );
        // Sans LITERAL+, un mot accentué ne s'écrit pas.
        let (mut client, _) = session(DOVECOT, vec![]);
        assert!(client.chercher_texte("été").is_err());
    }

    #[test]
    fn idle_signale_puis_se_termine() {
        let (mut client, journal) = session(
            DOVECOT,
            vec![
                echange("SELECT \"INBOX\"", "* 3 EXISTS\r\n", "OK [READ-WRITE] fait"),
                echange("IDLE", "* 4 EXISTS\r\n", ""),
                echange("DONE", "", "OK Idle completed"),
            ],
        );
        client.selectionner("INBOX", None).unwrap();
        client.idle_commencer().unwrap();
        assert_eq!(client.idle_attendre(Duration::from_millis(10)).unwrap().as_deref(), Some("* 4 EXISTS"));
        // Plus rien : le délai s'écoule, la session reste en IDLE.
        assert_eq!(client.idle_attendre(Duration::from_millis(10)).unwrap(), None);
        client.idle_terminer().unwrap();
        assert_eq!(journal.lock().unwrap().as_slice(), ["SELECT \"INBOX\"", "IDLE", "DONE"]);
        // Hors IDLE, terminer ne fait rien.
        assert!(client.idle_terminer().unwrap().is_empty());
    }

    #[test]
    fn recherche_dans_le_tampon() {
        assert_eq!(trouver(b"abc\r\ndef", b"\r\n"), Some(3));
        assert_eq!(trouver(b"abc", b"\r\n"), None);
    }
}
