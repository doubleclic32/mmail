// SPDX-License-Identifier: GPL-3.0-or-later
//! CardDAV (RFC 6352) : les carnets d'adresses d'une boîte (0.6.1) — SOGo pour
//! Mailcow —, lus et gardés dans l'index. Leurs contacts viennent en tête des
//! adresses proposées à la saisie, et se consultent dans le carnet de MMail.
//! Depuis la 0.6.20, ils s'y créent, s'y modifient et s'y suppriment aussi
//! (cf. `fiche`) : chaque écriture part au serveur sous condition d'ETag, et
//! l'index suit à la synchronisation qui la suit. L'index garde le texte de
//! chaque fiche, de quoi la modifier sans perdre ce que MMail ne montre pas.
//!
//! Même découverte que les agendas (cf. `caldav`) : `/.well-known/carddav`
//! sur le serveur de la boîte, puis l'adresse de SOGo ; de là le principal,
//! puis la collection des carnets (`addressbook-home-set`). Même
//! synchronisation par comparaison : l'étiquette (`getctag`) d'un carnet dit
//! s'il a changé, ses ETag disent quelles fiches relire et lesquelles ont
//! disparu.

use std::collections::HashSet;

use crate::caldav::{adresse, demander, est, index, reponses, texte, Condition, Dav, DEMANDE_ETAGS, DEMANDE_PRINCIPAL};
use crate::imap::{Erreur, Resultat};
use crate::magasin::Magasin;

const DAV: &str = "DAV:";
const CARDDAV: &str = "urn:ietf:params:xml:ns:carddav";
const CS: &str = "http://calendarserver.org/ns/";
/// Fiches demandées par requête `addressbook-multiget`.
const LOT: usize = 100;
/// Délai avant de chercher de nouveau un serveur qui ne s'est pas fait
/// connaître.
const REESSAI_DECOUVERTE: i64 = 24 * 3600;

/// PROPFIND, profondeur 0 sur le principal : la collection des carnets.
pub const DEMANDE_COLLECTION: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav"><d:prop><card:addressbook-home-set/></d:prop></d:propfind>"#;

/// PROPFIND, profondeur 1 sur la collection : les carnets, leur étiquette, et
/// les droits de l'utilisateur sur chacun (RFC 3744).
pub const DEMANDE_CARNETS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/"><d:prop><d:displayname/><d:resourcetype/><cs:getctag/><d:current-user-privilege-set/></d:prop></d:propfind>"#;

/// Type des fiches déposées.
pub const TYPE_VCARD: &str = "text/vcard; charset=utf-8";

/// REPORT `addressbook-multiget` : les fiches désignées, avec leur texte.
pub fn demande_fiches(hrefs: &[String]) -> String {
    let mut corps = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<card:addressbook-multiget xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><card:address-data/></d:prop>"#,
    );
    for href in hrefs {
        corps.push_str("<d:href>");
        corps.push_str(&href.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"));
        corps.push_str("</d:href>");
    }
    corps.push_str("</card:addressbook-multiget>");
    corps
}

#[derive(Clone, Debug, PartialEq)]
pub struct CarnetDistant {
    pub href: String,
    pub nom: String,
    pub ctag: String,
    /// Faux si le serveur dit l'utilisateur sans droit d'écriture (carnet
    /// partagé en lecture, carnet global) ; vrai s'il ne dit rien.
    pub ecriture: bool,
}

