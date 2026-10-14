// SPDX-License-Identifier: GPL-3.0-or-later
//! Sauvegarde de la configuration hors du poste (0.5.10, issue #29).
//!
//! La configuration — comptes et mots de passe, identités et signatures avec
//! leurs images, dossiers masqués et favoris, agendas affichés, réglages —
//! part vers le service de sauvegarde de M-Media, chiffrée ici : le service ne
//! reçoit ni la clé ni rien de lisible.
//!
//! La clé se tire de l'adresse et du mot de passe d'un compte (PBKDF2,
//! `TOURS` tours). Un poste neuf qui reçoit un compte — lien de
//! configuration, saisie — retrouve donc la sauvegarde sans code à garder.
//! Chaque compte en porte une copie, chiffrée par sa propre clé ; un
//! changement de mot de passe rend illisibles les copies antérieures, et la
//! suivante se fait avec le nouveau.
//!
//! Chaque poste range sa copie sous son propre numéro, tiré au hasard : deux
//! postes ne s'écrasent pas, et la restauration propose le poste à reprendre.
//!
//! Le service (`SERVICE`) :
//! - `PUT <service>/<identifiant>/<poste>` dépose une copie ;
//! - `GET <service>/<identifiant>` liste les postes,
//!   `[{"poste", "date", "taille"}]` ;
//! - `GET <service>/<identifiant>/<poste>` rend la dernière copie du poste.

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aws_lc_rs::rand::SecureRandom;
use aws_lc_rs::{aead, digest, hmac, pbkdf2, rand};
use serde::{Deserialize, Serialize};

use crate::http;
use crate::imap::{Erreur, Resultat};
use crate::magasin::Magasin;

/// Service de sauvegarde de M-Media. `MMAIL_SAUVEGARDES` le remplace pour les
/// essais — en HTTPS, comme lui.
const SERVICE: &str = "https://agent.mmedia.fr/mmail/sauvegardes";

/// Tours de PBKDF2-HMAC-SHA256 : le seuil que l'OWASP recommande. Le calcul
/// prend une fraction de seconde par compte et par session ; il protège un
/// mot de passe faible d'un essai exhaustif sur une copie dérobée.
const TOURS: u32 = 600_000;

/// En-tête d'une copie chiffrée : format, version 1.
const ENTETE: &[u8; 4] = b"MMS1";

/// Taille au-delà de laquelle une copie n'est ni envoyée ni lue : une image
/// de signature est bornée à 5 Mo.
pub const TAILLE_MAX: usize = 16 * 1024 * 1024;

/// Repère qui remplace, dans les réglages sauvegardés, l'adresse du dossier
/// des signatures : elle dépend du poste et de la session.
const REPERE_SIGNATURES: &str = "mmail-sauvegarde:signatures";

pub fn service() -> String {
    std::env::var("MMAIL_SAUVEGARDES")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| v.starts_with("https://"))
        .unwrap_or_else(|| SERVICE.to_string())
}

// ------------------------------------------------------------------ clés

/// Clés d'un compte : l'identifiant de ses copies chez le service, et la clé
/// qui les chiffre. Les deux se tirent du même secret ; le premier ne livre
/// rien du second.
#[derive(Clone)]
pub struct Cles {
    pub identifiant: String,
    cle: [u8; 32],
}

/// Clés d'un compte, tirées de son adresse et de son mot de passe.
pub fn deriver(adresse: &str, mot_de_passe: &str) -> Cles {
    let sel = format!("mmail-sauvegarde-1|{}", adresse.trim().to_lowercase());
    let mut maitre = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(TOURS).expect("tours non nuls"),
        sel.as_bytes(),
        mot_de_passe.as_bytes(),
        &mut maitre,
    );
    let k = hmac::Key::new(hmac::HMAC_SHA256, &maitre);
    let identifiant = hexa(hmac::sign(&k, b"identifiant").as_ref());
    let mut cle = [0u8; 32];
    cle.copy_from_slice(hmac::sign(&k, b"chiffrement").as_ref());
    Cles { identifiant, cle }
}

