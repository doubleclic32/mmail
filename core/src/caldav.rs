// SPDX-License-Identifier: GPL-3.0-or-later
//! CalDAV (RFC 4791) : découverte des agendas d'une boîte et lecture de leurs
//! objets, sur le client HTTPS du programme.
//!
//! Découverte selon la RFC 6764 : `/.well-known/caldav` sur le serveur de la
//! boîte, puis l'adresse de SOGo (Mailcow) ; de là le principal de
//! l'utilisateur, puis la collection de ses agendas. Les identifiants ne
//! partent que vers le serveur de la boîte : le domaine de l'adresse, que la
//! RFC propose aussi d'interroger, est souvent un site hébergé chez un tiers.
//!
//! Synchronisation par comparaison, sans `sync-collection` : l'étiquette de
//! chaque agenda (`getctag`) dit s'il a changé ; s'il a changé, la liste des
//! ETag dit quels objets relire, et ceux qui ont disparu.

use std::collections::HashSet;

use crate::http::{self, Demande, Reponse};
use crate::imap::{Erreur, Resultat};
use crate::magasin::{Magasin, ObjetAgenda};

/// Taille au-delà de laquelle une réponse est refusée : un lot d'objets, pièces
/// jointes comprises.
const TAILLE_MAX: usize = 32 * 1024 * 1024;

/// Objets demandés par `calendar-multiget`.
const LOT: usize = 100;

/// Délai avant un nouvel essai de découverte infructueux, sauf demande de
/// l'utilisateur : une boîte sans agenda ne coûte pas trois requêtes à chaque
/// synchronisation.
const REESSAI_DECOUVERTE: i64 = 24 * 3600;

/// Espaces de noms.
const DAV: &str = "DAV:";
const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const CS: &str = "http://calendarserver.org/ns/";
const ICAL: &str = "http://apple.com/ns/ical/";

/// PROPFIND, profondeur 0 : le principal de l'utilisateur.
pub const DEMANDE_PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#;

/// PROPFIND, profondeur 0 sur le principal : la collection des agendas.
pub const DEMANDE_COLLECTION: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><c:calendar-home-set/></d:prop></d:propfind>"#;

/// PROPFIND, profondeur 1 sur la collection : les agendas et leur étiquette.
pub const DEMANDE_AGENDAS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:ic="http://apple.com/ns/ical/"><d:prop><d:displayname/><d:resourcetype/><cs:getctag/><ic:calendar-color/><c:supported-calendar-component-set/><d:current-user-privilege-set/></d:prop></d:propfind>"#;

/// PROPFIND, profondeur 1 sur un agenda : l'ETag de chaque objet.
pub const DEMANDE_ETAGS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:getetag/></d:prop></d:propfind>"#;

/// REPORT `calendar-multiget` : les objets désignés, avec leur texte.
pub fn demande_objets(hrefs: &[String]) -> String {
    let mut corps = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-multiget xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:getetag/><c:calendar-data/></d:prop>"#,
    );
    for href in hrefs {
        corps.push_str("<d:href>");
        corps.push_str(&echapper_xml(href));
        corps.push_str("</d:href>");
    }
    corps.push_str("</c:calendar-multiget>");
    corps
}

fn echapper_xml(texte: &str) -> String {
    texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Un agenda trouvé sur le serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaDistant {
    pub href: String,
    pub nom: String,
    /// `#RRGGBB`, ou vide.
    pub couleur: String,
    pub ctag: String,
    /// L'utilisateur peut y écrire. Sans `current-user-privilege-set`, on le
    /// suppose : le serveur refusera, le cas échéant.
    pub ecriture: bool,
}

/// Un objet d'un agenda : son adresse, son ETag et, s'il a été demandé, son
/// texte iCalendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjetDistant {
    pub href: String,
    pub etag: String,
    pub ical: Option<String>,
}

pub(crate) fn est(noeud: &roxmltree::Node, espace: &str, nom: &str) -> bool {
    noeud.is_element() && noeud.tag_name().name() == nom && noeud.tag_name().namespace() == Some(espace)
}