impl Default for CarnetDistant {
    fn default() -> Self {
        CarnetDistant { href: String::new(), nom: String::new(), ctag: String::new(), ecriture: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FicheDistante {
    pub href: String,
    pub etag: String,
    pub vcard: Option<String>,
}

/// Ce que MMail garde d'une fiche vCard.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contact {
    pub nom: String,
    pub organisation: String,
    pub courriels: Vec<String>,
    pub telephones: Vec<String>,
}

/// Carnets d'une réponse PROPFIND sur la collection : les collections de type
/// `addressbook`.
pub fn lire_carnets(xml: &str) -> Vec<CarnetDistant> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let mut carnets = Vec::new();
    for (href, proprietes) in reponses(&doc) {
        let mut carnet = CarnetDistant { href, ..Default::default() };
        let mut est_carnet = false;
        let mut annuaire = false;
        for p in proprietes {
            let valeur = p.text().unwrap_or("").trim().to_string();
            if est(&p, DAV, "displayname") {
                carnet.nom = valeur;
            } else if est(&p, CS, "getctag") {
                carnet.ctag = valeur;
            } else if est(&p, DAV, "resourcetype") {
                est_carnet = p.children().any(|t| est(&t, CARDDAV, "addressbook"));
                // Carnet d'annuaire (RFC 6352 § 6.3.2) : la liste globale de
                // SOGo, que l'on ne modifie pas — elle se dit pourtant « all ».
                annuaire = p.children().any(|t| est(&t, CARDDAV, "directory"));
            } else if est(&p, DAV, "current-user-privilege-set") {
                carnet.ecriture = p.descendants().any(|d| {
                    ["write", "write-content", "bind", "all"].iter().any(|n| est(&d, DAV, n))
                });
            }
        }
        if est_carnet {
            carnet.ecriture &= !annuaire;
            if carnet.nom.is_empty() {
                carnet.nom = carnet.href.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
            }
            carnets.push(carnet);
        }
    }
    carnets
}

/// Fiches d'une réponse PROPFIND (ETag seuls) ou `addressbook-multiget`. La
/// collection elle-même, et ce qui n'a pas d'ETag, n'en sont pas.
pub fn lire_fiches(xml: &str, collection: &str) -> Vec<FicheDistante> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let fin = |a: &str| a.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
    let mut fiches = Vec::new();
    for (href, proprietes) in reponses(&doc) {
        // La collection, et les sous-collections, finissent par « / ».
        if href.ends_with('/') || fin(&href) == fin(collection) {
            continue;
        }
        let mut fiche = FicheDistante { href, ..Default::default() };
        for p in proprietes {
            if est(&p, DAV, "getetag") {
                fiche.etag = p.text().unwrap_or("").trim().to_string();
            } else if est(&p, CARDDAV, "address-data") {
                fiche.vcard = Some(p.text().unwrap_or("").to_string());
            }
        }
        if !fiche.etag.is_empty() {
            fiches.push(fiche);
        }
    }
    fiches
}

/// Valeur vCard déséchappée (RFC 6350, 3.4).
fn desechapper(valeur: &str) -> String {
    let mut sortie = String::new();
    let mut car = valeur.chars();
    while let Some(c) = car.next() {
        if c == '\\' {
            match car.next() {
                Some('n') | Some('N') => sortie.push(' '),
                Some(autre) => sortie.push(autre),
                None => {}
            }
        } else {
            sortie.push(c);
        }
    }
    sortie.trim().to_string()
}

/// Lit une fiche vCard (3.0 ou 4.0) : nom affiché (`FN`, à défaut `N`),
/// organisation, adresses de courriel et numéros de téléphone.
pub fn lire_vcard(texte: &str) -> Contact {
    // Lignes repliées : une ligne qui commence par une espace ou une tabulation
    // continue la précédente.
    let mut lignes: Vec<String> = Vec::new();
    for ligne in texte.split('\n') {
        let ligne = ligne.trim_end_matches('\r');
        if (ligne.starts_with(' ') || ligne.starts_with('\t')) && !lignes.is_empty() {
            lignes.last_mut().unwrap().push_str(&ligne[1..]);
        } else {
            lignes.push(ligne.to_string());
        }
    }
    let mut contact = Contact::default();
    let mut nom_structure = String::new();
    for ligne in &lignes {
        let Some((tete, valeur)) = ligne.split_once(':') else { continue };
        // « item1.EMAIL;TYPE=work » : le groupe et les paramètres ne servent pas.
        let propriete = tete.split(';').next().unwrap_or("");
        let propriete = propriete.rsplit('.').next().unwrap_or("").to_ascii_uppercase();
        match propriete.as_str() {
            "FN" => contact.nom = desechapper(valeur),
            "N" => {
                // Nom;Prénom;Autres;Préfixe;Suffixe — un « \; » ne coupe pas.
                let parties: Vec<String> = crate::fiche::composantes(valeur);
                let prenom = parties.get(1).cloned().unwrap_or_default();
                let nom = parties.first().cloned().unwrap_or_default();
                nom_structure = format!("{prenom} {nom}").trim().to_string();
            }
            "ORG" => {
                contact.organisation =
                    crate::fiche::composantes(valeur).into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
            }
            "EMAIL" => {
                let a = desechapper(valeur.trim_start_matches("mailto:"));
                if a.contains('@') && !contact.courriels.iter().any(|c| c.eq_ignore_ascii_case(&a)) {
                    contact.courriels.push(a);
                }
            }
            "TEL" => {
                let t = desechapper(valeur.trim_start_matches("tel:"));
                if !t.is_empty() && !contact.telephones.contains(&t) {
                    contact.telephones.push(t);
                }
            }
            _ => {}
        }
    }
    if contact.nom.is_empty() {
        contact.nom = nom_structure;
    }
    if contact.nom.is_empty() {
        contact.nom = contact.organisation.clone();
    }
    contact
}