/// Comme `deriver`, une seule fois par adresse et mot de passe le temps de la
/// session : la sauvegarde se refait à chaque changement de configuration.
pub fn cles(adresse: &str, mot_de_passe: &str) -> Cles {
    static CACHE: OnceLock<Mutex<HashMap<Vec<u8>, Cles>>> = OnceLock::new();
    let repere = digest::digest(
        &digest::SHA256,
        format!("{}\0{}", adresse.trim().to_lowercase(), mot_de_passe).as_bytes(),
    )
    .as_ref()
    .to_vec();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(c) = cache.lock().ok().and_then(|c| c.get(&repere).cloned()) {
        return c;
    }
    let c = deriver(adresse, mot_de_passe);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(repere, c.clone());
    }
    c
}

fn hexa(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Numéro d'un poste, tiré au hasard une fois pour toutes.
pub fn nouveau_poste() -> String {
    let mut octets = [0u8; 16];
    let _ = rand::SystemRandom::new().fill(&mut octets);
    hexa(&octets)
}

/// Numéro de poste tel que le service l'accepte : 32 chiffres hexadécimaux.
pub fn poste_valide(poste: &str) -> bool {
    poste.len() == 32 && poste.bytes().all(|o| o.is_ascii_hexdigit() && !o.is_ascii_uppercase())
}

// ------------------------------------------------------------ chiffrement

/// Chiffre une copie (AES-256-GCM) : `ENTETE`, aléa de 12 octets, texte
/// chiffré et son sceau. L'identifiant est authentifié avec : une copie
/// déplacée sous un autre identifiant ne s'ouvre pas.
pub fn chiffrer(cles: &Cles, clair: &[u8]) -> Resultat<Vec<u8>> {
    let mut alea = [0u8; aead::NONCE_LEN];
    rand::SystemRandom::new()
        .fill(&mut alea)
        .map_err(|_| Erreur::Protocole("aléa indisponible".into()))?;
    let cle = aead::UnboundKey::new(&aead::AES_256_GCM, &cles.cle)
        .map(aead::LessSafeKey::new)
        .map_err(|_| Erreur::Protocole("clé de chiffrement refusée".into()))?;
    let mut corps = clair.to_vec();
    cle.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(alea),
        aead::Aad::from(cles.identifiant.as_bytes()),
        &mut corps,
    )
    .map_err(|_| Erreur::Protocole("chiffrement impossible".into()))?;
    let mut copie = Vec::with_capacity(ENTETE.len() + alea.len() + corps.len());
    copie.extend_from_slice(ENTETE);
    copie.extend_from_slice(&alea);
    copie.extend_from_slice(&corps);
    Ok(copie)
}

/// Texte d'une copie, ou `None` si elle n'est pas lisible avec ces clés —
/// autre mot de passe, copie abîmée.
pub fn dechiffrer(cles: &Cles, copie: &[u8]) -> Option<Vec<u8>> {
    let debut = ENTETE.len() + aead::NONCE_LEN;
    if copie.len() < debut + aead::AES_256_GCM.tag_len() || &copie[..ENTETE.len()] != ENTETE {
        return None;
    }
    let alea = aead::Nonce::try_assume_unique_for_key(&copie[ENTETE.len()..debut]).ok()?;
    let cle = aead::UnboundKey::new(&aead::AES_256_GCM, &cles.cle).map(aead::LessSafeKey::new).ok()?;
    let mut corps = copie[debut..].to_vec();
    let clair = cle.open_in_place(alea, aead::Aad::from(cles.identifiant.as_bytes()), &mut corps).ok()?;
    Some(clair.to_vec())
}

// --------------------------------------------------------------- document