/// Chaque réponse d'un `multistatus` : son href, et les propriétés rendues
/// avec le statut 200.
pub(crate) fn reponses<'a, 'b>(doc: &'b roxmltree::Document<'a>) -> Vec<(String, Vec<roxmltree::Node<'a, 'b>>)> {
    let mut sortie = Vec::new();
    for reponse in doc.descendants().filter(|n| est(n, DAV, "response")) {
        let href = reponse
            .children()
            .find(|n| est(n, DAV, "href"))
            .and_then(|n| n.text())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut proprietes = Vec::new();
        for propstat in reponse.children().filter(|n| est(n, DAV, "propstat")) {
            let statut = propstat.children().find(|n| est(n, DAV, "status")).and_then(|n| n.text()).unwrap_or("");
            if !statut.split_whitespace().nth(1).is_some_and(|code| code == "200") {
                continue;
            }
            if let Some(prop) = propstat.children().find(|n| est(n, DAV, "prop")) {
                proprietes.extend(prop.children().filter(|n| n.is_element()));
            }
        }
        if !href.is_empty() {
            sortie.push((href, proprietes));
        }
    }
    sortie
}

/// Href contenu dans une propriété (`current-user-principal`,
/// `calendar-home-set`) de la première réponse qui la porte.
pub fn lire_href(xml: &str, espace_dav: bool, nom: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let espace = if espace_dav { DAV } else { CALDAV };
    reponses(&doc).into_iter().find_map(|(_, proprietes)| {
        proprietes.iter().find(|p| est(p, espace, nom)).and_then(|p| {
            p.children().find(|n| est(n, DAV, "href")).and_then(|n| n.text()).map(|t| t.trim().to_string())
        })
    })
    .filter(|h| !h.is_empty())
}

/// Agendas d'une réponse PROPFIND sur la collection : les collections de type
/// `calendar` qui acceptent des événements.
pub fn lire_agendas(xml: &str) -> Vec<AgendaDistant> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let mut agendas = Vec::new();
    for (href, proprietes) in reponses(&doc) {
        let mut agenda = AgendaDistant { href, nom: String::new(), couleur: String::new(), ctag: String::new(), ecriture: true };
        let mut calendrier = false;
        // Sans la propriété, la RFC dit : tous les composants acceptés.
        let mut evenements = true;
        for p in proprietes {
            let texte = p.text().unwrap_or("").trim().to_string();
            if est(&p, DAV, "displayname") {
                agenda.nom = texte;
            } else if est(&p, CS, "getctag") {
                agenda.ctag = texte;
            } else if est(&p, ICAL, "calendar-color") {
                // SOGo écrit `#RRGGBBAA` : la transparence ne sert pas ici.
                if texte.starts_with('#') && texte.len() >= 7 && texte[1..7].chars().all(|c| c.is_ascii_hexdigit()) {
                    agenda.couleur = texte[..7].to_ascii_uppercase();
                }
            } else if est(&p, DAV, "resourcetype") {
                calendrier = p.children().any(|t| est(&t, CALDAV, "calendar"));
            } else if est(&p, CALDAV, "supported-calendar-component-set") {
                evenements = p.children().any(|c| est(&c, CALDAV, "comp") && c.attribute("name") == Some("VEVENT"));
            } else if est(&p, DAV, "current-user-privilege-set") {
                agenda.ecriture = p.descendants().any(|d| {
                    d.is_element()
                        && d.tag_name().namespace() == Some(DAV)
                        && matches!(d.tag_name().name(), "write" | "write-content" | "all" | "bind")
                });
            }
        }
        if calendrier && evenements {
            if agenda.nom.is_empty() {
                agenda.nom = agenda.href.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
            }
            agendas.push(agenda);
        }
    }
    agendas
}

