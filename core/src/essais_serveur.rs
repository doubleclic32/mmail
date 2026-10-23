// SPDX-License-Identifier: GPL-3.0-or-later
//! Épreuve du client contre un vrai Dovecot — et, pour l'agenda, contre le
//! SOGo du même serveur.
//!
//! Ignoré par défaut : il exige un serveur et deux comptes, donnés par
//! l'environnement. Aucune identification n'est écrite ici.
//!
//! ```text
//! MMAIL_HOTE=imap.exemple.fr MMAIL_UTILISATEUR=… MMAIL_MOTDEPASSE=… \
//! MMAIL_UTILISATEUR2=… MMAIL_MOTDEPASSE2=… \
//!   cargo test --manifest-path core/Cargo.toml essais_serveur -- --ignored --test-threads=1
//! ```
//!
//! Chaque épreuve dépose ses propres messages, marqués d'un `Message-ID`
//! unique, et les retire en fin de parcours : le contenu d'essai des boîtes
//! n'est pas touché. `--test-threads=1` : deux épreuves qui écrivent dans la
//! même boîte en même temps fausseraient leurs comptes.


#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use crate::deplacement;
use crate::imap::Client;
use crate::magasin::{Etape, Magasin, Operation};
use crate::protocole::secondes_date_interne;
use crate::synchro;

fn variable(nom: &str) -> String {
    std::env::var(nom).unwrap_or_else(|_| panic!("{nom} absente de l'environnement"))
}

fn session() -> Client {
    session_de("MMAIL_UTILISATEUR", "MMAIL_MOTDEPASSE")
}

fn session2() -> Client {
    session_de("MMAIL_UTILISATEUR2", "MMAIL_MOTDEPASSE2")
}

fn session_de(utilisateur: &str, mot_de_passe: &str) -> Client {
    let mut client = Client::connecter(&variable("MMAIL_HOTE"), 993).expect("connexion");
    client
        .ouvrir_session(&variable(utilisateur), &variable(mot_de_passe))
        .expect("session");
    client
}

/// Index en mémoire portant les deux comptes d'essai, arborescences relues.
fn index(a: &mut Client, b: Option<&mut Client>) -> (Magasin, i64, i64) {
    let magasin = Magasin::en_memoire().unwrap();
    let hote = variable("MMAIL_HOTE");
    let ua = variable("MMAIL_UTILISATEUR");
    let ca = magasin.compte(&ua, &hote, 993, &ua).unwrap();
    synchro::arborescence(a, &magasin, ca).expect("arborescence A");
    let mut cb = 0;
    if let Some(b) = b {
        let ub = variable("MMAIL_UTILISATEUR2");
        cb = magasin.compte(&ub, &hote, 993, &ub).unwrap();
        synchro::arborescence(b, &magasin, cb).expect("arborescence B");
    }
    (magasin, ca, cb)
}

/// Message d'essai au `Message-ID` unique.
fn message_essai(objet: &str) -> (String, Vec<u8>) {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let id = format!("mmail-essai-{unique}@exemple.fr");
    let brut = format!(
        "From: Essai MMail <mmail@exemple.fr>\r\n\
         To: test@exemple.fr\r\n\
         Subject: {objet} \u{e9}preuve\r\n\
         Date: Tue, 22 Sep 2026 10:00:00 +0200\r\n\
         Message-ID: <{id}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         Corps d'essai d\u{e9}pos\u{e9} par les \u{e9}preuves de MMail.\r\n"
    );
    (id, brut.into_bytes())
}

const DATE_ESSAI: &str = "22-Sep-2026 10:00:00 +0200";

/// Retire d'un dossier tout message portant un `Message-ID` donné.
fn nettoyer(client: &mut Client, chemin: &str, message_id: &str) {
    client.selectionner(chemin, None).expect("SELECT pour nettoyage");
    let uids = client.chercher_message_id(message_id).expect("SEARCH pour nettoyage");
    client.supprimer(&uids).expect("purge de nettoyage");
}