/// Adresse de la collection des carnets d'une boîte, ou `None` si aucun
/// serveur CardDAV ne s'est fait connaître.
pub fn decouvrir(hote: &str, dav: &impl Dav) -> Resultat<Option<String>> {
    let departs = [format!("https://{hote}/.well-known/carddav"), format!("https://{hote}/SOGo/dav/")];
    let mut refus = None;
    for depart in departs {
        let essai = (|| -> Resultat<Option<String>> {
            let reponse = demander(dav, "PROPFIND", &depart, "0", DEMANDE_PRINCIPAL)?;
            let Some(principal) = crate::caldav::lire_href(&texte(&reponse), true, "current-user-principal")
                .and_then(|href| adresse(&reponse.adresse, &href))
            else {
                return Ok(None);
            };
            let reponse = demander(dav, "PROPFIND", &principal, "0", DEMANDE_COLLECTION)?;
            Ok(lire_collection(&texte(&reponse)).and_then(|href| adresse(&reponse.adresse, &href)))
        })();
        match essai {
            Ok(Some(collection)) => return Ok(Some(collection)),
            Ok(None) => {}
            Err(e @ Erreur::Refuse(_)) => refus = Some(e),
            Err(_) => {}
        }
    }
    refus.map_or(Ok(None), Err)
}

/// Href de `addressbook-home-set` dans une réponse PROPFIND.
pub fn lire_collection(xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    reponses(&doc)
        .into_iter()
        .find_map(|(_, proprietes)| {
            proprietes.iter().find(|p| est(p, CARDDAV, "addressbook-home-set")).and_then(|p| {
                p.children().find(|n| est(n, DAV, "href")).and_then(|n| n.text()).map(|t| t.trim().to_string())
            })
        })
        .filter(|h| !h.is_empty())
}

#[derive(Debug, Default)]
pub struct Bilan {
    pub change: bool,
    pub carnets: usize,
    pub lus: usize,
    pub retires: usize,
}

/// Synchronise les carnets d'adresses d'un compte avec l'index. `insister` :
/// à la demande de l'utilisateur, la découverte est retentée sans attendre.
pub fn synchroniser(magasin: &Magasin, compte: i64, hote: &str, dav: &impl Dav, maintenant: i64, insister: bool) -> Resultat<Bilan> {
    let mut bilan = Bilan::default();
    let (mut collection, essai) = magasin.carddav(compte).map_err(index)?;
    if collection.is_empty() {
        if !insister && essai > 0 && maintenant - essai < REESSAI_DECOUVERTE {
            return Ok(bilan);
        }
        match decouvrir(hote, dav) {
            Ok(Some(trouvee)) => {
                magasin.poser_carddav(compte, &trouvee, 0).map_err(index)?;
                collection = trouvee;
            }
            Ok(None) => {
                magasin.poser_carddav(compte, "", maintenant).map_err(index)?;
                return Ok(bilan);
            }
            Err(e) => {
                magasin.poser_carddav(compte, "", maintenant).map_err(index)?;
                return Err(e);
            }
        }
    }
    let reponse = dav.envoyer("PROPFIND", &collection, "1", DEMANDE_CARNETS)?;
    match reponse.statut {
        207 => {}
        301 | 302 | 404 | 410 => {
            magasin.poser_carddav(compte, "", 0).map_err(index)?;
            return Err(Erreur::Protocole(format!("collection des carnets introuvable (HTTP {})", reponse.statut)));
        }
        401 | 403 => return Err(Erreur::Refuse(format!("accès aux contacts refusé (HTTP {})", reponse.statut))),
        statut => return Err(Erreur::Protocole(format!("PROPFIND {collection} : HTTP {statut}"))),
    }
    let distants: Vec<CarnetDistant> = lire_carnets(&texte(&reponse))
        .into_iter()
        .filter_map(|mut c| {
            c.href = adresse(&reponse.adresse, &c.href)?;
            Some(c)
        })
        .collect();
    let descriptions: Vec<(String, String, bool)> = distants.iter().map(|c| (c.href.clone(), c.nom.clone(), c.ecriture)).collect();
    bilan.change |= magasin.poser_carnets(compte, &descriptions).map_err(index)?;
    bilan.carnets = distants.len();
    let locaux = magasin.carnets(compte).map_err(index)?;
    for distant in &distants {
        let Some((id, _, ctag)) = locaux.iter().find(|(_, url, _)| *url == distant.href) else { continue };
        if !distant.ctag.is_empty() && distant.ctag == *ctag {
            continue;
        }
        let (lus, retires) = synchroniser_carnet(magasin, *id, &distant.href, dav)?;
        magasin.poser_ctag_carnet(*id, &distant.ctag).map_err(index)?;
        bilan.lus += lus;
        bilan.retires += retires;
        bilan.change |= lus + retires > 0;
    }
    Ok(bilan)
}