/// Objets d'une réponse PROPFIND (ETag seuls) ou `calendar-multiget` (avec
/// texte). La collection elle-même, et ce qui n'a pas d'ETag, n'en sont pas.
pub fn lire_objets(xml: &str, collection: &str) -> Vec<ObjetDistant> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let mut objets = Vec::new();
    for (href, proprietes) in reponses(&doc) {
        if chemin(&href).trim_end_matches('/') == chemin(collection).trim_end_matches('/') || href.ends_with('/') {
            continue;
        }
        let mut objet = ObjetDistant { href, etag: String::new(), ical: None };
        for p in proprietes {
            if est(&p, DAV, "getetag") {
                objet.etag = p.text().unwrap_or("").trim().to_string();
            } else if est(&p, CALDAV, "calendar-data") {
                objet.ical = Some(p.text().unwrap_or("").to_string());
            }
        }
        if !objet.etag.is_empty() {
            objets.push(objet);
        }
    }
    objets
}

/// Chemin d'une adresse : `/a/b/` pour `https://hote/a/b/` comme pour `/a/b/`.
fn chemin(adresse: &str) -> &str {
    match adresse.strip_prefix("https://").or_else(|| adresse.strip_prefix("http://")) {
        Some(reste) => reste.find('/').map(|i| &reste[i..]).unwrap_or("/"),
        None => adresse,
    }
}

/// Adresse complète d'un href rendu par le serveur, relatif à `base`.
pub fn adresse(base: &str, href: &str) -> Option<String> {
    if href.starts_with("https://") {
        return Some(href.to_string());
    }
    let reste = base.strip_prefix("https://")?;
    let origine = &base[..8 + reste.find('/').unwrap_or(reste.len())];
    if href.starts_with('/') {
        return Some(format!("{origine}{href}"));
    }
    let dossier = &base[..base.rfind('/').filter(|&i| i >= origine.len()).map(|i| i + 1).unwrap_or(base.len())];
    let dossier = if dossier.ends_with('/') { dossier.to_string() } else { format!("{dossier}/") };
    Some(format!("{dossier}{href}"))
}

// ------------------------------------------------------------- réseau

/// Identifiants de la boîte : le serveur d'agenda de Mailcow (SOGo) accepte
/// ceux du serveur IMAP.
pub struct Acces<'a> {
    pub utilisateur: &'a str,
    pub mot_de_passe: &'a str,
}

/// Ce qui porte les requêtes WebDAV : le serveur de la boîte, ou un faux
/// serveur dans les tests.
pub trait Dav {
    /// Envoie une requête et rend la réponse, quel qu'en soit le statut.
    fn envoyer(&self, methode: &str, url: &str, profondeur: &str, corps: &str) -> Resultat<Reponse>;

    /// Dépose (PUT) ou retire (DELETE) un objet iCalendar, sous condition
    /// d'ETag (`If-Match`) s'il est donné.
    fn ecrire(&self, methode: &str, url: &str, corps: &str, si_etag: Option<&str>) -> Resultat<Reponse>;
}

impl Dav for Acces<'_> {
    fn envoyer(&self, methode: &str, url: &str, profondeur: &str, corps: &str) -> Resultat<Reponse> {
        let jeton = crate::smtp::base64(format!("{}:{}", self.utilisateur, self.mot_de_passe).as_bytes());
        let entetes = [
            ("Authorization", format!("Basic {jeton}")),
            ("Depth", profondeur.to_string()),
            ("Content-Type", "application/xml; charset=utf-8".to_string()),
        ];
        http::envoyer(&Demande {
            methode,
            url,
            accepte: "application/xml, text/xml",
            entetes: &entetes,
            corps: corps.as_bytes(),
            taille_max: TAILLE_MAX,
        })
    }

    fn ecrire(&self, methode: &str, url: &str, corps: &str, si_etag: Option<&str>) -> Resultat<Reponse> {
        let jeton = crate::smtp::base64(format!("{}:{}", self.utilisateur, self.mot_de_passe).as_bytes());
        let mut entetes = vec![
            ("Authorization", format!("Basic {jeton}")),
            ("Content-Type", "text/calendar; charset=utf-8".to_string()),
        ];
        if let Some(etag) = si_etag {
            entetes.push(("If-Match", etag.to_string()));
        }
        http::envoyer(&Demande { methode, url, accepte: "*/*", entetes: &entetes, corps: corps.as_bytes(), taille_max: TAILLE_MAX })
    }
}