fn compter(client: &mut Client, chemin: &str, message_id: &str) -> Vec<u32> {
    client.examiner(chemin).expect("EXAMINE");
    client.chercher_message_id(message_id).expect("SEARCH")
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn capacites_attendues() {
    let client = session();
    for capacite in ["QRESYNC", "CONDSTORE", "UIDPLUS", "MOVE", "SPECIAL-USE", "IDLE"] {
        assert!(client.sait(capacite), "{capacite} manquante");
    }
    println!("capacités : {}", client.capacites().join(" "));
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn arborescence_et_roles() {
    let mut client = session();
    let dossiers = client.dossiers().expect("LIST");
    println!("{} dossiers", dossiers.len());
    for d in &dossiers {
        println!("  {:<24} profondeur {} rôle {:?}", d.chemin, d.profondeur(), d.role());
    }
    assert!(dossiers.iter().any(|d| d.chemin == "INBOX"));
    // Les rôles viennent de SPECIAL-USE : c'est ce qui permet de placer
    // « Envoyés » et « Corbeille » sans deviner d'après leur nom.
    assert!(dossiers.iter().any(|d| d.role() == Some("Sent")));
    assert!(dossiers.iter().any(|d| d.role() == Some("Trash")));
    // Un sous-dossier doit être reconnu comme tel.
    assert!(dossiers.iter().any(|d| d.profondeur() == 1));
}

/// Boîte partagée (0.6.8) : B partage sa boîte de réception avec A
/// (`SETACL`, RFC 4314) ; A la reconnaît à la relecture de son arborescence,
/// puis plus du tout une fois le partage retiré.
#[test]
#[ignore = "exige un serveur IMAP à boîtes partagées et deux comptes"]
fn boite_partagee_reconnue() {
    let ua = variable("MMAIL_UTILISATEUR");
    let ub = variable("MMAIL_UTILISATEUR2");
    let mut b = session2();
    b.brute(&format!("SETACL INBOX \"{ua}\" lr")).expect("SETACL");
    let magasin = Magasin::en_memoire().unwrap();
    let ca = magasin.compte(&ua, &variable("MMAIL_HOTE"), 993, &ua).unwrap();
    let partagees = |a: &mut Client| {
        synchro::arborescence(a, &magasin, ca).map(|_| magasin.compte_par_id(ca).unwrap().unwrap().partagees)
    };
    let pendant = partagees(&mut session());
    // Retiré avant tout constat : un échec ne laisse pas le partage en place.
    b.brute(&format!("DELETEACL INBOX \"{ua}\"")).expect("DELETEACL");
    let apres = partagees(&mut session());

    println!("partagées : {pendant:?}, puis {apres:?}");
    let pendant = pendant.expect("arborescence pendant le partage");
    assert_eq!(pendant.len(), 1, "{pendant:?}");
    assert!(pendant[0].eq_ignore_ascii_case(&ub), "{pendant:?}");
    assert!(apres.expect("arborescence après le partage").is_empty());
}

/// Catégories (0.6.15) sur une boîte partagée comme chez Mailcow : B partage
/// sa boîte de réception avec A (droits de contact@ chez Lalanne). A crée une
/// catégorie dans le catalogue de la boîte (`METADATA`) et la pose sur un
/// message ; B relit le catalogue et voit le mot-clé, que la recherche trouve.
#[test]
#[ignore = "exige un serveur IMAP à boîtes partagées et métadonnées, et deux comptes"]
fn categories_sur_boite_partagee() {
    use crate::categories::{Categorie, Catalogue, Changement, ENTREE};
    let ua = variable("MMAIL_UTILISATEUR");
    let mut b = session2();
    assert!(b.sait("METADATA"), "le serveur ne garde pas de métadonnées");
    let (id_message, brut) = message_essai("Catégories");
    let uid_b = b.deposer("INBOX", &[], DATE_ESSAI, &brut).unwrap().unwrap();
    b.brute(&format!("SETACL INBOX \"{ua}\" lrswiptek")).expect("SETACL");
    let resultat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut a = session();
        let (magasin, ca, cb) = index(&mut a, Some(&mut b));
        // La boîte de réception partagée : « Shared/<B>/INBOX » chez Mailcow
        // (`mail_shared_explicit_inbox`), « Shared/<B> » sinon (CI).
        let dossiers: Vec<String> = magasin.dossiers(ca).unwrap().into_iter()
            .filter(|d| d.selectionnable && d.chemin.starts_with("Shared/"))
            .map(|d| d.chemin)
            .collect();
        let partagee = dossiers.iter().find(|c| c.ends_with("/INBOX"))
            .or_else(|| dossiers.iter().find(|c| !c["Shared/".len()..].contains('/')))
            .cloned()
            .expect("boîte partagée vue par A");
        let mot_cle = format!("epreuve_{uid_b}");
        let categorie = Categorie { mot_cle: mot_cle.clone(), nom: "Épreuve à faire".into(), couleur: "#0B8043".into() };
        synchro::changer_catalogue(&mut a, &magasin, ca, &partagee, &Changement::Poser(categorie.clone()))
            .expect("catalogue écrit par A");
        // B relit le catalogue de sa propre boîte de réception.
        assert!(synchro::relire_catalogue(&mut b, &magasin, cb, "INBOX").expect("relecture par B"));
        let (valeur, serveur) = magasin.catalogue(cb, "INBOX").unwrap().unwrap();
        assert!(serveur);
        assert!(Catalogue::lire(&valeur).liste().contains(&categorie), "{valeur}");
        // A pose la catégorie sur le message de B, à travers le partage.
        synchro::synchroniser(&mut a, &magasin, ca, &partagee).expect("synchro A");
        a.selectionner(&partagee, None).unwrap();
        let uid_a = a.chercher_message_id(&id_message).unwrap()[0];
        synchro::marquer_categorie(&mut a, &magasin, ca, &partagee, &[uid_a], &mot_cle, true).expect("STORE par A");
        synchro::synchroniser(&mut b, &magasin, cb, "INBOX").expect("synchro B");
        let id_b = magasin.dossier_id(cb, "INBOX").unwrap();
        assert!(magasin.message(id_b, uid_b).unwrap().unwrap().mots_cles.split(' ').any(|m| m == mot_cle));
        assert_eq!(b.chercher("", &[vec![mot_cle.clone()]]).unwrap(), vec![uid_b]);
        b.ecrire_metadonnee("INBOX", ENTREE, None).expect("catalogue effacé");
    }));
    // Retirés avant tout constat : un échec ne laisse ni partage ni message.
    b.brute(&format!("DELETEACL INBOX \"{ua}\"")).expect("DELETEACL");
    nettoyer(&mut b, "INBOX", &id_message);
    if let Err(e) = resultat {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn sous_dossier_cree_et_abonne() {
    let mut client = session();
    let dossiers = client.dossiers().expect("LIST");
    let separateur = dossiers.iter().find(|d| d.chemin == "INBOX").expect("INBOX").separateur.clone();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let nom = format!("Épreuve MMail {unique}");
    let chemin = crate::protocole::chemin_de_sous_dossier("INBOX", &separateur, &nom).expect("chemin");

    client.creer(&chemin).expect("CREATE");
    client.abonner(&chemin).expect("SUBSCRIBE");
    let liste = client.dossiers().expect("LIST après création");
    let abonnes = client.brute("LSUB \"\" \"*\"").expect("LSUB");
    let magasin = Magasin::en_memoire().unwrap();
    let u = variable("MMAIL_UTILISATEUR");
    let compte = magasin.compte(&u, &variable("MMAIL_HOTE"), 993, &u).unwrap();
    synchro::arborescence(&mut client, &magasin, compte).expect("arborescence");
    let dans_l_index = magasin.dossiers(compte).unwrap().into_iter().find(|d| d.chemin == chemin);
    // Retiré avant tout constat : un échec ne laisse rien dans la boîte.
    client.supprimer_dossier(&chemin).expect("DELETE");
    let apres = client.dossiers().expect("LIST après retrait");

    println!("créé puis retiré : {chemin}");
    assert!(liste.iter().any(|d| d.chemin == chemin), "absent de LIST");
    assert!(abonnes.lines().any(|l| l.contains(&chemin)), "pas abonné : {abonnes}");
    let d = dans_l_index.expect("absent de l'index");
    assert_eq!(d.nom, nom, "nom accentué relu tel quel");
    assert_eq!(d.profondeur, 1);
    assert!(!apres.iter().any(|d| d.chemin == chemin), "dossier d'épreuve resté sur le serveur");
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn index_et_message_accentue() {
    let mut client = session();
    let etat = client.examiner("INBOX").expect("EXAMINE");
    println!("INBOX : {etat:?}");
    assert!(etat.messages > 0, "la boîte d'essai doit contenir des messages");
    assert!(etat.uid_validity > 0);
    assert!(etat.uid_next > etat.messages);
    // HIGHESTMODSEQ conditionne la synchronisation incrémentale à venir.
    assert!(etat.highest_mod_seq > 0, "CONDSTORE annoncé mais pas de HIGHESTMODSEQ");

    let entetes = client.entetes("1:*").expect("FETCH");
    assert_eq!(entetes.len() as u32, etat.messages);
    let premier = &entetes[0];
    println!(
        "premier message : uid {} taille {} drapeaux {:?}",
        premier.uid, premier.taille, premier.drapeaux
    );
    assert!(premier.taille > 0);
    assert!(!premier.date_interne.is_empty());

    // Les en-têtes bruts doivent porter le sujet, accents compris une fois
    // décodés : c'est mail-parser qui s'en charge côté index.
    let brut = String::from_utf8_lossy(&premier.brut);
    assert!(brut.contains("Subject:"), "en-têtes sans sujet : {brut}");

    let corps = client.corps(premier.uid).expect("BODY.PEEK[]");
    assert!(corps.len() >= premier.taille as usize / 2);
    let texte = String::from_utf8_lossy(&corps);
    assert!(texte.contains("Subject:"));
    println!("corps de {} octets", corps.len());

    // BODY.PEEK ne doit pas avoir marqué le message comme lu.
    let apres = client.entetes(&premier.uid.to_string()).expect("FETCH 2");
    assert_eq!(apres[0].lu(), premier.lu(), "la lecture a changé le drapeau \\Seen");
    client.fermer();
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn synchronisation_incrementale() {
    let mut a = session();
    let mut autre = session();
    assert!(a.qresync(), "QRESYNC doit être activé sur Dovecot");
    let (magasin, compte, _) = index(&mut a, None);
    let id = magasin.dossier_id(compte, "INBOX").unwrap();

    // L'arborescence relue avec ses compteurs garde les rôles SPECIAL-USE.
    let dossiers = magasin.dossiers(compte).unwrap();
    for role in ["Sent", "Trash", "Drafts", "Junk"] {
        assert!(dossiers.iter().any(|d| d.role == role), "rôle {role} perdu : {dossiers:?}");
    }
    assert!(dossiers.iter().any(|d| d.messages > 0), "compteurs absents");

    // Première visite : relecture complète, compteurs cohérents.
    let premier = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 1");
    assert!(premier.complet);
    let dossier = magasin.dossier(id).unwrap().unwrap();
    assert_eq!(dossier.messages as usize, magasin.uids(id).unwrap().len());

    // Seconde visite sans changement : reprise QRESYNC, rien à relire.
    let second = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 2");
    assert!(!second.complet, "la reprise doit être incrémentale : {second:?}");
    assert_eq!((second.nouveaux, second.retires), (0, 0));

    // Un message arrive par une autre session.
    let (message_id, brut) = message_essai("Synchronisation");
    let uid = autre
        .deposer("INBOX", &["\\Seen".into()], DATE_ESSAI, &brut)
        .expect("APPEND")
        .expect("APPENDUID");
    let troisieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 3");
    assert!(!troisieme.complet);
    assert_eq!(troisieme.nouveaux, 1);
    let ligne = magasin.message(id, uid).unwrap().expect("message indexé");
    assert_eq!(ligne.message_id, message_id);
    assert!(ligne.lu);
    assert!(ligne.sujet.contains("épreuve"), "sujet : {}", ligne.sujet);
    assert_eq!(Some(ligne.horodatage), secondes_date_interne(DATE_ESSAI));

    // Un drapeau change ailleurs : QRESYNC le rend.
    autre.selectionner("INBOX", None).unwrap();
    autre.marquer(&[uid], "\\Seen", false).unwrap();
    let quatrieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 4");
    assert!(!quatrieme.complet);
    assert!(!magasin.message(id, uid).unwrap().unwrap().lu, "\\Seen retiré ailleurs");

    // Marquage depuis le client : serveur et index suivent.
    synchro::marquer_lu(&mut a, &magasin, compte, "INBOX", &[uid], true).unwrap();
    assert!(magasin.message(id, uid).unwrap().unwrap().lu);

    // Le message disparaît ailleurs : VANISHED le retire de l'index.
    autre.supprimer(&[uid]).expect("purge");
    let cinquieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 5");
    assert!(!cinquieme.complet);
    assert!(cinquieme.retires >= 1);
    assert!(magasin.message(id, uid).unwrap().is_none());
    assert_eq!(magasin.uids(id).unwrap().len(), premier_compte(&magasin, id));
}

/// Réponse et transfert faits sur le téléphone : l'autre session pose
/// `\Answered` et `$Forwarded`, la reprise QRESYNC les rend à l'index.
#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn repondu_et_transfere_poses_par_un_autre_logiciel() {
    let mut a = session();
    let mut telephone = session();
    let (magasin, compte, _) = index(&mut a, None);
    let id = magasin.dossier_id(compte, "INBOX").unwrap();
    synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 1");

    let (message_id, brut) = message_essai("Transfert ailleurs");
    let uid = telephone
        .deposer("INBOX", &["\\Seen".into()], DATE_ESSAI, &brut)
        .expect("APPEND")
        .expect("APPENDUID");
    synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 2");
    let ligne = magasin.message(id, uid).unwrap().expect("message indexé");
    assert!(!ligne.repondu && !ligne.transfere);

    telephone.selectionner("INBOX", None).unwrap();
    telephone.marquer(&[uid], "$Forwarded", true).unwrap();
    telephone.marquer(&[uid], "\\Answered", true).unwrap();
    let bilan = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 3");
    assert!(!bilan.complet, "la reprise doit suffire : {bilan:?}");
    let ligne = magasin.message(id, uid).unwrap().unwrap();
    assert!(ligne.transfere, "$Forwarded posé ailleurs");
    assert!(ligne.repondu, "\\Answered posé ailleurs");

    // Un index d'avant la 0.5.4 : HIGHESTMODSEQ oublié par la migration, tous
    // les drapeaux sont relus.
    magasin.oublier_modseq(id).unwrap();
    telephone.marquer(&[uid], "\\Answered", false).unwrap();
    synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 4");
    let ligne = magasin.message(id, uid).unwrap().unwrap();
    assert!(ligne.transfere && !ligne.repondu);

    nettoyer(&mut telephone, "INBOX", &message_id);
}

fn premier_compte(magasin: &Magasin, id: i64) -> usize {
    magasin.dossier(id).unwrap().unwrap().messages as usize
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn deplacement_dans_la_boite_et_retour() {
    let mut a = session();
    let (magasin, compte, _) = index(&mut a, None);
    let (message_id, brut) = message_essai("Tri interne");
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, &brut).unwrap().unwrap();
    synchro::synchroniser(&mut a, &magasin, compte, "INBOX").unwrap();
    let inbox = magasin.dossier_id(compte, "INBOX").unwrap();
    assert!(magasin.message(inbox, uid).unwrap().is_some());

    let rapport =
        deplacement::dans_la_boite(&mut a, &magasin, compte, "INBOX", &[uid], "Essais/Clients").unwrap();
    assert_eq!(rapport.deplaces, 1);
    assert!(magasin.message(inbox, uid).unwrap().is_none(), "retiré de l'index source");
    assert!(compter(&mut a, "INBOX", &message_id).is_empty());
    let la = compter(&mut a, "Essais/Clients", &message_id);
    assert_eq!(la.len(), 1);

    // Retour, depuis le dossier cible synchronisé.
    synchro::synchroniser(&mut a, &magasin, compte, "Essais/Clients").unwrap();
    deplacement::dans_la_boite(&mut a, &magasin, compte, "Essais/Clients", &la, "INBOX").unwrap();
    assert!(compter(&mut a, "Essais/Clients", &message_id).is_empty());
    assert_eq!(compter(&mut a, "INBOX", &message_id).len(), 1);
    nettoyer(&mut a, "INBOX", &message_id);
}

#[test]
#[ignore = "exige un serveur IMAP et deux comptes"]
fn deplacement_entre_boites_et_retour() {
    let mut a = session();
    let mut b = session2();
    let mut annexe_b = session2();
    let mut annexe_a = session();
    let (magasin, ca, cb) = index(&mut a, Some(&mut b));
    let atelier = std::env::temp_dir().join(format!("mmail-essai-{}", std::process::id()));

    let (message_id, brut) = message_essai("Tri entre boîtes");
    let drapeaux = vec!["\\Seen".to_string(), "\\Flagged".to_string()];
    let uid = a.deposer("INBOX", &drapeaux, DATE_ESSAI, &brut).unwrap().unwrap();
    synchro::synchroniser(&mut a, &magasin, ca, "INBOX").unwrap();

    // A/INBOX → B/Archive.
    let rapport = deplacement::entre_boites(
        &mut a, &mut annexe_b, &magasin, ca, "INBOX", &[uid], cb, "Archive", &atelier,
    )
    .expect("déplacement A → B");
    assert_eq!(rapport.deplaces, 1, "{rapport:?}");
    assert!(rapport.erreurs.is_empty());
    assert_eq!(magasin.nombre_operations().unwrap(), 0, "aucune opération ne doit rester en file");
    assert!(
        std::fs::read_dir(&atelier).map(|d| d.count()).unwrap_or(0) == 0,
        "la copie de travail doit être effacée"
    );
    assert!(compter(&mut a, "INBOX", &message_id).is_empty(), "purgé de la source");
    let dans_b = compter(&mut b, "Archive", &message_id);
    assert_eq!(dans_b.len(), 1, "présent une fois dans la cible");

    // Drapeaux, date de réception et octets conservés.
    b.selectionner("Archive", None).unwrap();
    let copie = b.message_complet(dans_b[0]).unwrap().expect("message dans B");
    assert!(copie.drapeaux.iter().any(|d| d == "\\Seen"));
    assert!(copie.drapeaux.iter().any(|d| d == "\\Flagged"));
    assert_eq!(secondes_date_interne(&copie.date_interne), secondes_date_interne(DATE_ESSAI));
    assert_eq!(copie.octets, brut, "le message doit arriver octet pour octet");

    // Retour B/Archive → A/INBOX.
    synchro::synchroniser(&mut b, &magasin, cb, "Archive").unwrap();
    let rapport = deplacement::entre_boites(
        &mut b, &mut annexe_a, &magasin, cb, "Archive", &dans_b, ca, "INBOX", &atelier,
    )
    .expect("déplacement B → A");
    assert_eq!(rapport.deplaces, 1);
    assert!(compter(&mut b, "Archive", &message_id).is_empty());
    assert_eq!(compter(&mut a, "INBOX", &message_id).len(), 1);
    nettoyer(&mut a, "INBOX", &message_id);
    let _ = std::fs::remove_dir_all(&atelier);
}

#[test]
#[ignore = "exige un serveur IMAP et deux comptes"]
fn reprise_d_un_deplacement_interrompu_sans_doublon() {
    let mut a = session();
    let mut b = session2();
    let mut annexe_b = session2();
    let (magasin, ca, cb) = index(&mut a, Some(&mut b));
    let atelier = std::env::temp_dir().join(format!("mmail-reprise-{}", std::process::id()));
    std::fs::create_dir_all(&atelier).unwrap();

    // Situation d'une coupure juste après le dépôt : le message est dans la
    // source ET dans la cible, l'opération est restée à l'étape « déposer ».
    let (message_id, brut) = message_essai("Reprise");
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, &brut).unwrap().unwrap();
    b.deposer("Archive", &[], DATE_ESSAI, &brut).unwrap();
    synchro::synchroniser(&mut a, &magasin, ca, "INBOX").unwrap();
    let validite = a.selectionner("INBOX", None).unwrap().etat.uid_validity;
    let fichier = atelier.join("reprise.eml");
    std::fs::write(&fichier, &brut).unwrap();
    let mut operation = Operation {
        id: 0,
        compte_source: ca,
        chemin_source: "INBOX".into(),
        validite_source: validite,
        uid_source: uid,
        compte_cible: cb,
        chemin_cible: "Archive".into(),
        message_id: message_id.clone(),
        drapeaux: String::new(),
        date_interne: DATE_ESSAI.into(),
        fichier: fichier.to_string_lossy().into_owned(),
        etape: Etape::Deposer,
        erreur: String::new(),
    };
    operation.id = magasin.inscrire_operation(&operation).unwrap();

    deplacement::reprendre(&mut a, &mut annexe_b, &magasin, &operation).expect("reprise");
    assert_eq!(compter(&mut b, "Archive", &message_id).len(), 1, "pas de doublon dans la cible");
    assert!(compter(&mut a, "INBOX", &message_id).is_empty(), "source purgée");
    assert_eq!(magasin.nombre_operations().unwrap(), 0);
    assert!(!fichier.exists(), "copie effacée une fois l'opération soldée");

    nettoyer(&mut b, "Archive", &message_id);
    let _ = std::fs::remove_dir_all(&atelier);
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn piece_jointe_relue_octet_pour_octet() {
    let mut a = session();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let message_id = format!("mmail-essai-{unique}@exemple.fr");
    // Contenu binaire couvrant toutes les valeurs d'octet : un décodage
    // base64 approximatif ne passerait pas.
    let contenu: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
    let base64 = base64_simple(&contenu);
    let brut = format!(
        "From: Essai MMail <mmail@exemple.fr>\r\n\
         Subject: Pi\u{e8}ce jointe d'\u{e9}preuve\r\n\
         Message-ID: <{message_id}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"LIMITE\"\r\n\
         \r\n\
         --LIMITE\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         Voir la pi\u{e8}ce jointe.\r\n\
         --LIMITE\r\n\
         Content-Type: application/octet-stream; name=\"donn\u{e9}es.bin\"\r\n\
         Content-Disposition: attachment; filename=\"donnees.bin\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {base64}\r\n\
         --LIMITE--\r\n"
    );
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, brut.as_bytes()).unwrap().unwrap();
    a.selectionner("INBOX", None).unwrap();
    let octets = a.corps(uid).expect("corps");
    let pieces = crate::index::pieces_jointes(&octets);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    assert_eq!(pieces[0].nom, "donnees.bin");
    assert!(!pieces[0].risquee);
    let (_, relu) = crate::index::extraire_piece(&octets, 0).expect("extraction");
    assert_eq!(relu, contenu, "la pièce jointe doit revenir octet pour octet");
    nettoyer(&mut a, "INBOX", &message_id);
}

/// Base64 standard, lignes de 76 caractères (RFC 2045).
fn base64_simple(octets: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut sortie = String::new();
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16
            | (*bloc.get(1).unwrap_or(&0) as u32) << 8
            | *bloc.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= bloc.len() {
                sortie.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                sortie.push('=');
            }
        }
    }
    sortie
        .as_bytes()
        .chunks(76)
        .map(|l| std::str::from_utf8(l).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Envoi par la soumission SMTP de la boîte A vers la boîte B, pièce jointe
/// comprise ; le message doit arriver dans la boîte de réception de B, intact,
/// puis il est retiré. Deux boîtes du même serveur : rien ne sort.
#[test]
#[ignore = "exige un serveur IMAP/SMTP et deux comptes"]
fn envoi_smtp_d_une_boite_a_l_autre() {
    use crate::redaction::{fabriquer, Fichier, Redaction};
    use mail_parser::{MessageParser, MimeHeaders};

    let hote = variable("MMAIL_HOTE");
    let (ua, ub) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_UTILISATEUR2"));
    let maintenant = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let r = Redaction {
        de: ua.clone(),
        nom: "Essai MMail".into(),
        a: ub.clone(),
        objet: format!("Essai d'envoi MMail {maintenant} — accentué"),
        texte: "Bonjour,\n.ligne qui commence par un point\nFin.".into(),
        ..Default::default()
    };
    let pj = [Fichier { nom: "piece jointe é.txt".into(), contenu: "contenu d'essai €".as_bytes().to_vec() }];
    let f = fabriquer(&r, maintenant, &pj).expect("fabrication");
    crate::smtp::envoyer(&hote, &ua, &variable("MMAIL_MOTDEPASSE"), &ua, &f.destinataires, &f.envoi, false)
        .expect("envoi SMTP");

    let mut b = session2();
    let mut trouves = Vec::new();
    for _ in 0..30 {
        b.selectionner("INBOX", None).expect("sélection");
        trouves = b.chercher_message_id(&f.message_id).expect("recherche");
        if !trouves.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    assert_eq!(trouves.len(), 1, "message arrivé dans la boîte B");
    let recu = b.corps(trouves[0]).expect("lecture");
    let m = MessageParser::default().parse(&recu).expect("analyse");
    assert_eq!(m.subject(), Some(r.objet.as_str()));
    assert!(m.body_text(0).unwrap().contains(".ligne qui commence par un point"));
    let piece = m.attachments().next().expect("pièce jointe");
    assert_eq!(piece.attachment_name(), Some("piece jointe é.txt"));
    assert_eq!(piece.contents(), "contenu d'essai €".as_bytes());
    b.supprimer(&trouves).expect("nettoyage");
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn vidage_d_un_dossier_d_indesirables() {
    // Comme le fil de travail : la session a d'abord synchronisé un dossier.
    let mut a = session();
    let (magasin, ca, _) = index(&mut a, None);
    synchro::synchroniser(&mut a, &magasin, ca, "INBOX").expect("synchronisation de INBOX");
    for i in 0..3 {
        let (_, brut) = message_essai(&format!("Indésirable {i}"));
        a.deposer("Junk", &[], DATE_ESSAI, &brut).expect("dépôt dans Junk");
    }
    let debut = std::time::Instant::now();
    let nombre = a.vider("Junk").expect("vidage de Junk");
    println!("vidage : {nombre} message(s) en {:?}", debut.elapsed());
    assert!(nombre >= 3);
    let reste = a.selectionner("Junk", None).expect("SELECT après vidage");
    assert_eq!(reste.etat.messages, 0);
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn reservation_par_deux_postes() {
    // Deux MMail ouverts sur la même boîte relèvent le même message dans la
    // même minute : un seul doit pouvoir le réserver.
    let mut a = session();
    let mut b = session();
    let (id, brut) = message_essai("Réservation");
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, &brut).expect("APPEND").expect("APPENDUID");
    a.selectionner("INBOX", None).expect("SELECT A");
    b.selectionner("INBOX", None).expect("SELECT B");
    let modseq_a = a.champs(&uid.to_string(), "MESSAGE-ID").expect("FETCH A")[0].modseq;
    let modseq_b = b.champs(&uid.to_string(), "MESSAGE-ID").expect("FETCH B")[0].modseq;
    assert!(modseq_a > 0, "MODSEQ non rendu");
    assert_eq!(modseq_a, modseq_b);
    assert!(a.reserver(uid, modseq_a).expect("STORE A"), "le premier poste doit réserver");
    assert!(!b.reserver(uid, modseq_b).expect("STORE B"), "le second poste ne doit pas réserver");
    let vu_par_b = b.champs(&uid.to_string(), "MESSAGE-ID").expect("FETCH B après");
    assert!(vu_par_b[0].reserve(), "la réservation doit se voir des autres postes");
    assert!(!vu_par_b[0].supprime(), "un message réservé n'est pas marqué pour la purge (0.6.4)");
    a.supprimer(&[uid]).expect("purge");
    assert!(compter(&mut a, "INBOX", &id).is_empty());
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn idle_signale_un_nouveau_message() {
    // Une session en IDLE sur INBOX ; une autre y dépose un message : la
    // première doit l'apprendre sans rien demander.
    let mut a = session();
    let mut b = session();
    a.selectionner("INBOX", None).expect("SELECT");
    a.idle_commencer().expect("IDLE");
    let (id, brut) = message_essai("IDLE");
    let debut = std::time::Instant::now();
    let uid = b.deposer("INBOX", &[], DATE_ESSAI, &brut).expect("APPEND").expect("APPENDUID");
    let mut signal = None;
    while debut.elapsed() < std::time::Duration::from_secs(15) {
        if let Some(ligne) = a.idle_attendre(std::time::Duration::from_millis(200)).expect("attente") {
            if ligne.ends_with("EXISTS") {
                signal = Some(ligne);
                break;
            }
        }
    }
    println!("signal {:?} après {:?}", signal, debut.elapsed());
    a.idle_terminer().expect("DONE");
    b.selectionner("INBOX", None).expect("SELECT B");
    b.supprimer(&[uid]).expect("purge");
    assert!(compter(&mut a, "INBOX", &id).is_empty());
    assert!(signal.is_some(), "aucun EXISTS reçu en IDLE");
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn recherche_dans_le_texte() {
    // Un mot qui n'est que dans le corps, et un mot accentué : seul le serveur
    // les trouve (l'index ne porte que les en-têtes).
    let mut client = session();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let id = format!("mmail-recherche-{unique}@exemple.fr");
    let mot = format!("zorglub{}", unique % 100_000);
    let brut = format!(
        "From: Essai <mmail@exemple.fr>\r\nTo: test@exemple.fr\r\nSubject: Recherche\r\n\
         Message-ID: <{id}>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\r\nLe mot {mot} et une échéance.\r\n"
    );
    let uid = client.deposer("INBOX", &[], DATE_ESSAI, brut.as_bytes()).expect("APPEND").expect("APPENDUID");
    client.selectionner("INBOX", None).expect("SELECT");
    let trouves = client.chercher_texte(&mot).expect("SEARCH");
    let accent = client.chercher_texte(&format!("{mot} échéance")).expect("SEARCH accentué");
    client.supprimer(&[uid]).expect("purge");
    assert_eq!(trouves, vec![uid]);
    assert_eq!(accent, vec![uid]);
    assert!(compter(&mut client, "INBOX", &id).is_empty());
}

// ------------------------------------------------------------- agenda

/// Requête directe sur le serveur d'agenda, pour déposer et retirer
/// l'événement d'essai : le client, lui, ne fait que lire.
fn requete_agenda(methode: &str, url: &str, corps: &str) -> u16 {
    let (utilisateur, mot_de_passe) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let jeton = crate::smtp::base64(format!("{utilisateur}:{mot_de_passe}").as_bytes());
    let entetes = [
        ("Authorization", format!("Basic {jeton}")),
        ("Content-Type", "text/calendar; charset=utf-8".to_string()),
    ];
    crate::http::envoyer(&crate::http::Demande {
        methode,
        url,
        accepte: "*/*",
        entetes: &entetes,
        corps: corps.as_bytes(),
        taille_max: 1 << 20,
    })
    .unwrap_or_else(|e| panic!("{methode} {url} : {e}"))
    .statut
}

fn evenement_essai(uid: &str, titre: &str, nombre: u32) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//MMail//Epreuve//FR\r\nBEGIN:VEVENT\r\nUID:{uid}\r\n\
DTSTAMP:20260101T000000Z\r\nDTSTART;TZID=Europe/Paris:20300107T100000\r\nDTEND;TZID=Europe/Paris:20300107T110000\r\n\
RRULE:FREQ=WEEKLY;COUNT={nombre}\r\nSUMMARY:{titre}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

/// Titres et débuts des occurrences de janvier 2030 portant `uid`.
fn occurrences_janvier_2030(magasin: &Magasin, uid: &str) -> Vec<(String, String)> {
    let (de, a) = ("2030-01-01T00:00:00Z".parse().unwrap(), "2030-02-01T00:00:00Z".parse().unwrap());
    magasin
        .evenements_periode(1893456000, 1896134400)
        .unwrap()
        .iter()
        .flat_map(|(_, _, ical)| crate::agenda::occurrences(ical, de, a))
        .filter(|o| o.uid == uid)
        .map(|o| (o.resume, o.debut.format("%Y-%m-%dT%H:%MZ").to_string()))
        .collect()
}

#[test]
#[ignore = "exige un serveur CardDAV et un compte"]
fn carnet_decouvert_synchronise_et_suivi() {
    use crate::caldav::Acces;
    use crate::carddav;
    let hote = variable("MMAIL_HOTE");
    let (utilisateur, mot_de_passe) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let maintenant = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte(&utilisateur, &hote, 993, &utilisateur).unwrap();
    let acces = Acces { utilisateur: &utilisateur, mot_de_passe: &mot_de_passe };
    let synchro = || carddav::synchroniser(&magasin, compte, &hote, &acces, maintenant, false).expect("synchronisation");

    let bilan = synchro();
    assert!(bilan.carnets >= 1, "{bilan:?}");
    let collection = magasin.carddav(compte).unwrap().0;
    assert!(collection.starts_with("https://"), "{collection}");
    let carnets = magasin.carnets(compte).unwrap();
    let (_, carnet, _) = carnets.iter().find(|(_, url, _)| url.ends_with("/personal/")).unwrap_or(&carnets[0]).clone();
    let uid = format!("mmail-epreuve-{maintenant}");
    let url = format!("{carnet}{uid}.vcf");
    let fiche = format!(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:{uid}\r\nFN:Épreuve Carnet {maintenant}\r\nN:Carnet;Épreuve;;;\r\n\
ORG:MMEDIA\r\nEMAIL;TYPE=work:epreuve-{maintenant}@exemple.fr\r\nTEL;TYPE=cell:06 00 00 00 00\r\nEND:VCARD\r\n"
    );
    assert!(matches!(requete_dav("PUT", &url, "text/vcard; charset=utf-8", &fiche), 201 | 204));
    assert!(synchro().lus >= 1);
    let trouves = magasin.fiches_contacts(&format!("epreuve-{maintenant}"), 5).unwrap();
    assert_eq!(trouves.len(), 1, "{trouves:?}");
    assert_eq!(trouves[0].nom, format!("Épreuve Carnet {maintenant}"));
    assert_eq!(trouves[0].organisation, "MMEDIA");
    assert_eq!(trouves[0].telephones, vec!["06 00 00 00 00"]);
    // La saisie la propose en tête.
    let saisie = magasin.contacts(&format!("epreuve-{maintenant}"), 3).unwrap();
    assert_eq!(saisie.first().map(|(_, a)| a.clone()), Some(format!("epreuve-{maintenant}@exemple.fr")));
    // Rien n'a changé : rien n'est relu.
    assert_eq!(synchro().lus, 0);
    assert!(matches!(requete_dav("DELETE", &url, "text/vcard", ""), 200 | 204));
    let bilan = synchro();
    assert!(bilan.retires >= 1, "{bilan:?}");
    assert!(magasin.fiches_contacts(&format!("epreuve-{maintenant}"), 5).unwrap().is_empty());
}

fn requete_dav(methode: &str, url: &str, type_contenu: &str, corps: &str) -> u16 {
    let (utilisateur, mot_de_passe) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let jeton = crate::smtp::base64(format!("{utilisateur}:{mot_de_passe}").as_bytes());
    let entetes = [("Authorization", format!("Basic {jeton}")), ("Content-Type", type_contenu.to_string())];
    crate::http::envoyer(&crate::http::Demande {
        methode,
        url,
        accepte: "*/*",
        entetes: &entetes,
        corps: corps.as_bytes(),
        taille_max: 1 << 20,
    })
    .unwrap_or_else(|e| panic!("{methode} {url} : {e}"))
    .statut
}

#[test]
#[ignore = "exige un serveur CalDAV et un compte"]
fn agenda_decouvert_synchronise_et_suivi() {
    use crate::caldav::{self, Acces};
    let hote = variable("MMAIL_HOTE");
    let (utilisateur, mot_de_passe) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let maintenant = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte(&utilisateur, &hote, 993, &utilisateur).unwrap();
    let acces = Acces { utilisateur: &utilisateur, mot_de_passe: &mot_de_passe };
    let synchro = |insister| caldav::synchroniser(&magasin, compte, &hote, &acces, maintenant, insister).expect("synchronisation");

    let bilan = synchro(false);
    assert!(bilan.agendas >= 1, "{bilan:?}");
    let collection = magasin.caldav(compte).unwrap().0;
    assert!(collection.starts_with("https://"), "{collection}");
    // Rien n'a changé : rien n'est relu.
    assert_eq!(synchro(false).lus, 0);

    let agendas = magasin.agendas().unwrap();
    let agenda = agendas.iter().find(|a| a.adresse.ends_with("/personal/")).unwrap_or(&agendas[0]);
    let uid = format!("mmail-epreuve-{maintenant}");
    let url = format!("{}{uid}.ics", agenda.adresse);
    assert_eq!(requete_agenda("PUT", &url, &evenement_essai(&uid, "Épreuve MMail", 3)), 201);
    let bilan = synchro(false);
    assert!(bilan.lus >= 1, "{bilan:?}");
    // Lundi 7 janvier 2030 à 10 h à Paris, puis chaque semaine : 9 h UTC.
    assert_eq!(
        occurrences_janvier_2030(&magasin, &uid),
        [
            ("Épreuve MMail".to_string(), "2030-01-07T09:00Z".to_string()),
            ("Épreuve MMail".to_string(), "2030-01-14T09:00Z".to_string()),
            ("Épreuve MMail".to_string(), "2030-01-21T09:00Z".to_string()),
        ]
    );

    assert!(matches!(requete_agenda("PUT", &url, &evenement_essai(&uid, "Épreuve modifiée", 2)), 201 | 204));
    assert!(synchro(false).lus >= 1);
    let modifie = occurrences_janvier_2030(&magasin, &uid);
    assert_eq!(modifie.len(), 2);
    assert!(modifie.iter().all(|(titre, _)| titre == "Épreuve modifiée"));

    assert!(matches!(requete_agenda("DELETE", &url, ""), 200 | 204));
    assert!(synchro(false).retires >= 1);
    assert!(occurrences_janvier_2030(&magasin, &uid).is_empty());
}

/// Objet de l'index qui porte `uid`.
fn objet_par_uid(magasin: &Magasin, uid: &str) -> Option<crate::magasin::ObjetLocal> {
    magasin
        .evenements_periode(i64::MIN / 2, i64::MAX / 2)
        .unwrap()
        .into_iter()
        .find(|(_, _, ical)| ical.contains(&format!("UID:{uid}")))
        .and_then(|(id, _, _)| magasin.objet(id).unwrap())
}

fn debuts_fevrier_2030(magasin: &Magasin, uid: &str) -> Vec<String> {
    let (de, a) = ("2030-02-01T00:00:00Z".parse().unwrap(), "2030-03-01T00:00:00Z".parse().unwrap());
    objet_par_uid(magasin, uid)
        .map(|o| crate::agenda::occurrences(&o.ical, de, a).iter().map(|o| o.debut.format("%d %H:%M").to_string()).collect())
        .unwrap_or_default()
}

#[test]
#[ignore = "exige un serveur CalDAV et un compte"]
fn agenda_ecriture_serie_occurrence_conflit_retrait() {
    use crate::caldav::{self, Acces};
    use crate::saisie::{self, Saisie};
    let hote = variable("MMAIL_HOTE");
    let (utilisateur, mot_de_passe) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let maintenant = chrono::Utc::now();
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte(&utilisateur, &hote, 993, &utilisateur).unwrap();
    let acces = Acces { utilisateur: &utilisateur, mot_de_passe: &mot_de_passe };
    let synchro = || caldav::synchroniser(&magasin, compte, &hote, &acces, maintenant.timestamp(), false).expect("synchronisation");
    synchro();
    let agendas = magasin.agendas().unwrap();
    let agenda = agendas.iter().find(|a| a.ecriture && a.adresse.ends_with("/personal/")).unwrap_or(&agendas[0]);
    assert!(agenda.ecriture, "{agenda:?}");
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();

    // Série : chaque lundi de février 2030 à 10 h, rappel un quart d'heure avant.
    let s = Saisie {
        titre: "Épreuve d'écriture MMail".into(),
        debut: "2030-02-04T10:00".into(),
        fin: "2030-02-04T11:00".into(),
        repetition: "WEEKLY".into(),
        jusqu_au: "2030-02-25".into(),
        rappel: 15,
        ..Default::default()
    };
    let (uid, ical) = saisie::creer(&s, paris, maintenant).unwrap();
    let url = format!("{}/{uid}.ics", agenda.adresse.trim_end_matches('/'));
    caldav::deposer(&acces, &url, &ical, None).expect("dépôt");
    synchro();
    assert_eq!(debuts_fevrier_2030(&magasin, &uid), ["04 09:00", "11 09:00", "18 09:00", "25 09:00"]);

    // L'occurrence du 11 déplacée à 14 h ; le même dépôt, ETag périmé, refusé.
    let o = objet_par_uid(&magasin, &uid).unwrap();
    let origine = "2030-02-11T09:00:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap().timestamp();
    let mut so = saisie::saisie_de(&o.ical, Some(origine), paris).unwrap();
    so.debut = "2030-02-11T14:00".into();
    so.fin = "2030-02-11T15:00".into();
    let modifie = saisie::modifier(&o.ical, &so, paris, maintenant).unwrap();
    caldav::deposer(&acces, &url, &modifie, Some(&o.etag)).expect("modification");
    let conflit = caldav::deposer(&acces, &url, &modifie, Some(&o.etag)).unwrap_err();
    assert!(conflit.to_string().contains(caldav::CONFLIT), "{conflit}");
    synchro();
    assert_eq!(debuts_fevrier_2030(&magasin, &uid), ["04 09:00", "11 13:00", "18 09:00", "25 09:00"]);

    // L'occurrence du 18 retirée, puis toute la série.
    let o = objet_par_uid(&magasin, &uid).unwrap();
    let origine = "2030-02-18T09:00:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap().timestamp();
    let retire = saisie::retirer_occurrence(&o.ical, origine, maintenant).unwrap();
    caldav::deposer(&acces, &url, &retire, Some(&o.etag)).expect("retrait d'une occurrence");
    synchro();
    assert_eq!(debuts_fevrier_2030(&magasin, &uid), ["04 09:00", "11 13:00", "25 09:00"]);
    let o = objet_par_uid(&magasin, &uid).unwrap();
    caldav::retirer(&acces, &agenda.adresse, &url, Some(&o.etag)).expect("retrait de la série");
    synchro();
    assert!(objet_par_uid(&magasin, &uid).is_none());
}

/// Attend, au plus une minute, un message de la boîte dont le texte contient
/// `motif`, et rend son contenu.
fn attendre_message(client: &mut Client, motif: &str) -> Vec<u8> {
    for _ in 0..30 {
        client.selectionner("INBOX", None).unwrap();
        if let Some(uid) = client.chercher_texte(motif).unwrap().into_iter().max() {
            return client.corps(uid).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    panic!("aucun message contenant « {motif} »");
}

#[test]
#[ignore = "exige un serveur CalDAV à planification (SOGo) et deux comptes"]
fn invitation_acceptee_puis_inscrite_et_annulee() {
    use crate::caldav::{self, Acces};
    use crate::saisie::{self, Saisie};
    let hote = variable("MMAIL_HOTE");
    let (ua, ma) = (variable("MMAIL_UTILISATEUR"), variable("MMAIL_MOTDEPASSE"));
    let (ub, mb) = (variable("MMAIL_UTILISATEUR2"), variable("MMAIL_MOTDEPASSE2"));
    let maintenant = chrono::Utc::now();
    let magasin = Magasin::en_memoire().unwrap();
    let (ca, cb) = (magasin.compte(&ua, &hote, 993, &ua).unwrap(), magasin.compte(&ub, &hote, 993, &ub).unwrap());
    let (aa, ab) = (Acces { utilisateur: &ua, mot_de_passe: &ma }, Acces { utilisateur: &ub, mot_de_passe: &mb });
    let synchro = |compte, acces: &Acces| caldav::synchroniser(&magasin, compte, &hote, acces, maintenant.timestamp(), false).expect("synchronisation");
    synchro(ca, &aa);
    synchro(cb, &ab);
    assert_eq!(magasin.planification(ca).unwrap(), Some(true), "le serveur doit distribuer les invitations");
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    let agenda_de = |compte| magasin.agendas().unwrap().into_iter().find(|a| a.compte == compte && a.ecriture).unwrap();

    // A invite B : le serveur envoie l'invitation et la dépose chez B.
    let titre = format!("Épreuve d'invitation MMail {}", maintenant.timestamp());
    let s = Saisie {
        titre: titre.clone(),
        debut: "2030-03-04T10:00".into(),
        fin: "2030-03-04T11:00".into(),
        rappel: -1,
        participants: Some(vec![ub.clone()]),
        organisateur: ua.clone(),
        ..Default::default()
    };
    let (uid, ical) = saisie::creer(&s, paris, maintenant).unwrap();
    let url_a = format!("{}/{uid}.ics", agenda_de(ca).adresse.trim_end_matches('/'));
    caldav::deposer(&aa, &url_a, &ical, None).expect("dépôt de la réunion");
    let mut b = session2();
    let courriel = attendre_message(&mut b, &maintenant.timestamp().to_string());
    let invitation = crate::invitation::extraire(&courriel).expect("partie calendrier de l'invitation");
    assert_eq!(invitation.methode, "REQUEST");
    synchro(cb, &ab);
    let chez_b = magasin.objet_par_uid(cb, &uid).unwrap().expect("réunion déposée chez l'invité");
    assert_eq!(crate::invitation::reponse_de(&chez_b.ical, &ub), "NEEDS-ACTION");

    // B accepte : le serveur envoie la réponse à A, qui l'inscrit dans sa copie.
    let accepte = crate::invitation::repondre(&invitation.ical, Some(&chez_b.ical), &ub, "ACCEPTED").unwrap();
    let url_b = caldav::adresse(&agenda_de(cb).adresse, &chez_b.href).unwrap();
    caldav::deposer(&ab, &url_b, &accepte, Some(&chez_b.etag)).expect("réponse de l'invité");
    let mut a = session();
    let reponse = attendre_message(&mut a, &maintenant.timestamp().to_string());
    let reponse = crate::invitation::extraire(&reponse).expect("partie calendrier de la réponse");
    assert_eq!(reponse.methode, "REPLY");
    // Invité du même serveur, répondant sur la copie que le serveur lui a
    // déposée : SOGo reporte déjà la réponse chez l'organisateur. Sinon — un
    // invité d'ailleurs, dont la réponse arrive par courriel —, MMail l'inscrit.
    synchro(ca, &aa);
    let chez_a = magasin.objet_par_uid(ca, &uid).unwrap().unwrap();
    if let Some(inscrite) = crate::invitation::inscrire_reponse(&chez_a.ical, &reponse.ical).unwrap() {
        caldav::deposer(&aa, &url_a, &inscrite, Some(&chez_a.etag)).expect("inscription de la réponse");
    }
    synchro(ca, &aa);
    let chez_a = magasin.objet_par_uid(ca, &uid).unwrap().unwrap();
    assert_eq!(crate::invitation::reponse_de(&chez_a.ical, &ub), "ACCEPTED");

    // A annule : la réunion disparaît de chez B.
    caldav::retirer(&aa, &agenda_de(ca).adresse, &url_a, Some(&chez_a.etag)).expect("annulation");
    std::thread::sleep(std::time::Duration::from_secs(5));
    synchro(cb, &ab);
    assert!(magasin.objet_par_uid(cb, &uid).unwrap().is_none(), "réunion encore chez l'invité");
    // Les courriels de l'épreuve — invitation, réponse, annulation — retirés.
    let motif = maintenant.timestamp().to_string();
    for client in [&mut a, &mut b] {
        client.selectionner("INBOX", None).unwrap();
        let uids = client.chercher_texte(&motif).unwrap();
        client.supprimer(&uids).expect("purge de nettoyage");
    }
}