/// Ce qu'une copie contient, en clair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Document {
    /// Toujours « sauvegarde » : de quoi reconnaître le document.
    pub mmail: String,
    pub version: u32,
    /// Secondes depuis 1970.
    pub date: i64,
    /// Nom du poste, pour le choix à la restauration.
    pub poste: String,
    /// Version de MMail qui l'a écrit.
    pub programme: String,
    pub comptes: Vec<CompteSauve>,
    /// Réglages de l'interface, par catégorie : `{"interface": {…}, …}`.
    pub reglages: serde_json::Value,
    /// Images des signatures, en base64 : `"<dossier du compte>/<fichier>"`.
    #[serde(default)]
    pub images: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CompteSauve {
    pub adresse: String,
    pub hote: String,
    /// Serveur d'envoi, s'il n'est pas celui de la lecture ; absent sinon,
    /// et dans les sauvegardes antérieures à la 0.5.11.
    #[serde(default, rename = "hoteEnvoi", skip_serializing_if = "String::is_empty")]
    pub hote_envoi: String,
    /// Mode d'authentification : `microsoft` pour un compte Microsoft, absent
    /// pour un mot de passe (et dans les sauvegardes antérieures à la 0.5.13).
    /// Un compte Microsoft ne garde ni mot de passe ni jeton : il se reconnecte
    /// à Microsoft sur le poste où la configuration est remise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub auth: String,
    pub port: u16,
    pub utilisateur: String,
    /// Absent quand l'utilisateur ne garde pas ses mots de passe, ou que le
    /// compte n'était pas connecté : il sera demandé.
    #[serde(default, rename = "motDePasse", skip_serializing_if = "Option::is_none")]
    pub mot_de_passe: Option<String>,
    #[serde(default)]
    pub replie: bool,
    #[serde(default)]
    pub dossiers: Vec<DossierSauve>,
    #[serde(default)]
    pub agendas: Vec<AgendaSauve>,
}

/// Un dossier dont l'état local diffère de celui d'un dossier neuf : masqué,
/// favori ou replié. Les autres reviennent seuls avec le serveur.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DossierSauve {
    pub chemin: String,
    pub nom: String,
    pub separateur: String,
    pub profondeur: u32,
    #[serde(default)]
    pub role: String,
    #[serde(default = "vrai")]
    pub selectionnable: bool,
    #[serde(default)]
    pub masque: bool,
    #[serde(default)]
    pub favori: Option<i64>,
    #[serde(default)]
    pub replie: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgendaSauve {
    pub adresse: String,
    pub nom: String,
    #[serde(default)]
    pub couleur: String,
    #[serde(default = "vrai")]
    pub ecriture: bool,
    pub affiche: bool,
}

fn vrai() -> bool {
    true
}

