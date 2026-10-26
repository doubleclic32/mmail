//! Réponses réelles de SOGo (Mailcow), boîte d'essai renommée.

use super::*;

const COLLECTION: &str = "/SOGo/dav/alice@exemple.fr/Calendar/";
const PERSONNEL: &str = "/SOGo/dav/alice@exemple.fr/Calendar/personal/";

#[test]
fn principal_puis_collection() {
    assert_eq!(
        lire_href(include_str!("sogo-principal.xml"), true, "current-user-principal").as_deref(),
        Some("/SOGo/dav/alice@exemple.fr/")
    );
    assert_eq!(lire_href(include_str!("sogo-collection.xml"), false, "calendar-home-set").as_deref(), Some(COLLECTION));
    assert_eq!(lire_href(include_str!("sogo-collection.xml"), true, "current-user-principal"), None);
    assert_eq!(lire_href("pas du XML", true, "current-user-principal"), None);
}

#[test]
fn agendas_de_sogo() {
    // La collection, et les deux exports `personal.ics` et `personal.xml` que
    // SOGo range à côté, ne sont pas des agendas.
    let agendas = lire_agendas(include_str!("sogo-agendas.xml"));
    assert_eq!(agendas.len(), 1, "{agendas:?}");
    let a = &agendas[0];
    assert_eq!(a.href, PERSONNEL);
    assert_eq!(a.nom, "Personal Calendar");
    assert_eq!(a.couleur, "#AAAAAA");
    assert!(!a.ctag.is_empty() && a.ctag != "-1", "{}", a.ctag);
}

#[test]
fn etags_puis_objets() {
    let etags = lire_objets(include_str!("sogo-etags.xml"), PERSONNEL);
    assert_eq!(etags.len(), 5);
    assert!(etags.iter().all(|o| o.etag == "\"gcs00000000\"" && o.ical.is_none()));
    assert!(etags.iter().any(|o| o.href == format!("{PERSONNEL}mmail-essai-hebdo.ics")));

    // Multiget : l'objet absent (404) est écarté, le texte est désechappé.
    let objets = lire_objets(include_str!("sogo-objets.xml"), PERSONNEL);
    assert_eq!(objets.len(), 2);
    let hebdo = objets.iter().find(|o| o.href.ends_with("mmail-essai-hebdo.ics")).unwrap();
    let ical = hebdo.ical.as_deref().unwrap();
    assert!(ical.contains("SUMMARY:Point hebdomadaire (déplacé, essai)"), "{ical}");
    let occ = crate::agenda::occurrences(
        ical,
        "2026-10-05T00:00:00Z".parse().unwrap(),
        "2026-10-19T00:00:00Z".parse().unwrap(),
    );
    assert_eq!(occ.len(), 3);
}

#[test]
fn demande_d_objets_echappee() {
    let corps = demande_objets(&["/a/b&c.ics".into()]);
    assert!(corps.contains("<d:href>/a/b&amp;c.ics</d:href>"));
    assert!(roxmltree::Document::parse(&corps).is_ok());
    for fixe in [DEMANDE_PRINCIPAL, DEMANDE_COLLECTION, DEMANDE_AGENDAS, DEMANDE_ETAGS] {
        assert!(roxmltree::Document::parse(fixe).is_ok());
    }
}

#[test]
fn adresses() {
    let base = "https://dav.exemple.fr/SOGo/dav/";
    assert_eq!(adresse(base, "/x/y/").as_deref(), Some("https://dav.exemple.fr/x/y/"));
    assert_eq!(adresse(base, "personal/").as_deref(), Some("https://dav.exemple.fr/SOGo/dav/personal/"));
    assert_eq!(adresse("https://dav.exemple.fr", "/x").as_deref(), Some("https://dav.exemple.fr/x"));
    assert_eq!(adresse(base, "https://autre.fr/z").as_deref(), Some("https://autre.fr/z"));
    assert_eq!(adresse("http://dav.exemple.fr/", "/x"), None);
    assert_eq!(chemin("https://dav.exemple.fr/a/b/"), "/a/b/");
    assert_eq!(chemin("/a/b/"), "/a/b/");
}

