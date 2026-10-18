// SPDX-License-Identifier: GPL-3.0-or-later
//! CardDAV (RFC 6352) : les carnets d'adresses d'une boîte (0.6.1) — SOGo pour
//! Mailcow —, lus et gardés dans l'index. Leurs contacts viennent en tête des
//! adresses proposées à la saisie, et se consultent dans le carnet de MMail.
//! Lecture seule : un contact se crée ou se modifie dans le webmail.
//!
//! Même découverte que les agendas (cf. `caldav`) : `/.well-known/carddav`
//! sur le serveur de la boîte, puis l'adresse de SOGo ; de là le principal,
//! puis la collection des carnets (`addressbook-home-set`). Même
//! synchronisation par comparaison : l'étiquette (`getctag`) d'un carnet dit
//! s'il a changé, ses ETag disent quelles fiches relire et lesquelles ont
//! disparu.

use std::collections::HashSet;

use crate::caldav::{adresse, demander, est, index, reponses, texte, Dav, DEMANDE_ETAGS, DEMANDE_PRINCIPAL};
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

/// PROPFIND, profondeur 1 sur la collection : les carnets et leur étiquette.
pub const DEMANDE_CARNETS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/"><d:prop><d:displayname/><d:resourcetype/><cs:getctag/></d:prop></d:propfind>"#;

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarnetDistant {
    pub href: String,
    pub nom: String,
    pub ctag: String,
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
        for p in proprietes {
            let valeur = p.text().unwrap_or("").trim().to_string();
            if est(&p, DAV, "displayname") {
                carnet.nom = valeur;
            } else if est(&p, CS, "getctag") {
                carnet.ctag = valeur;
            } else if est(&p, DAV, "resourcetype") {
                est_carnet = p.children().any(|t| est(&t, CARDDAV, "addressbook"));
            }
        }
        if est_carnet {
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
                // Nom;Prénom;Autres;Préfixe;Suffixe
                let parties: Vec<String> = valeur.split(';').map(desechapper).collect();
                let prenom = parties.get(1).cloned().unwrap_or_default();
                let nom = parties.first().cloned().unwrap_or_default();
                nom_structure = format!("{prenom} {nom}").trim().to_string();
            }
            "ORG" => contact.organisation = valeur.split(';').map(desechapper).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
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
    let descriptions: Vec<(String, String)> = distants.iter().map(|c| (c.href.clone(), c.nom.clone())).collect();
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
        let fiches: Vec<(String, String, Contact)> = lire_fiches(&texte(&reponse), url)
            .into_iter()
            .map(|f| {
                let contact = lire_vcard(f.vcard.as_deref().unwrap_or(""));
                (f.href, f.etag, contact)
            })
            .collect();
        lus += fiches.len();
        magasin.poser_contacts(carnet, &fiches).map_err(index)?;
    }
    Ok((lus, a_retirer.len()))
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
    }

    #[test]
    fn carnets_dans_l_index() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("moi@x.fr", "mail.x.fr", 993, "moi@x.fr").unwrap();
        let carnets = vec![("https://h/c/personal/".to_string(), "Personnel".to_string())];
        assert!(m.poser_carnets(compte, &carnets).unwrap());
        assert!(!m.poser_carnets(compte, &carnets).unwrap());
        let (id, _, _) = m.carnets(compte).unwrap()[0].clone();
        let marie = lire_vcard("BEGIN:VCARD\nFN:Marie Dupont\nORG:Lalanne\nEMAIL:marie@lalanne.fr\nTEL:0612\nEND:VCARD");
        m.poser_contacts(id, &[("a.vcf".into(), "\"1\"".into(), marie)]).unwrap();
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
</d:multistatus>"#;
        let carnets = lire_carnets(xml);
        assert_eq!(carnets.len(), 2);
        assert_eq!(carnets[0], CarnetDistant { href: "/SOGo/dav/u/Contacts/personal/".into(), nom: "Carnet personnel".into(), ctag: "42".into() });
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