/// Compose le document d'un profil. `mots_de_passe` : par adresse, ceux qui
/// partent avec la sauvegarde. `reglages` : l'objet JSON des réglages de
/// l'interface, tel que la fenêtre le donne.
pub fn composer(
    magasin: &Magasin,
    signatures: &Path,
    mots_de_passe: &HashMap<String, String>,
    reglages: &str,
    poste: &str,
    programme: &str,
    date: i64,
) -> Resultat<Document> {
    let agendas = magasin.agendas().map_err(local)?;
    let mut comptes = Vec::new();
    for c in magasin.comptes().map_err(local)? {
        let dossiers = magasin
            .dossiers(c.id)
            .map_err(local)?
            .into_iter()
            .filter(|d| d.masque || d.favori.is_some() || d.replie)
            .map(|d| DossierSauve {
                chemin: d.chemin,
                nom: d.nom,
                separateur: d.separateur,
                profondeur: d.profondeur,
                role: d.role,
                selectionnable: d.selectionnable,
                masque: d.masque,
                favori: d.favori,
                replie: d.replie,
            })
            .collect();
        let agendas = agendas
            .iter()
            .filter(|a| a.compte == c.id)
            .map(|a| AgendaSauve {
                adresse: a.adresse.clone(),
                nom: a.nom.clone(),
                couleur: a.couleur.clone(),
                ecriture: a.ecriture,
                affiche: a.affiche,
            })
            .collect();
        comptes.push(CompteSauve {
            mot_de_passe: mots_de_passe.get(&c.adresse).cloned(),
            adresse: c.adresse,
            hote: c.hote,
            hote_envoi: c.hote_envoi,
            auth: c.auth,
            port: c.port,
            utilisateur: c.utilisateur,
            replie: c.replie,
            dossiers,
            agendas,
        });
    }
    // Les images que les signatures désignent, et elles seules : une image
    // remplacée reste dans le dossier, elle ne part pas.
    let prefixe = crate::rendu::url_fichier(signatures);
    let mut images = BTreeMap::new();
    if let Ok(sous) = std::fs::read_dir(signatures) {
        for compte in sous.flatten().filter(|e| e.path().is_dir()) {
            let Ok(fichiers) = std::fs::read_dir(compte.path()) else { continue };
            for f in fichiers.flatten() {
                let chemin = f.path();
                let utile = chemin.is_file()
                    && reglages.contains(&crate::rendu::url_fichier(&chemin))
                    && f.metadata().map(|m| m.len() <= 5 * 1024 * 1024).unwrap_or(false);
                if !utile {
                    continue;
                }
                if let Ok(octets) = std::fs::read(&chemin) {
                    let nom = format!(
                        "{}/{}",
                        compte.file_name().to_string_lossy(),
                        f.file_name().to_string_lossy()
                    );
                    images.insert(nom, crate::smtp::base64(&octets));
                }
            }
        }
    }
    let reglages: serde_json::Value = serde_json::from_str(&reglages.replace(&prefixe, REPERE_SIGNATURES))
        .map_err(|e| Erreur::Protocole(format!("réglages illisibles : {e}")))?;
    Ok(Document {
        mmail: "sauvegarde".into(),
        version: 1,
        date,
        poste: poste.to_string(),
        programme: programme.to_string(),
        comptes,
        reglages,
        images,
    })
}

/// Remet dans l'index ce qu'un document y garde : les comptes, dans leur
/// ordre, et l'état local de leurs dossiers et de leurs agendas. Un compte
/// déjà présent garde son serveur : la sauvegarde peut être plus ancienne
/// qu'un changement.
pub fn appliquer(magasin: &Magasin, document: &Document) -> Resultat<()> {
    for c in &document.comptes {
        let id = match magasin.compte_connu(&c.adresse).map_err(local)? {
            Some(id) => id,
            None => {
                let id = magasin.compte(&c.adresse, &c.hote, c.port, &c.utilisateur).map_err(local)?;
                magasin.poser_serveur_envoi(&c.adresse, &c.hote_envoi).map_err(local)?;
                magasin.poser_auth(&c.adresse, &c.auth).map_err(local)?;
                id
            }
        };
        magasin.replier_compte(id, c.replie).map_err(local)?;
        for d in &c.dossiers {
            let dossier = crate::magasin::DossierLocal {
                chemin: d.chemin.clone(),
                nom: d.nom.clone(),
                separateur: d.separateur.clone(),
                profondeur: d.profondeur,
                role: d.role.clone(),
                selectionnable: d.selectionnable,
                masque: d.masque,
                favori: d.favori,
                replie: d.replie,
                ..Default::default()
            };
            magasin.restaurer_dossier(id, &dossier).map_err(local)?;
        }
        for a in &c.agendas {
            let agenda = crate::magasin::AgendaLocal {
                id: 0,
                compte: id,
                adresse: a.adresse.clone(),
                nom: a.nom.clone(),
                couleur: a.couleur.clone(),
                ctag: String::new(),
                affiche: a.affiche,
                ecriture: a.ecriture,
            };
            magasin.restaurer_agenda(&agenda).map_err(local)?;
        }
    }
    let ordre: Vec<String> = document.comptes.iter().map(|c| c.adresse.clone()).collect();
    magasin.ordonner_comptes(&ordre).map_err(local)
}

fn local(e: crate::magasin::Erreur) -> Erreur {
    Erreur::Protocole(format!("index local : {e}"))
}