// ------------------------------------------------------------- synchronisation

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::magasin::Magasin;

const ORIGINE: &str = "https://dav.exemple.fr";
const AGENDAS: &str = "/dav/alice/agendas/";

/// Un agenda du faux serveur : nom, étiquette, objets (href → ETag, texte).
struct FauxAgenda {
    nom: String,
    ctag: u32,
    objets: BTreeMap<String, (String, String)>,
}

/// Faux serveur CalDAV : découverte par `/.well-known/caldav` redirigé vers
/// `/dav/`, agendas sous `/dav/alice/agendas/`.
struct Faux {
    agendas: RefCell<BTreeMap<String, FauxAgenda>>,
    journal: RefCell<Vec<String>>,
    /// Objets rendus au plus par `calendar-multiget` ; au-delà, la réponse
    /// est « trop volumineuse ». Un objet dont le texte contient « ÉNORME »
    /// l'est toujours.
    lot_max: usize,
    /// Statut rendu à tout, à la place d'une réponse : 404 pour un serveur
    /// sans CalDAV, 401 pour des identifiants refusés.
    statut: Option<u16>,
}

fn reponse(adresse: &str, statut: u16, corps: String) -> Resultat<Reponse> {
    Ok(Reponse {
        statut,
        type_contenu: "application/xml".into(),
        redirection: None,
        corps: corps.into_bytes(),
        adresse: adresse.into(),
        entetes: vec![("dav".into(), "1, 2, calendar-access, calendar-auto-schedule".into())],
    })
}