/// Envoie une requête en exigeant une réponse « multistatus » (207).
pub(crate) fn demander(dav: &impl Dav, methode: &str, url: &str, profondeur: &str, corps: &str) -> Resultat<Reponse> {
    let reponse = dav.envoyer(methode, url, profondeur, corps)?;
    match reponse.statut {
        207 => Ok(reponse),
        401 | 403 => Err(Erreur::Refuse(format!("accès à l'agenda refusé (HTTP {})", reponse.statut))),
        statut => Err(Erreur::Protocole(format!("{methode} {url} : HTTP {statut}"))),
    }
}

/// Une erreur de l'index local, dite comme telle.
pub(crate) fn index(e: crate::magasin::Erreur) -> Erreur {
    Erreur::Protocole(format!("index local : {e}"))
}

pub(crate) fn texte(reponse: &Reponse) -> String {
    String::from_utf8_lossy(&reponse.corps).into_owned()
}

/// Adresse de la collection des agendas d'une boîte, ou `None` si aucun
/// serveur CalDAV ne s'est fait connaître. Un refus des identifiants est
/// rendu comme tel : il dit que le serveur existe.
pub fn decouvrir(hote: &str, dav: &impl Dav) -> Resultat<Option<String>> {
    let departs = [format!("https://{hote}/.well-known/caldav"), format!("https://{hote}/SOGo/dav/")];
    let mut refus = None;
    for depart in departs {
        match collection_depuis(&depart, dav) {
            Ok(Some(collection)) => return Ok(Some(collection)),
            Ok(None) => {}
            Err(e @ Erreur::Refuse(_)) => refus = Some(e),
            Err(_) => {}
        }
    }
    refus.map_or(Ok(None), Err)
}

/// Du point de départ au principal, du principal à la collection des agendas.
fn collection_depuis(depart: &str, dav: &impl Dav) -> Resultat<Option<String>> {
    let reponse = demander(dav, "PROPFIND", depart, "0", DEMANDE_PRINCIPAL)?;
    let Some(principal) = lire_href(&texte(&reponse), true, "current-user-principal")
        .and_then(|href| adresse(&reponse.adresse, &href))
    else {
        return Ok(None);
    };
    let reponse = demander(dav, "PROPFIND", &principal, "0", DEMANDE_COLLECTION)?;
    Ok(lire_href(&texte(&reponse), false, "calendar-home-set").and_then(|href| adresse(&reponse.adresse, &href)))
}

/// Le serveur distribue-t-il lui-même invitations et réponses (RFC 6638,
/// `calendar-auto-schedule` dans l'en-tête `DAV` d'un OPTIONS) ?
pub fn planification(dav: &impl Dav, url: &str) -> Resultat<bool> {
    let reponse = dav.envoyer("OPTIONS", url, "0", "")?;
    if !(200..300).contains(&reponse.statut) {
        return Err(Erreur::Protocole(format!("OPTIONS {url} : HTTP {}", reponse.statut)));
    }
    Ok(reponse.entete("dav").is_some_and(|v| v.split(',').any(|c| c.trim().eq_ignore_ascii_case("calendar-auto-schedule"))))
}

/// Ce qu'a fait une synchronisation.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    /// Vrai si les agendas ou leur contenu ont changé.
    pub change: bool,
    pub agendas: usize,
    pub lus: usize,
    pub retires: usize,
}