fn synchroniser_carnet(magasin: &Magasin, carnet: i64, url: &str, dav: &impl Dav) -> Resultat<(usize, usize)> {
    let reponse = demander(dav, "PROPFIND", url, "1", DEMANDE_ETAGS)?;
    let distantes = lire_fiches(&texte(&reponse), url);
    let locales = magasin.etags_contacts(carnet).map_err(index)?;
    let presentes: HashSet<&str> = distantes.iter().map(|f| f.href.as_str()).collect();
    let a_retirer: Vec<String> = locales.keys().filter(|h| !presentes.contains(h.as_str())).cloned().collect();
    let a_lire: Vec<String> =
        distantes.into_iter().filter(|f| locales.get(&f.href) != Some(&f.etag)).map(|f| f.href).collect();
    magasin.retirer_contacts(carnet, &a_retirer).map_err(index)?;
    let mut lus = 0;
    for lot in a_lire.chunks(LOT) {
        let reponse = demander(dav, "REPORT", url, "1", &demande_fiches(lot))?;
        let fiches: Vec<(String, String, Contact, String)> = lire_fiches(&texte(&reponse), url)
            .into_iter()
            .map(|f| {
                let vcard = f.vcard.unwrap_or_default();
                (f.href, f.etag, lire_vcard(&vcard), vcard)
            })
            .collect();
        lus += fiches.len();
        magasin.poser_contacts(carnet, &fiches).map_err(index)?;
    }
    Ok((lus, a_retirer.len()))
}

// ------------------------------------------------------------- écriture

/// Motif d'une écriture refusée parce que la fiche a changé sur le serveur
/// depuis la dernière lecture.
pub const CONFLIT: &str = "la fiche a été modifiée ailleurs entre-temps";

fn issue_ecriture(reponse: &crate::http::Reponse, creation: bool) -> Resultat<()> {
    match reponse.statut {
        200..=299 => Ok(()),
        412 if creation => Err(Erreur::Refuse("une fiche porte déjà cet identifiant dans le carnet".into())),
        412 => Err(Erreur::Refuse(CONFLIT.into())),
        401 => Err(Erreur::Refuse("accès aux contacts refusé".into())),
        403 => Err(Erreur::Refuse("ce carnet n'accepte pas l'écriture".into())),
        507 => Err(Erreur::Refuse("espace insuffisant sur le serveur".into())),
        statut => Err(Erreur::Protocole(format!("écriture dans le carnet : HTTP {statut}"))),
    }
}