fn multistatus(reponses: &[String]) -> String {
    format!(r#"<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://calendarserver.org/ns/">{}</D:multistatus>"#, reponses.concat())
}

fn propriete(href: &str, prop: &str) -> String {
    format!("<D:response><D:href>{href}</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop>{prop}</D:prop></D:propstat></D:response>")
}

fn xml(texte: &str) -> String {
    texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Faux {
    fn nouveau() -> Faux {
        Faux { agendas: RefCell::new(BTreeMap::new()), journal: RefCell::new(Vec::new()), lot_max: 1000, statut: None }
    }

    fn poser(&self, agenda: &str, nom: &str, objet: &str, etag: &str, ical: &str) {
        let mut agendas = self.agendas.borrow_mut();
        let a = agendas.entry(format!("{AGENDAS}{agenda}/")).or_insert_with(|| FauxAgenda { nom: nom.into(), ctag: 0, objets: BTreeMap::new() });
        a.ctag += 1;
        a.objets.insert(format!("{AGENDAS}{agenda}/{objet}.ics"), (etag.into(), ical.into()));
    }

    fn retirer(&self, agenda: &str, objet: &str) {
        let mut agendas = self.agendas.borrow_mut();
        let a = agendas.get_mut(&format!("{AGENDAS}{agenda}/")).unwrap();
        a.ctag += 1;
        a.objets.remove(&format!("{AGENDAS}{agenda}/{objet}.ics"));
    }

    fn requetes(&self) -> Vec<String> {
        self.journal.borrow_mut().drain(..).collect()
    }
}

impl Dav for Faux {
    fn envoyer(&self, methode: &str, url: &str, profondeur: &str, corps: &str) -> Resultat<Reponse> {
        let chemin = chemin(url).to_string();
        self.journal.borrow_mut().push(format!("{methode} {chemin} {profondeur}"));
        if let Some(statut) = self.statut {
            return reponse(url, statut, String::new());
        }
        let agendas = self.agendas.borrow();
        if methode == "OPTIONS" {
            return reponse(url, 200, String::new());
        }
        if profondeur == "0" && agendas.values().any(|a| a.objets.contains_key(&chemin)) {
            // ETag d'un objet seul, relu avant un retrait.
            let (etag, _) = agendas.values().find_map(|a| a.objets.get(&chemin)).unwrap();
            return reponse(url, 207, multistatus(&[propriete(&chemin, &format!("<D:getetag>{}</D:getetag>", xml(etag)))]));
        }
        match (methode, chemin.as_str(), profondeur) {
            // Redirigé vers /dav/ : l'adresse rendue est celle d'arrivée.
            ("PROPFIND", "/.well-known/caldav", "0") => reponse(
                &format!("{ORIGINE}/dav/"),
                207,
                multistatus(&[propriete("/dav/", "<D:current-user-principal><D:href>/dav/alice/</D:href></D:current-user-principal>")]),
            ),
            ("PROPFIND", "/dav/alice/", "0") => reponse(
                url,
                207,
                multistatus(&[propriete("/dav/alice/", &format!("<C:calendar-home-set><D:href>{AGENDAS}</D:href></C:calendar-home-set>"))]),
            ),
            ("PROPFIND", AGENDAS, "1") => {
                let mut r = vec![propriete(AGENDAS, "<D:resourcetype><D:collection/></D:resourcetype>")];
                for (href, a) in agendas.iter() {
                    r.push(propriete(
                        href,
                        &format!(
                            "<D:displayname>{}</D:displayname><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><A:getctag>{}</A:getctag>",
                            a.nom, a.ctag
                        ),
                    ));
                }
                reponse(url, 207, multistatus(&r))
            }
            ("PROPFIND", agenda, "1") if agendas.contains_key(agenda) => {
                let mut r = vec![propriete(agenda, "<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>")];
                for (href, (etag, _)) in &agendas[agenda].objets {
                    r.push(propriete(href, &format!("<D:getetag>{}</D:getetag>", xml(etag))));
                }
                reponse(url, 207, multistatus(&r))
            }
            ("REPORT", agenda, "1") if agendas.contains_key(agenda) => {
                let doc = roxmltree::Document::parse(corps).unwrap();
                let hrefs: Vec<&str> = doc.descendants().filter(|n| est(n, DAV, "href")).filter_map(|n| n.text()).collect();
                let objets = &agendas[agenda].objets;
                let enorme = hrefs.iter().any(|h| objets.get(*h).is_some_and(|(_, i)| i.contains("ÉNORME")));
                if hrefs.len() > self.lot_max || enorme {
                    return Err(Erreur::Refuse(http::TROP_VOLUMINEUSE.into()));
                }
                let r: Vec<String> = hrefs
                    .iter()
                    .map(|h| match objets.get(*h) {
                        Some((etag, ical)) => propriete(h, &format!("<D:getetag>{}</D:getetag><C:calendar-data>{}</C:calendar-data>", xml(etag), xml(ical))),
                        None => format!("<D:response><D:href>{h}</D:href><D:status>HTTP/1.1 404 Not Found</D:status></D:response>"),
                    })
                    .collect();
                reponse(url, 207, multistatus(&r))
            }
            _ => reponse(url, 404, String::new()),
        }
    }

    /// PUT et DELETE sur les objets, ETag vérifiés comme le fait SOGo : au PUT
    /// seulement.
    fn ecrire(&self, methode: &str, url: &str, _type: &str, corps: &str, condition: Condition) -> Resultat<Reponse> {
        let si_etag = match condition {
            Condition::SiEtag(e) => Some(e),
            _ => None,
        };
        let chemin = chemin(url).to_string();
        self.journal.borrow_mut().push(format!("{methode} {chemin} {}", si_etag.unwrap_or("-")));
        if let Some(statut) = self.statut {
            return reponse(url, statut, String::new());
        }
        let mut agendas = self.agendas.borrow_mut();
        let Some((_, agenda)) = agendas.iter_mut().find(|(href, _)| chemin.starts_with(href.as_str())) else {
            return reponse(url, 404, String::new());
        };
        let actuel = agenda.objets.get(&chemin).map(|(e, _)| e.clone());
        match methode {
            "PUT" => {
                if si_etag.is_some() && si_etag.map(str::to_string) != actuel {
                    return reponse(url, 412, String::new());
                }
                let version = actuel.as_deref().and_then(|e| e.trim_matches('"').parse::<u32>().ok()).map_or(0, |n| n + 1);
                agenda.objets.insert(chemin, (format!("\"{version}\""), corps.to_string()));
                agenda.ctag += 1;
                reponse(url, if actuel.is_some() { 204 } else { 201 }, String::new())
            }
            "DELETE" => match agenda.objets.remove(&chemin) {
                Some(_) => {
                    agenda.ctag += 1;
                    reponse(url, 204, String::new())
                }
                None => reponse(url, 404, String::new()),
            },
            _ => reponse(url, 405, String::new()),
        }
    }
}

impl Faux {
    /// Écritures reçues : (méthode, chemin, If-Match).
    fn ecritures(&self) -> Vec<String> {
        self.journal.borrow().iter().filter(|l| l.starts_with("PUT") || l.starts_with("DELETE")).cloned().collect()
    }
}

fn evenement(uid: &str, jour: u32, titre: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART:202610{jour:02}T090000Z\r\n\
DTEND:202610{jour:02}T100000Z\r\nSUMMARY:{titre}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

fn profil() -> (Magasin, i64) {
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte("alice@exemple.fr", "dav.exemple.fr", 993, "alice@exemple.fr").unwrap();
    (magasin, compte)
}

fn synchro(magasin: &Magasin, compte: i64, faux: &Faux, maintenant: i64, insister: bool) -> Resultat<Bilan> {
    synchroniser(magasin, compte, "dav.exemple.fr", faux, maintenant, insister)
}

/// Titres des événements d'octobre 2026 des agendas affichés.
fn titres(magasin: &Magasin) -> Vec<String> {
    let mut t: Vec<String> = magasin
        .evenements_periode(1790812800, 1793491200)
        .unwrap()
        .iter()
        .flat_map(|(_, _, ical)| crate::agenda::occurrences(ical, "2026-10-01T00:00:00Z".parse().unwrap(), "2026-11-01T00:00:00Z".parse().unwrap()))
        .map(|o| o.resume)
        .collect();
    t.sort();
    t
}

#[test]
fn decouverte_puis_synchronisations_successives() {
    let (magasin, compte) = profil();
    let faux = Faux::nouveau();
    faux.poser("perso", "Personnel", "a", "\"1\"", &evenement("a", 5, "Alpha"));
    faux.poser("perso", "Personnel", "b", "\"1\"", &evenement("b", 6, "Bêta"));
    faux.poser("perso", "Personnel", "c", "\"1\"", &evenement("c", 7, "Gamma"));

    let bilan = synchro(&magasin, compte, &faux, 1000, false).unwrap();
    assert_eq!(bilan, Bilan { change: true, agendas: 1, lus: 3, retires: 0 });
    assert_eq!(magasin.caldav(compte).unwrap().0, format!("{ORIGINE}{AGENDAS}"));
    let agendas = magasin.agendas().unwrap();
    assert_eq!(agendas.len(), 1);
    assert_eq!((agendas[0].nom.as_str(), agendas[0].adresse.as_str()), ("Personnel", "https://dav.exemple.fr/dav/alice/agendas/perso/"));
    assert_eq!(titres(&magasin), ["Alpha", "Bêta", "Gamma"]);
    assert_eq!(
        faux.requetes(),
        [
            "PROPFIND /.well-known/caldav 0",
            "PROPFIND /dav/alice/ 0",
            "PROPFIND /dav/alice/agendas/ 1",
            // Une fois pour toutes : le serveur distribue-t-il les invitations ?
            "OPTIONS /dav/alice/agendas/ 0",
            "PROPFIND /dav/alice/agendas/perso/ 1",
            "REPORT /dav/alice/agendas/perso/ 1",
        ]
    );
    assert_eq!(magasin.planification(compte).unwrap(), Some(true));

    // Rien n'a changé : une seule requête, celle des étiquettes.
    assert_eq!(synchro(&magasin, compte, &faux, 2000, false).unwrap(), Bilan { change: false, agendas: 1, lus: 0, retires: 0 });
    assert_eq!(faux.requetes(), ["PROPFIND /dav/alice/agendas/ 1"]);

    // Un objet modifié, un retiré, un ajouté : deux lus, un retiré.
    faux.poser("perso", "Personnel", "a", "\"2\"", &evenement("a", 5, "Alpha modifié"));
    faux.retirer("perso", "b");
    faux.poser("perso", "Personnel", "d", "\"1\"", &evenement("d", 8, "Delta"));
    assert_eq!(synchro(&magasin, compte, &faux, 3000, false).unwrap(), Bilan { change: true, agendas: 1, lus: 2, retires: 1 });
    assert_eq!(titres(&magasin), ["Alpha modifié", "Delta", "Gamma"]);

    // Un agenda masqué le reste après synchronisation ; un agenda disparu du
    // serveur disparaît de l'index, objets compris.
    faux.poser("travail", "Travail", "t", "\"1\"", &evenement("t", 9, "Tau"));
    synchro(&magasin, compte, &faux, 4000, false).unwrap();
    let travail = magasin.agendas().unwrap().into_iter().find(|a| a.nom == "Travail").unwrap();
    magasin.afficher_agenda(travail.id, false).unwrap();
    faux.poser("travail", "Travail", "u", "\"1\"", &evenement("u", 10, "Upsilon"));
    synchro(&magasin, compte, &faux, 5000, false).unwrap();
    assert!(!magasin.agendas().unwrap().iter().find(|a| a.nom == "Travail").unwrap().affiche);
    assert_eq!(titres(&magasin), ["Alpha modifié", "Delta", "Gamma"]);
    magasin.afficher_agenda(travail.id, true).unwrap();
    assert_eq!(titres(&magasin), ["Alpha modifié", "Delta", "Gamma", "Tau", "Upsilon"]);
    faux.agendas.borrow_mut().remove("/dav/alice/agendas/travail/");
    assert!(synchro(&magasin, compte, &faux, 6000, false).unwrap().change);
    assert_eq!(magasin.agendas().unwrap().len(), 1);
    assert_eq!(magasin.etags(travail.id).unwrap().len(), 0);
}

#[test]
fn lots_trop_volumineux_coupes_en_deux() {
    let (magasin, compte) = profil();
    let mut faux = Faux::nouveau();
    faux.lot_max = 2;
    for i in 1..=5u32 {
        faux.poser("perso", "Personnel", &format!("o{i}"), "\"1\"", &evenement(&format!("o{i}"), i, &format!("Objet {i}")));
    }
    faux.poser("perso", "Personnel", "gros", "\"1\"", &evenement("gros", 20, "ÉNORME"));
    let bilan = synchro(&magasin, compte, &faux, 1000, false).unwrap();
    assert_eq!(bilan.lus, 6);
    assert_eq!(titres(&magasin), ["Objet 1", "Objet 2", "Objet 3", "Objet 4", "Objet 5"]);
    // L'objet trop gros, rangé sans texte avec son ETag, n'est pas redemandé.
    faux.poser("perso", "Personnel", "o6", "\"1\"", &evenement("o6", 6, "Objet 6"));
    faux.requetes();
    assert_eq!(synchro(&magasin, compte, &faux, 2000, false).unwrap().lus, 1);
    assert_eq!(faux.requetes().iter().filter(|r| r.starts_with("REPORT")).count(), 1);
}

#[test]
fn boite_sans_agenda_et_identifiants_refuses() {
    let (magasin, compte) = profil();
    let mut faux = Faux::nouveau();
    faux.statut = Some(404);
    assert_eq!(synchro(&magasin, compte, &faux, 1000, false).unwrap(), Bilan::default());
    // Le serveur de la boîte, puis l'adresse de SOGo ; jamais le domaine de
    // l'adresse.
    assert_eq!(faux.requetes(), ["PROPFIND /.well-known/caldav 0", "PROPFIND /SOGo/dav/ 0"]);
    assert_eq!(magasin.caldav(compte).unwrap(), (String::new(), 1000));
    // Moins d'un jour après : pas de nouvel essai, sauf à la demande.
    synchro(&magasin, compte, &faux, 1000 + 3600, false).unwrap();
    assert!(faux.requetes().is_empty());
    synchro(&magasin, compte, &faux, 1000 + 3600, true).unwrap();
    assert_eq!(faux.requetes().len(), 2);
    synchro(&magasin, compte, &faux, 1000 + 3600 + 86400, false).unwrap();
    assert_eq!(faux.requetes().len(), 2);

    faux.statut = Some(401);
    let erreur = synchro(&magasin, compte, &faux, 1000 + 3 * 86400, true).unwrap_err();
    assert!(matches!(erreur, Erreur::Refuse(_)), "{erreur}");
}

#[test]
fn droits_d_ecriture_des_agendas() {
    let xml = |privileges: &str| {
        multistatus(&[propriete(
            "/dav/alice/agendas/partage/",
            &format!(
                "<D:displayname>Partagé</D:displayname><D:resourcetype><D:collection/><C:calendar/></D:resourcetype>\
<D:current-user-privilege-set>{privileges}</D:current-user-privilege-set>"
            ),
        )])
    };
    let lecture = lire_agendas(&xml("<D:privilege><D:read/></D:privilege><D:privilege><D:read-current-user-privilege-set/></D:privilege>"));
    assert!(!lecture[0].ecriture);
    let ecriture = lire_agendas(&xml("<D:privilege><D:read/></D:privilege><D:privilege><D:write-content/></D:privilege>"));
    assert!(ecriture[0].ecriture);
    // Sans la propriété : écriture supposée, le serveur refusera s'il le faut.
    assert!(lire_agendas(include_str!("sogo-agendas.xml"))[0].ecriture);
}

#[test]
fn depot_conflit_et_retrait() {
    let (magasin, compte) = profil();
    let faux = Faux::nouveau();
    faux.poser("perso", "Personnel", "a", "\"1\"", &evenement("a", 5, "Alpha"));
    synchro(&magasin, compte, &faux, 1000, false).unwrap();
    let agenda = format!("{ORIGINE}{AGENDAS}perso/");

    // Nouvel objet : sans condition.
    deposer(&faux, &format!("{agenda}b.ics"), &evenement("b", 6, "Bêta"), None).unwrap();
    synchro(&magasin, compte, &faux, 2000, false).unwrap();
    assert_eq!(titres(&magasin), ["Alpha", "Bêta"]);

    // Modification avec l'ETag connu, puis avec un ETag périmé : refusée.
    deposer(&faux, &format!("{agenda}a.ics"), &evenement("a", 5, "Alpha 2"), Some("\"1\"")).unwrap();
    let conflit = deposer(&faux, &format!("{agenda}a.ics"), &evenement("a", 5, "Alpha 3"), Some("\"1\"")).unwrap_err();
    assert!(conflit.to_string().contains(CONFLIT), "{conflit}");
    synchro(&magasin, compte, &faux, 3000, false).unwrap();
    assert_eq!(titres(&magasin), ["Alpha 2", "Bêta"]);

    // Retrait : l'ETag est relu avant le DELETE, que SOGo ne conditionne pas.
    let etags = magasin.etags(magasin.agendas().unwrap()[0].id).unwrap();
    let href_b = format!("{AGENDAS}perso/b.ics");
    faux.requetes();
    let perime = retirer(&faux, &agenda, &format!("{agenda}b.ics"), Some("\"périmé\"")).unwrap_err();
    assert!(perime.to_string().contains(CONFLIT));
    assert!(faux.ecritures().is_empty());
    retirer(&faux, &agenda, &format!("{agenda}b.ics"), Some(&etags[&href_b])).unwrap();
    assert_eq!(faux.ecritures(), [format!("DELETE {href_b} {}", etags[&href_b])]);
    // Déjà absent : rien à faire.
    retirer(&faux, &agenda, &format!("{agenda}b.ics"), Some(&etags[&href_b])).unwrap();
    synchro(&magasin, compte, &faux, 4000, false).unwrap();
    assert_eq!(titres(&magasin), ["Alpha 2"]);
}