/// Synchronise les agendas d'un compte avec l'index. `insister` : à la
/// demande de l'utilisateur, la découverte est retentée sans attendre.
pub fn synchroniser(
    magasin: &Magasin,
    compte: i64,
    hote: &str,
    dav: &impl Dav,
    maintenant: i64,
    insister: bool,
) -> Resultat<Bilan> {
    let mut bilan = Bilan::default();
    let (mut collection, essai) = magasin.caldav(compte).map_err(index)?;
    if collection.is_empty() {
        // Zéro : jamais essayé.
        if !insister && essai > 0 && maintenant - essai < REESSAI_DECOUVERTE {
            return Ok(bilan);
        }
        match decouvrir(hote, dav) {
            Ok(Some(trouvee)) => {
                magasin.poser_caldav(compte, &trouvee, 0).map_err(index)?;
                collection = trouvee;
            }
            Ok(None) => {
                magasin.poser_caldav(compte, "", maintenant).map_err(index)?;
                return Ok(bilan);
            }
            Err(e) => {
                magasin.poser_caldav(compte, "", maintenant).map_err(index)?;
                return Err(e);
            }
        }
    }
    let reponse = dav.envoyer("PROPFIND", &collection, "1", DEMANDE_AGENDAS)?;
    match reponse.statut {
        207 => {}
        // La collection a changé d'adresse : elle sera cherchée de nouveau.
        301 | 302 | 404 | 410 => {
            magasin.poser_caldav(compte, "", 0).map_err(index)?;
            return Err(Erreur::Protocole(format!("collection d'agendas introuvable (HTTP {})", reponse.statut)));
        }
        401 | 403 => return Err(Erreur::Refuse(format!("accès à l'agenda refusé (HTTP {})", reponse.statut))),
        statut => return Err(Erreur::Protocole(format!("PROPFIND {collection} : HTTP {statut}"))),
    }
    // Une fois pour toutes : le serveur distribue-t-il les invitations ?
    if magasin.planification(compte).map_err(index)?.is_none() {
        if let Ok(p) = planification(dav, &collection) {
            magasin.poser_planification(compte, Some(p)).map_err(index)?;
        }
    }
    let distants: Vec<AgendaDistant> = lire_agendas(&texte(&reponse))
        .into_iter()
        .filter_map(|mut a| {
            a.href = adresse(&reponse.adresse, &a.href)?;
            Some(a)
        })
        .collect();
    let descriptions: Vec<_> =
        distants.iter().map(|a| (a.href.clone(), a.nom.clone(), a.couleur.clone(), a.ecriture)).collect();
    bilan.change |= magasin.poser_agendas(compte, &descriptions).map_err(index)?;
    bilan.agendas = distants.len();
    let locaux = magasin.agendas().map_err(index)?;
    for distant in &distants {
        let Some(local) = locaux.iter().find(|l| l.compte == compte && l.adresse == distant.href) else { continue };
        // Étiquette inchangée : rien n'a bougé. Sans étiquette, on compare.
        if !distant.ctag.is_empty() && distant.ctag == local.ctag {
            continue;
        }
        let (lus, retires) = synchroniser_agenda(magasin, local.id, &distant.href, dav)?;
        magasin.poser_ctag(local.id, &distant.ctag).map_err(index)?;
        bilan.lus += lus;
        bilan.retires += retires;
        bilan.change |= lus + retires > 0;
    }
    Ok(bilan)
}

/// Compare les ETag d'un agenda à ceux de l'index : relit ce qui est nouveau
/// ou modifié, retire ce qui a disparu. Rend le nombre d'objets lus et
/// retirés.
fn synchroniser_agenda(magasin: &Magasin, agenda: i64, url: &str, dav: &impl Dav) -> Resultat<(usize, usize)> {
    let reponse = demander(dav, "PROPFIND", url, "1", DEMANDE_ETAGS)?;
    let distants = lire_objets(&texte(&reponse), url);
    let locaux = magasin.etags(agenda).map_err(index)?;
    let presents: HashSet<&str> = distants.iter().map(|o| o.href.as_str()).collect();
    let a_retirer: Vec<String> = locaux.keys().filter(|h| !presents.contains(h.as_str())).cloned().collect();
    let a_lire: Vec<(String, String)> = distants
        .into_iter()
        .filter(|o| locaux.get(&o.href) != Some(&o.etag))
        .map(|o| (o.href, o.etag))
        .collect();
    magasin.retirer_evenements(agenda, &a_retirer).map_err(index)?;
    let mut lus = 0;
    for lot in a_lire.chunks(LOT) {
        lus += lire_lot(magasin, agenda, url, lot, dav)?;
    }
    Ok((lus, a_retirer.len()))
}