/// Crée une fiche (sans ETag : elle ne doit pas exister) ou la remplace (avec
/// l'ETag lu : une fiche changée entre-temps n'est pas écrasée). Rend l'ETag
/// du dépôt, si le serveur le donne. SOGo ignore `If-None-Match` : c'est
/// l'identifiant aléatoire d'une fiche neuve qui la garde d'en écraser une.
pub fn deposer(dav: &impl Dav, url: &str, vcard: &str, etag: Option<&str>) -> Resultat<Option<String>> {
    let condition = etag.map_or(Condition::SiAbsent, Condition::SiEtag);
    let reponse = dav.ecrire("PUT", url, TYPE_VCARD, vcard, condition)?;
    issue_ecriture(&reponse, etag.is_none())?;
    Ok(reponse.entete("etag").map(str::to_string))
}

/// Retire une fiche, si elle n'a pas changé depuis que l'index l'a lue. SOGo
/// ignore `If-Match` sur un DELETE : l'ETag est relu d'abord. Une fiche déjà
/// absente est tenue pour retirée.
pub fn retirer(dav: &impl Dav, collection: &str, url: &str, etag: &str) -> Resultat<()> {
    let reponse = dav.envoyer("PROPFIND", url, "0", DEMANDE_ETAGS)?;
    match reponse.statut {
        404 | 410 => return Ok(()),
        207 => {
            let actuel = lire_fiches(&texte(&reponse), collection).into_iter().next().map(|f| f.etag);
            if actuel.as_deref().is_some_and(|e| e != etag) {
                return Err(Erreur::Refuse(CONFLIT.into()));
            }
        }
        _ => issue_ecriture(&reponse, false)?,
    }
    let reponse = dav.ecrire("DELETE", url, TYPE_VCARD, "", Condition::SiEtag(etag))?;
    if matches!(reponse.statut, 404 | 410) {
        return Ok(());
    }
    issue_ecriture(&reponse, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiche_vcard() {
        let c = lire_vcard(
            "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Dupont;Marie;;;\r\nFN:Marie Dupont\r\nORG:Transports\\, Lalanne;Accueil\r\n\
             item1.EMAIL;TYPE=work:marie@lalanne.fr\r\nEMAIL;TYPE=home:Marie@Exemple.FR\r\nEMAIL:marie@lalanne.fr\r\n\
             TEL;TYPE=cell:06 12 34\r\n 56 78\r\nNOTE:ligne\\nsuivante\r\nEND:VCARD\r\n",
        );
        assert_eq!(c.nom, "Marie Dupont");
        assert_eq!(c.organisation, "Transports, Lalanne, Accueil");
        assert_eq!(c.courriels, vec!["marie@lalanne.fr", "Marie@Exemple.FR"]);
        assert_eq!(c.telephones, vec!["06 12 3456 78"]);
        // Sans FN : le nom structuré, puis l'organisation.
        assert_eq!(lire_vcard("BEGIN:VCARD\nN:Martin;Paul\nEND:VCARD").nom, "Paul Martin");
        assert_eq!(lire_vcard("BEGIN:VCARD\nORG:MMEDIA\nEMAIL:contact@mmedia.fr\nEND:VCARD").nom, "MMEDIA");
        assert_eq!(lire_vcard("BEGIN:VCARD\nEMAIL:pas-une-adresse\nEND:VCARD").courriels.len(), 0);
        // Un point-virgule échappé reste dans l'organisation.
        assert_eq!(lire_vcard("BEGIN:VCARD\nORG:A\\; B\\, C;Service\nEND:VCARD").organisation, "A; B, C, Service");
    }

    #[test]
    fn carnets_dans_l_index() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("moi@x.fr", "mail.x.fr", 993, "moi@x.fr").unwrap();
        let carnets = vec![("https://h/c/personal/".to_string(), "Personnel".to_string(), true)];
        assert!(m.poser_carnets(compte, &carnets).unwrap());
        assert!(!m.poser_carnets(compte, &carnets).unwrap());
        let (id, _, _) = m.carnets(compte).unwrap()[0].clone();
        let marie = lire_vcard("BEGIN:VCARD\nFN:Marie Dupont\nORG:Lalanne\nEMAIL:marie@lalanne.fr\nTEL:0612\nEND:VCARD");
        m.poser_contacts(id, &[("a.vcf".into(), "\"1\"".into(), marie, "BEGIN:VCARD".into())]).unwrap();
        assert_eq!(m.etags_contacts(id).unwrap().get("a.vcf").map(String::as_str), Some("\"1\""));
        let trouves = m.fiches_contacts("lalan", 10).unwrap();
        assert_eq!(trouves.len(), 1);
        assert_eq!(trouves[0].courriels, vec!["marie@lalanne.fr"]);
        assert_eq!(trouves[0].telephones, vec!["0612"]);
        assert_eq!(trouves[0].carnet, "Personnel");
        // La saisie propose le contact, avec son nom.
        let saisie = m.contacts("marie", 5).unwrap();
        assert_eq!(saisie.first(), Some(&("Marie Dupont".to_string(), "marie@lalanne.fr".to_string())));
        // Fiche retirée, carnet disparu : plus rien.
        m.retirer_contacts(id, &["a.vcf".into()]).unwrap();
        assert!(m.fiches_contacts("", 10).unwrap().is_empty());
        assert!(m.poser_carnets(compte, &[]).unwrap());
        assert!(m.carnets(compte).unwrap().is_empty());
    }

    #[test]
    fn carnets_et_fiches() {
        let xml = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/">
<d:response><d:href>/SOGo/dav/u/Contacts/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/SOGo/dav/u/Contacts/personal/</d:href><d:propstat><d:prop><d:displayname>Carnet personnel</d:displayname><d:resourcetype><d:collection/><card:addressbook/></d:resourcetype><cs:getctag>42</cs:getctag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/SOGo/dav/u/Contacts/collected/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><card:addressbook/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/SOGo/dav/u/Contacts/lab.mmedia.fr/</d:href><d:propstat><d:prop><d:displayname>GAL lab.mmedia.fr</d:displayname><d:resourcetype><d:collection/><card:addressbook/><card:directory/></d:resourcetype><d:current-user-privilege-set><d:privilege><d:read/></d:privilege><d:privilege><d:all/></d:privilege></d:current-user-privilege-set></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;
        let carnets = lire_carnets(xml);
        assert_eq!(carnets.len(), 3);
        // La liste globale de SOGo se dit « all » : un annuaire reste en lecture.
        assert!(!carnets[2].ecriture && carnets[1].ecriture);
        assert_eq!(
            carnets[0],
            CarnetDistant { href: "/SOGo/dav/u/Contacts/personal/".into(), nom: "Carnet personnel".into(), ctag: "42".into(), ecriture: true }
        );
        assert_eq!(carnets[1].nom, "collected");
        let fiches = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
<d:response><d:href>/SOGo/dav/u/Contacts/personal/</d:href><d:propstat><d:prop><d:getetag>"c"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/SOGo/dav/u/Contacts/personal/a.vcf</d:href><d:propstat><d:prop><d:getetag>"1"</d:getetag><card:address-data>BEGIN:VCARD
FN:A
END:VCARD</card:address-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/SOGo/dav/u/Contacts/personal/b.vcf</d:href><d:propstat><d:prop><d:getetag>"2"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;
        let f = lire_fiches(fiches, "https://h/SOGo/dav/u/Contacts/personal/");
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].etag, "\"1\"");
        assert!(f[0].vcard.as_deref().unwrap().contains("FN:A"));
        assert!(f[1].vcard.is_none());
        assert!(demande_fiches(&["/a&b.vcf".into()]).contains("<d:href>/a&amp;b.vcf</d:href>"));
    }
}