/// Empreinte d'un document, date mise à part : deux sauvegardes de même
/// empreinte sont identiques, la seconde ne part pas.
pub fn empreinte(document: &Document) -> String {
    let mut sans_date = document.clone();
    sans_date.date = 0;
    let texte = serde_json::to_vec(&sans_date).unwrap_or_default();
    hexa(digest::digest(&digest::SHA256, &texte).as_ref())
}

/// Lit le texte d'une copie déchiffrée.
pub fn lire_document(clair: &[u8]) -> Option<Document> {
    let d: Document = serde_json::from_slice(clair).ok()?;
    (d.mmail == "sauvegarde" && d.version == 1).then_some(d)
}

/// Réglages d'un document, l'adresse du dossier des signatures rendue à
/// celle de ce poste.
pub fn reglages_pour(document: &Document, signatures: &Path) -> String {
    document
        .reglages
        .to_string()
        .replace(REPERE_SIGNATURES, &crate::rendu::url_fichier(signatures))
}

/// Remet les images des signatures dans le dossier de ce poste. Un nom qui
/// sortirait du dossier — `..`, chemin absolu — est écarté.
pub fn poser_images(document: &Document, signatures: &Path) -> Resultat<usize> {
    let mut posees = 0;
    for (nom, contenu) in &document.images {
        let Some((compte, fichier)) = nom.split_once('/') else { continue };
        let sur = |p: &str| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.contains(['/', '\\', ':'])
                && !p.chars().any(|c| c.is_control())
        };
        if !sur(compte) || !sur(fichier) {
            continue;
        }
        let Some(octets) = crate::rendu::base64(contenu) else { continue };
        let dossier = signatures.join(compte);
        std::fs::create_dir_all(&dossier)?;
        std::fs::write(dossier.join(fichier), octets)?;
        posees += 1;
    }
    Ok(posees)
}

// ---------------------------------------------------------------- service

/// Une copie rangée chez le service.
#[derive(Debug, Clone, PartialEq)]
pub struct Copie {
    pub poste: String,
    pub date: i64,
    pub taille: u64,
}

/// Dépose la copie d'un poste.
pub fn deposer(cles: &Cles, poste: &str, copie: &[u8]) -> Resultat<()> {
    if copie.len() > TAILLE_MAX {
        return Err(Erreur::Refuse("sauvegarde trop volumineuse".into()));
    }
    let url = format!("{}/{}/{}", service(), cles.identifiant, poste);
    let entetes = [("Content-Type", "application/octet-stream".to_string())];
    let reponse = http::envoyer(&http::Demande {
        methode: "PUT",
        url: &url,
        accepte: "application/json",
        entetes: &entetes,
        corps: copie,
        taille_max: 64 * 1024,
    })?;
    match reponse.statut {
        200 | 201 | 204 => Ok(()),
        statut => Err(Erreur::Refuse(format!("le service de sauvegarde a répondu {statut}"))),
    }
}

/// Copies rangées pour un compte, la plus récente d'abord.
pub fn lister(cles: &Cles) -> Resultat<Vec<Copie>> {
    let url = format!("{}/{}", service(), cles.identifiant);
    let reponse = http::requete("GET", &url, "application/json")?;
    match reponse.statut {
        200 => {}
        404 => return Ok(Vec::new()),
        statut => return Err(Erreur::Refuse(format!("le service de sauvegarde a répondu {statut}"))),
    }
    Ok(analyser_liste(&reponse.corps))
}