/// Lit et range un lot d'objets. Une réponse trop volumineuse fait couper le
/// lot en deux, jusqu'à l'objet seul, rangé alors sans texte : son ETag
/// retenu, il n'est pas redemandé tant qu'il ne change pas.
fn lire_lot(magasin: &Magasin, agenda: i64, url: &str, lot: &[(String, String)], dav: &impl Dav) -> Resultat<usize> {
    let hrefs: Vec<String> = lot.iter().map(|(h, _)| h.clone()).collect();
    match demander(dav, "REPORT", url, "1", &demande_objets(&hrefs)) {
        Ok(reponse) => {
            let objets: Vec<ObjetAgenda> = lire_objets(&texte(&reponse), url)
                .into_iter()
                .map(|o| ranger(o.href, o.etag, o.ical.unwrap_or_default()))
                .collect();
            magasin.poser_evenements(agenda, &objets).map_err(index)?;
            Ok(objets.len())
        }
        Err(Erreur::Refuse(motif)) if motif == http::TROP_VOLUMINEUSE => {
            if let [(href, etag)] = lot {
                magasin.poser_evenements(agenda, &[ranger(href.clone(), etag.clone(), String::new())]).map_err(index)?;
                return Ok(1);
            }
            let (premiers, derniers) = lot.split_at(lot.len() / 2);
            Ok(lire_lot(magasin, agenda, url, premiers, dav)? + lire_lot(magasin, agenda, url, derniers, dav)?)
        }
        Err(e) => Err(e),
    }
}

// ------------------------------------------------------------- écriture

/// Motif d'une écriture refusée parce que l'objet a changé sur le serveur
/// depuis la dernière synchronisation.
pub const CONFLIT: &str = "l'événement a été modifié ailleurs entre-temps : l'agenda est relu, recommencez";

fn issue_ecriture(reponse: &Reponse) -> Resultat<()> {
    match reponse.statut {
        200..=299 => Ok(()),
        412 => Err(Erreur::Refuse(CONFLIT.into())),
        401 => Err(Erreur::Refuse("accès à l'agenda refusé".into())),
        403 => Err(Erreur::Refuse("cet agenda n'accepte pas l'écriture".into())),
        507 => Err(Erreur::Refuse("espace insuffisant sur le serveur".into())),
        statut => Err(Erreur::Protocole(format!("écriture dans l'agenda : HTTP {statut}"))),
    }
}

/// Dépose un objet : nouveau sans ETag, remplacé avec l'ETag connu — un
/// objet changé entre-temps sur le serveur n'est pas écrasé. Rend l'ETag du
/// dépôt, si le serveur le donne.
pub fn deposer(dav: &impl Dav, url: &str, ical: &str, etag: Option<&str>) -> Resultat<Option<String>> {
    let reponse = dav.ecrire("PUT", url, ical, etag)?;
    issue_ecriture(&reponse)?;
    Ok(reponse.entete("etag").map(str::to_string))
}

/// Retire un objet, s'il n'a pas changé depuis que l'index l'a lu (ETag
/// donné). SOGo ignore `If-Match` sur un DELETE : l'ETag est relu d'abord. Un
/// objet déjà absent est tenu pour retiré.
pub fn retirer(dav: &impl Dav, collection: &str, url: &str, etag: Option<&str>) -> Resultat<()> {
    if let Some(etag) = etag {
        let reponse = dav.envoyer("PROPFIND", url, "0", DEMANDE_ETAGS)?;
        match reponse.statut {
            404 | 410 => return Ok(()),
            207 => {
                let actuel = lire_objets(&texte(&reponse), collection).into_iter().next().map(|o| o.etag);
                if actuel.as_deref().is_some_and(|e| e != etag) {
                    return Err(Erreur::Refuse(CONFLIT.into()));
                }
            }
            _ => issue_ecriture(&reponse)?,
        }
    }
    let reponse = dav.ecrire("DELETE", url, "", etag)?;
    if matches!(reponse.statut, 404 | 410) {
        return Ok(());
    }
    issue_ecriture(&reponse)
}

/// Un objet prêt à ranger. Sans événement lisible (tâche, objet vide), sa
/// période ne touche aucune vue.
fn ranger(href: String, etag: String, ical: String) -> ObjetAgenda {
    let (debut, fin) = crate::agenda::etendue(&ical).unwrap_or((0, i64::MIN));
    ObjetAgenda { href, etag, ical, debut, fin }
}

#[cfg(test)]
mod tests;