/// Écriture (0.6.20) contre un faux serveur qui se comporte comme SOGo :
/// ETag vérifié au PUT, ignoré au DELETE ; droits rendus par carnet.
#[cfg(test)]
mod essais_ecriture {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;
    use crate::http::Reponse;

    const COLLECTION: &str = "/dav/u/Contacts/";
    const PERSO: &str = "/dav/u/Contacts/personal/";
    const PARTAGE: &str = "/dav/u/Contacts/partage/";

    #[derive(Default)]
    struct Faux {
        /// Par carnet : étiquette, et par fiche son ETag et son texte.
        carnets: RefCell<BTreeMap<String, (u32, BTreeMap<String, (String, String)>)>>,
        version: RefCell<u32>,
    }

    fn reponse(statut: u16, corps: String, etag: Option<String>) -> Resultat<Reponse> {
        Ok(Reponse {
            statut,
            type_contenu: "application/xml".into(),
            redirection: None,
            corps: corps.into_bytes(),
            adresse: format!("https://h{COLLECTION}"),
            entetes: etag.map(|e| vec![("etag".to_string(), e)]).unwrap_or_default(),
        })
    }

    fn xml(t: &str) -> String {
        t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    fn multistatus(r: &[String]) -> String {
        format!(r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav" xmlns:A="http://calendarserver.org/ns/">{}</D:multistatus>"#, r.join(""))
    }

    fn propriete(href: &str, prop: &str) -> String {
        format!("<D:response><D:href>{href}</D:href><D:propstat><D:prop>{prop}</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>")
    }

    fn chemin(url: &str) -> &str {
        url.strip_prefix("https://h").unwrap_or(url)
    }

    impl Faux {
        fn nouveau() -> Faux {
            let f = Faux::default();
            f.carnets.borrow_mut().insert(PERSO.into(), (1, BTreeMap::new()));
            f.carnets.borrow_mut().insert(PARTAGE.into(), (1, BTreeMap::new()));
            f
        }

        fn etag(&self) -> String {
            *self.version.borrow_mut() += 1;
            format!("\"{}\"", self.version.borrow())
        }
    }

    impl Dav for Faux {
        fn envoyer(&self, methode: &str, url: &str, profondeur: &str, corps: &str) -> Resultat<Reponse> {
            let c = chemin(url);
            let carnets = self.carnets.borrow();
            if profondeur == "0" {
                return match carnets.values().find_map(|(_, f)| f.get(c)) {
                    Some((etag, _)) => reponse(207, multistatus(&[propriete(c, &format!("<D:getetag>{}</D:getetag>", xml(etag)))]), None),
                    None => reponse(404, String::new(), None),
                };
            }
            match (methode, c) {
                ("PROPFIND", COLLECTION) => {
                    let mut r = vec![propriete(COLLECTION, "<D:resourcetype><D:collection/></D:resourcetype>")];
                    for (href, (ctag, _)) in carnets.iter() {
                        let droits = if href == PERSO {
                            "<D:privilege><D:read/></D:privilege><D:privilege><D:write/></D:privilege>"
                        } else {
                            "<D:privilege><D:read/></D:privilege>"
                        };
                        r.push(propriete(
                            href,
                            &format!("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype><A:getctag>{ctag}</A:getctag><D:current-user-privilege-set>{droits}</D:current-user-privilege-set>"),
                        ));
                    }
                    reponse(207, multistatus(&r), None)
                }
                ("PROPFIND", carnet) if carnets.contains_key(carnet) => {
                    let r: Vec<String> =
                        carnets[carnet].1.iter().map(|(h, (e, _))| propriete(h, &format!("<D:getetag>{}</D:getetag>", xml(e)))).collect();
                    reponse(207, multistatus(&r), None)
                }
                ("REPORT", carnet) if carnets.contains_key(carnet) => {
                    let doc = roxmltree::Document::parse(corps).unwrap();
                    let r: Vec<String> = doc
                        .descendants()
                        .filter(|n| est(n, DAV, "href"))
                        .filter_map(|n| n.text())
                        .filter_map(|h| carnets[carnet].1.get(h).map(|(e, v)| (h, e, v)))
                        .map(|(h, e, v)| propriete(h, &format!("<D:getetag>{}</D:getetag><C:address-data>{}</C:address-data>", xml(e), xml(v))))
                        .collect();
                    reponse(207, multistatus(&r), None)
                }
                _ => reponse(404, String::new(), None),
            }
        }

        fn ecrire(&self, methode: &str, url: &str, type_contenu: &str, corps: &str, condition: Condition) -> Resultat<Reponse> {
            assert_eq!(type_contenu, TYPE_VCARD);
            let c = chemin(url).to_string();
            let etag = self.etag();
            let mut carnets = self.carnets.borrow_mut();
            let Some((_, (ctag, fiches))) = carnets.iter_mut().find(|(h, _)| c.starts_with(h.as_str())) else {
                return reponse(404, String::new(), None);
            };
            match methode {
                "PUT" => {
                    let actuel = fiches.get(&c).map(|(e, _)| e.clone());
                    let refuse = match condition {
                        Condition::SiAbsent => actuel.is_some(),
                        Condition::SiEtag(e) => actuel.as_deref() != Some(e),
                        Condition::Aucune => false,
                    };
                    if refuse {
                        return reponse(412, String::new(), None);
                    }
                    fiches.insert(c, (etag.clone(), corps.to_string()));
                    *ctag += 1;
                    reponse(if actuel.is_some() { 204 } else { 201 }, String::new(), Some(etag))
                }
                // SOGo ignore If-Match ici : `retirer` relit l'ETag avant.
                "DELETE" => match fiches.remove(&c) {
                    Some(_) => {
                        *ctag += 1;
                        reponse(204, String::new(), None)
                    }
                    None => reponse(404, String::new(), None),
                },
                _ => reponse(405, String::new(), None),
            }
        }
    }

    #[test]
    fn creation_modification_conflit_et_retrait() {
        let serveur = Faux::nouveau();
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("u@h.fr", "h", 993, "u@h.fr").unwrap();
        m.poser_carddav(compte, &format!("https://h{COLLECTION}"), 0).unwrap();
        let synchro = || synchroniser(&m, compte, "h", &serveur, 0, false).unwrap();
        assert_eq!(synchro().carnets, 2);
        let carnets = m.carnets_locaux().unwrap();
        let ecriture: Vec<(&str, bool)> = carnets.iter().map(|c| (c.nom.as_str(), c.ecriture)).collect();
        assert_eq!(ecriture, vec![("partage", false), ("personal", true)]);

        // Création : la fiche, son texte et son ETag arrivent dans l'index.
        let saisie = crate::fiche::Fiche {
            prenom: "Marie".into(),
            nom: "Dupont".into(),
            courriels: vec![crate::fiche::Coordonnee { genre: "travail".into(), valeur: "marie@exemple.fr".into(), cle: None }],
            ..Default::default()
        };
        let maintenant = chrono::Utc::now();
        let url = format!("https://h{PERSO}abc.vcf");
        let vcard = crate::fiche::creer(&saisie, "abc", maintenant).unwrap();
        assert_eq!(deposer(&serveur, &url, &vcard, None).unwrap().as_deref(), Some("\"1\""));
        // Même identifiant : pas d'écrasement.
        let e = deposer(&serveur, &url, &vcard, None).unwrap_err().to_string();
        assert!(e.contains("porte déjà"), "{e}");
        assert_eq!(synchro().lus, 1);
        let trouve = m.fiches_contacts("marie", 5).unwrap();
        assert_eq!((trouve[0].nom.as_str(), trouve[0].ecriture), ("Marie Dupont", true));
        let index = m.fiche_contact(trouve[0].id).unwrap().unwrap();
        assert_eq!(index.etag, "\"1\"");
        assert!(index.vcard.contains("UID:abc"));

        // Modification avec l'ETag lu ; puis une autre partie de cette même
        // version : refusée, la fiche ayant changé entre-temps.
        let origine = crate::fiche::lire(&index.vcard).unwrap();
        let mut changee = origine.clone();
        changee.organisation = "Exemple SA".into();
        let v2 = crate::fiche::modifier(&index.vcard, &origine, &changee, maintenant).unwrap();
        assert!(deposer(&serveur, &url, &v2, Some(&index.etag)).is_ok());
        let e = deposer(&serveur, &url, &v2, Some(&index.etag)).unwrap_err();
        assert!(matches!(&e, Erreur::Refuse(m) if m == CONFLIT), "{e}");
        assert!(retirer(&serveur, &format!("https://h{PERSO}"), &url, &index.etag).is_err(), "retrait d'une version dépassée");
        assert_eq!(synchro().lus, 1);
        let index = m.fiche_contact(index.id).unwrap().unwrap();
        assert!(index.vcard.contains("ORG:Exemple SA"));
        assert_eq!(m.fiches_contacts("exemple sa", 5).unwrap()[0].organisation, "Exemple SA");

        // Retrait avec l'ETag à jour ; une fiche déjà partie est tenue pour
        // retirée.
        retirer(&serveur, &format!("https://h{PERSO}"), &url, &index.etag).unwrap();
        retirer(&serveur, &format!("https://h{PERSO}"), &url, &index.etag).unwrap();
        assert_eq!(synchro().retires, 1);
        assert!(m.fiches_contacts("marie", 5).unwrap().is_empty());
        assert_eq!(m.fiche_par_adresse("MARIE@exemple.fr").unwrap(), None);
    }
}