pub fn analyser_liste(corps: &[u8]) -> Vec<Copie> {
    let valeur: serde_json::Value = serde_json::from_slice(corps).unwrap_or_default();
    let mut copies: Vec<Copie> = valeur
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|c| {
                    Some(Copie {
                        poste: c["poste"].as_str().filter(|p| poste_valide(p))?.to_string(),
                        date: c["date"].as_i64()?,
                        taille: c["taille"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    copies.sort_by(|a, b| b.date.cmp(&a.date));
    copies
}

/// Dernière copie d'un poste.
pub fn lire(cles: &Cles, poste: &str) -> Resultat<Vec<u8>> {
    let url = format!("{}/{}/{}", service(), cles.identifiant, poste);
    let reponse = http::requete_bornee("GET", &url, "application/octet-stream", TAILLE_MAX)?;
    match reponse.statut {
        200 => Ok(reponse.corps),
        statut => Err(Erreur::Refuse(format!("le service de sauvegarde a répondu {statut}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cles_stables_et_distinctes() {
        let a = deriver("Nom@Exemple.fr", "secret");
        let b = deriver("nom@exemple.fr ", "secret");
        assert_eq!(a.identifiant, b.identifiant, "l'adresse se lit sans casse ni blancs");
        assert_eq!(a.cle, b.cle);
        assert_eq!(a.identifiant.len(), 64);
        let c = deriver("nom@exemple.fr", "autre");
        assert_ne!(a.identifiant, c.identifiant);
        let d = deriver("autre@exemple.fr", "secret");
        assert_ne!(a.identifiant, d.identifiant);
        // L'identifiant ne livre pas la clé.
        assert_ne!(a.identifiant, hexa(&a.cle));
    }

    #[test]
    fn aller_retour_chiffre() {
        let cles = deriver("nom@exemple.fr", "secret");
        let copie = chiffrer(&cles, b"{\"mmail\":\"sauvegarde\"}").unwrap();
        assert_eq!(&copie[..4], b"MMS1");
        assert!(!copie.windows(8).any(|w| w == b"sauvegar"), "rien de lisible");
        assert_eq!(dechiffrer(&cles, &copie).unwrap(), b"{\"mmail\":\"sauvegarde\"}");
        // Deux chiffrements du même texte diffèrent.
        assert_ne!(copie, chiffrer(&cles, b"{\"mmail\":\"sauvegarde\"}").unwrap());
        // Autre mot de passe, copie abîmée, copie tronquée : illisible.
        assert!(dechiffrer(&deriver("nom@exemple.fr", "autre"), &copie).is_none());
        let mut abimee = copie.clone();
        let n = abimee.len();
        abimee[n - 20] ^= 1;
        assert!(dechiffrer(&cles, &abimee).is_none());
        assert!(dechiffrer(&cles, &copie[..20]).is_none());
        // Même clé, autre identifiant : refusée.
        let deplacee = Cles { identifiant: "0".repeat(64), cle: cles.cle };
        assert!(dechiffrer(&deplacee, &copie).is_none());
    }

    #[test]
    fn cache_des_cles() {
        let a = cles("cache@exemple.fr", "secret");
        let b = cles("cache@exemple.fr", "secret");
        assert_eq!(a.identifiant, b.identifiant);
        assert_ne!(a.identifiant, cles("cache@exemple.fr", "autre").identifiant);
    }

    #[test]
    fn numeros_de_poste() {
        let p = nouveau_poste();
        assert!(poste_valide(&p), "{p}");
        assert_ne!(p, nouveau_poste());
        assert!(!poste_valide("../../etc"));
        assert!(!poste_valide(&"A".repeat(32)));
    }

    #[test]
    fn liste_du_service() {
        let p1 = "0".repeat(32);
        let p2 = "a".repeat(32);
        let corps = format!(
            r#"[{{"poste":"{p1}","date":10,"taille":5}},{{"poste":"{p2}","date":20}},{{"poste":"../x","date":30}}]"#
        );
        let copies = analyser_liste(corps.as_bytes());
        assert_eq!(copies.len(), 2, "un numéro invalide est écarté");
        assert_eq!(copies[0].poste, p2, "la plus récente d'abord");
        assert_eq!(copies[1].taille, 5);
        assert!(analyser_liste(b"pas du json").is_empty());
    }

    fn dossier(chemin: &str) -> crate::protocole::Dossier {
        crate::protocole::Dossier { chemin: chemin.into(), separateur: "/".into(), attributs: vec![] }
    }

    #[test]
    fn document_et_signatures() {
        let racine = std::env::temp_dir().join(format!("mmail-sauvegarde-{}", nouveau_poste()));
        let profil = racine.join("profil");
        std::fs::create_dir_all(&profil).unwrap();
        let index = profil.join("index.sqlite");
        let magasin = Magasin::ouvrir(index.to_str().unwrap()).unwrap();
        let compte = magasin.compte("nom@exemple.fr", "mail.exemple.fr", 993, "nom@exemple.fr").unwrap();
        magasin.compte("autre@exemple.fr", "mail.exemple.fr", 993, "autre@exemple.fr").unwrap();
        magasin.compte("jean@contoso.fr", crate::microsoft::HOTE_IMAP, 993, "jean@contoso.fr").unwrap();
        magasin.poser_auth("jean@contoso.fr", crate::magasin::AUTH_MICROSOFT).unwrap();
        magasin
            .poser_dossiers(
                compte,
                &[
                    dossier("INBOX"),
                    dossier("Archives"),
                    dossier("Spam"),
                ],
            )
            .unwrap();
        let archives = magasin.dossier_id(compte, "Archives").unwrap();
        let spam = magasin.dossier_id(compte, "Spam").unwrap();
        magasin.epingler(archives, true).unwrap();
        magasin.masquer(spam, true).unwrap();

        let signatures = racine.join("signatures");
        let images = signatures.join("nom_exemple.fr");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("logo.png"), b"\x89PNG logo").unwrap();
        std::fs::write(images.join("ancien.png"), b"\x89PNG ancien").unwrap();
        let url = crate::rendu::url_fichier(&images.join("logo.png"));
        let identites = serde_json::json!({ "nom@exemple.fr": { "nom": "Nom", "signature": format!("<img src=\"{url}\">") } });
        let reglages = serde_json::json!({
            "redaction": { "identites": identites.to_string(), "miseEnForme": true },
            "interface": { "zoomListe": 1.2 }
        })
        .to_string();

        magasin.poser_serveur_envoi("nom@exemple.fr", "smtp.exemple.fr").unwrap();
        let mut mots = HashMap::new();
        mots.insert("nom@exemple.fr".to_string(), "secret".to_string());
        let doc = composer(&magasin, &signatures, &mots, &reglages, "POSTE", "0.5.10", 100).unwrap();
        assert_eq!(doc.comptes.len(), 3);
        assert_eq!(doc.comptes[2].auth, "microsoft");
        assert_eq!(doc.comptes[2].mot_de_passe, None, "ni mot de passe ni jeton");
        assert_eq!(doc.comptes[0].mot_de_passe.as_deref(), Some("secret"));
        assert_eq!(doc.comptes[1].mot_de_passe, None);
        assert_eq!(doc.comptes[0].hote_envoi, "smtp.exemple.fr");
        let json = serde_json::to_value(&doc.comptes[1]).unwrap();
        assert!(json.get("hoteEnvoi").is_none(), "absent quand c'est le serveur de lecture");
        assert!(json.get("auth").is_none(), "absent pour un mot de passe");
        let chemins: Vec<&str> = doc.comptes[0].dossiers.iter().map(|d| d.chemin.as_str()).collect();
        assert_eq!(chemins, ["Archives", "Spam"], "seuls les dossiers à l'état local");
        assert_eq!(doc.images.keys().collect::<Vec<_>>(), ["nom_exemple.fr/logo.png"], "l'image désignée seule");
        let texte = doc.reglages.to_string();
        assert!(!texte.contains("file://"), "aucun chemin du poste : {texte}");
        assert!(texte.contains(REPERE_SIGNATURES));

        // Même configuration, autre date : même empreinte.
        let doc2 = composer(&magasin, &signatures, &mots, &reglages, "POSTE", "0.5.10", 200).unwrap();
        assert_eq!(empreinte(&doc), empreinte(&doc2));
        mots.clear();
        let doc3 = composer(&magasin, &signatures, &mots, &reglages, "POSTE", "0.5.10", 200).unwrap();
        assert_ne!(empreinte(&doc), empreinte(&doc3));

        // Sur un autre poste : images reposées, adresses rendues à son dossier.
        let ailleurs = racine.join("ailleurs").join("signatures");
        assert_eq!(poser_images(&doc, &ailleurs).unwrap(), 1);
        assert_eq!(std::fs::read(ailleurs.join("nom_exemple.fr/logo.png")).unwrap(), b"\x89PNG logo");
        let rendus = reglages_pour(&doc, &ailleurs);
        assert!(rendus.contains(&crate::rendu::url_fichier(&ailleurs.join("nom_exemple.fr"))), "{rendus}");

        // Aller-retour complet par le chiffrement.
        let cles = deriver("nom@exemple.fr", "secret");
        let copie = chiffrer(&cles, &serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(lire_document(&dechiffrer(&cles, &copie).unwrap()).unwrap(), doc);

        // Restauration dans un profil neuf, qui a déjà reçu le second compte
        // — sur un autre serveur, qu'il garde.
        let neuf = Magasin::ouvrir(racine.join("neuf.sqlite").to_str().unwrap()).unwrap();
        neuf.compte("autre@exemple.fr", "ailleurs.exemple.fr", 993, "autre@exemple.fr").unwrap();
        appliquer(&neuf, &doc).unwrap();
        let comptes = neuf.comptes().unwrap();
        let adresses: Vec<&str> = comptes.iter().map(|c| c.adresse.as_str()).collect();
        assert_eq!(adresses, ["nom@exemple.fr", "autre@exemple.fr", "jean@contoso.fr"], "l'ordre de la sauvegarde");
        assert!(comptes[2].microsoft() && !comptes[0].microsoft(), "le mode d'authentification revient");
        assert_eq!(comptes[1].hote, "ailleurs.exemple.fr");
        assert_eq!(comptes[0].serveur_envoi(), "smtp.exemple.fr", "le serveur d'envoi revient");
        let restaures = neuf.dossiers(comptes[0].id).unwrap();
        let archives = restaures.iter().find(|d| d.chemin == "Archives").unwrap();
        assert!(archives.favori.is_some() && !archives.masque);
        assert!(restaures.iter().find(|d| d.chemin == "Spam").unwrap().masque);
        // La synchronisation garde cet état, et retire ce qui n'existe plus.
        neuf.poser_dossiers(comptes[0].id, &[dossier("INBOX"), dossier("Archives")]).unwrap();
        let apres = neuf.dossiers(comptes[0].id).unwrap();
        assert!(apres.iter().find(|d| d.chemin == "Archives").unwrap().favori.is_some());
        assert!(!apres.iter().any(|d| d.chemin == "Spam"));
        // Les index fermés d'abord : Windows refuse d'effacer un fichier ouvert.
        drop(neuf);
        drop(magasin);
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn images_hors_du_dossier_ecartees() {
        let racine = std::env::temp_dir().join(format!("mmail-sauvegarde-{}", nouveau_poste()));
        let mut images = BTreeMap::new();
        for nom in ["../evasion.png", "a/../../evasion.png", "/abs.png", "a/b/c.png", "a/..", "ok/logo.png"] {
            images.insert(nom.to_string(), crate::smtp::base64(b"x"));
        }
        let doc = Document {
            mmail: "sauvegarde".into(),
            version: 1,
            date: 0,
            poste: String::new(),
            programme: String::new(),
            comptes: Vec::new(),
            reglages: serde_json::json!({}),
            images,
        };
        let signatures = racine.join("signatures");
        assert_eq!(poser_images(&doc, &signatures).unwrap(), 1);
        assert!(signatures.join("ok/logo.png").exists());
        assert!(!racine.join("evasion.png").exists());
        std::fs::remove_dir_all(&racine).unwrap();
    }
}
