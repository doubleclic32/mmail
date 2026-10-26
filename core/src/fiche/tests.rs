// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn le_26_octobre() -> DateTime<Utc> {
    "2026-10-26T10:15:00Z".parse().unwrap()
}

/// Lignes logiques d'une vCard écrite : dépliées, sans BEGIN ni END.
fn lignes(vcard: &str) -> Vec<String> {
    Carte::analyser(vcard).unwrap().lignes.into_iter().map(|l| l.texte).collect()
}

/// Une fiche comme SOGo et un téléphone en écrivent : groupes, étiquettes
/// Apple, photo repliée, propriétés propres, échappements.
const SOGO: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nPRODID:-//Inverse inc.//SOGo 5.11//EN\r\nUID:6A1-6718\r\n\
N:Dupont;Marie;Anne;Mme;\r\nFN:Marie Dupont\r\nORG:Transports\\, Martin;Accueil\r\nTITLE:Secrétaire\r\n\
item1.EMAIL;TYPE=work,pref:marie@transports-martin.fr\r\nitem1.X-ABLabel:_$!<Work>!$_\r\nEMAIL;TYPE=home:Marie@Exemple.FR\r\n\
TEL;TYPE=cell:06 12 34\r\n 56 78\r\nTEL;TYPE=work,voice:05 53 00 00 00\r\n\
ADR;TYPE=work:BP 12;;1 rue du Port;Villeneuve;Région;12345;France\r\n\
NOTE:ligne\\nsuivante\\; fin\r\nCATEGORIES:Clients,Transport\r\nX-MOZILLA-HTML:FALSE\r\n\
PHOTO;ENCODING=b;TYPE=JPEG:/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4n\r\n ICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/\r\n\
REV:20250101T000000Z\r\nEND:VCARD\r\n";

#[test]
fn lecture_d_une_fiche() {
    let f = lire(SOGO).unwrap();
    assert_eq!(f.nom_affiche, "Marie Dupont");
    assert_eq!((f.prenom.as_str(), f.nom.as_str()), ("Marie", "Dupont"));
    assert_eq!(f.organisation, "Transports, Martin");
    assert_eq!(f.fonction, "Secrétaire");
    assert_eq!(f.note, "ligne\nsuivante; fin");
    let c: Vec<(&str, &str)> = f.courriels.iter().map(|c| (c.genre.as_str(), c.valeur.as_str())).collect();
    assert_eq!(c, vec![("travail", "marie@transports-martin.fr"), ("domicile", "Marie@Exemple.FR")]);
    let t: Vec<(&str, &str)> = f.telephones.iter().map(|c| (c.genre.as_str(), c.valeur.as_str())).collect();
    assert_eq!(t, vec![("portable", "06 12 3456 78"), ("travail", "05 53 00 00 00")]);
    assert_eq!(f.courriels[1].cle, Some(1));
    assert_eq!(
        f.adresse,
        Adresse { genre: "travail".into(), rue: "1 rue du Port".into(), code_postal: "12345".into(), ville: "Villeneuve".into(), pays: "France".into() }
    );
    // vCard 2.1 : paramètres nus.
    let f = lire("BEGIN:VCARD\nVERSION:2.1\nN:Martin;Paul\nTEL;CELL:0612\nTEL;WORK;FAX:0553\nEND:VCARD").unwrap();
    assert_eq!(f.telephones.iter().map(|c| c.genre.as_str()).collect::<Vec<_>>(), vec!["portable", "fax"]);
    assert!(lire("pas une fiche").is_none());
}

#[test]
fn modification_sans_toucher_au_reste() {
    let origine = lire(SOGO).unwrap();
    let mut saisie = origine.clone();
    saisie.organisation = "Martin SAS".into();
    saisie.courriels[1].valeur = "marie.dupont@exemple.fr".into();
    saisie.telephones.push(Coordonnee { genre: "fax".into(), valeur: "05 53 11 11 11".into(), cle: None });
    let ecrit = modifier(SOGO, &origine, &saisie, le_26_octobre()).unwrap();
    let avant = lignes(SOGO);
    let apres = lignes(&ecrit);
    // Tout ce que la saisie ne touche pas est rendu au caractère près, à sa
    // place : photo, catégories, propriétés propres, groupe et étiquette.
    for gardee in [
        "PRODID:-//Inverse inc.//SOGo 5.11//EN",
        "N:Dupont;Marie;Anne;Mme;",
        "FN:Marie Dupont",
        "TITLE:Secrétaire",
        "item1.EMAIL;TYPE=work,pref:marie@transports-martin.fr",
        "item1.X-ABLabel:_$!<Work>!$_",
        "TEL;TYPE=cell:06 12 3456 78",
        "TEL;TYPE=work,voice:05 53 00 00 00",
        "ADR;TYPE=work:BP 12;;1 rue du Port;Villeneuve;Région;12345;France",
        "NOTE:ligne\\nsuivante\\; fin",
        "CATEGORIES:Clients,Transport",
        "X-MOZILLA-HTML:FALSE",
    ] {
        assert!(apres.contains(&gardee.to_string()), "{gardee} perdue :\n{ecrit}");
    }
    let photo = avant.iter().find(|l| l.starts_with("PHOTO")).unwrap();
    assert!(apres.contains(photo));
    // L'organisation garde son service ; l'adresse changée garde son type.
    assert!(apres.contains(&"ORG:Martin SAS;Accueil".to_string()));
    assert!(apres.contains(&"EMAIL;TYPE=home:marie.dupont@exemple.fr".to_string()));
    let tel: Vec<&String> = apres.iter().filter(|l| l.starts_with("TEL")).collect();
    assert_eq!(tel.last().unwrap().as_str(), "TEL;TYPE=fax:05 53 11 11 11", "après les autres numéros");
    assert!(apres.contains(&"REV:20261026T101500Z".to_string()));
    assert_eq!(avant.len() + 1, apres.len());
    // Lignes pliées à 75 octets, fins de ligne CRLF.
    assert!(ecrit.split("\r\n").all(|l| l.len() <= 75));
    assert!(ecrit.starts_with("BEGIN:VCARD\r\nVERSION:3.0\r\n") && ecrit.ends_with("END:VCARD\r\n"));
    // Rien de changé : la vCard reste la même, ligne pour ligne.
    let meme = modifier(SOGO, &origine, &origine, le_26_octobre()).unwrap();
    assert_eq!(lignes(&meme), avant);
}

#[test]
fn retrait_genre_et_nom() {
    let origine = lire(SOGO).unwrap();
    let mut saisie = origine.clone();
    // L'adresse du groupe « item1 » retirée : son étiquette part avec elle.
    saisie.courriels.remove(0);
    // Le portable devient le numéro du domicile : `pref`, `voice`… restent.
    saisie.telephones[1].genre = "domicile".into();
    saisie.prenom = "Marie-Anne".into();
    saisie.nom_affiche = "Marie-Anne Dupont".into();
    let ecrit = modifier(SOGO, &origine, &saisie, le_26_octobre()).unwrap();
    let apres = lignes(&ecrit);
    assert!(!apres.iter().any(|l| l.starts_with("item1.")), "{ecrit}");
    assert!(apres.contains(&"TEL;TYPE=home,voice:05 53 00 00 00".to_string()), "{ecrit}");
    assert!(apres.contains(&"N:Dupont;Marie-Anne;Anne;Mme;".to_string()), "autres composantes gardées");
    assert!(apres.contains(&"FN:Marie-Anne Dupont".to_string()));
    // Rubriques vidées : elles partent.
    let mut vide = origine.clone();
    vide.fonction.clear();
    vide.note.clear();
    vide.adresse = Adresse::default();
    let apres = lignes(&modifier(SOGO, &origine, &vide, le_26_octobre()).unwrap());
    assert!(!apres.iter().any(|l| l.starts_with("TITLE") || l.starts_with("NOTE") || l.starts_with("ADR")));
}

#[test]
fn vcard_4() {
    let v4 = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:urn:uuid:1\r\nFN:Paul\r\nTEL;VALUE=uri;TYPE=cell:tel:+33612345678\r\n\
EMAIL:paul@x.fr\r\nEND:VCARD\r\n";
    let origine = lire(v4).unwrap();
    assert_eq!(origine.telephones[0].valeur, "+33612345678");
    let mut saisie = origine.clone();
    saisie.telephones[0].valeur = "+33 6 98 76 54 32".into();
    let apres = lignes(&modifier(v4, &origine, &saisie, le_26_octobre()).unwrap());
    assert!(apres.contains(&"TEL;VALUE=uri;TYPE=cell:tel:+33698765432".to_string()), "{apres:?}");
    // Pas de N ajouté en 4.0, pas de REV qui n'y était pas.
    assert!(!apres.iter().any(|l| l.starts_with("N:") || l.starts_with("REV")));
}

#[test]
fn creation() {
    let f = Fiche {
        prenom: "Jean".into(),
        nom: "Martin".into(),
        organisation: "A; B, C".into(),
        fonction: "Chef d'équipe".into(),
        courriels: vec![Coordonnee { genre: "travail".into(), valeur: "jean@martin.fr".into(), cle: None }],
        telephones: vec![
            Coordonnee { genre: "portable".into(), valeur: "06 00 00 00 01".into(), cle: None },
            Coordonnee { genre: "autre".into(), valeur: "".into(), cle: None },
        ],
        adresse: Adresse { genre: "domicile".into(), rue: "2 place du Marché".into(), code_postal: "47000".into(), ville: "Agen".into(), pays: String::new() },
        note: "Première ligne\nseconde, avec \\ antislash ; et un très long texte qui dépasse soixante-quinze octets à coup sûr".into(),
        ..Default::default()
    };
    let ecrit = creer(&f, "abc-123", le_26_octobre()).unwrap();
    let l = lignes(&ecrit);
    assert_eq!(&l[..3], &["VERSION:3.0".to_string(), format!("PRODID:-//MMEDIA//MMail {}//FR", env!("CARGO_PKG_VERSION")), "UID:abc-123".into()]);
    for attendue in [
        "FN:Jean Martin",
        "N:Martin;Jean;;;",
        "ORG:A\\; B\\, C",
        "TITLE:Chef d'équipe",
        "EMAIL;TYPE=work:jean@martin.fr",
        "TEL;TYPE=cell:06 00 00 00 01",
        "ADR;TYPE=home:;;2 place du Marché;Agen;;47000;",
        "REV:20261026T101500Z",
    ] {
        assert!(l.contains(&attendue.to_string()), "{attendue} absente :\n{ecrit}");
    }
    assert_eq!(l.iter().filter(|l| l.starts_with("TEL")).count(), 1, "numéro vide ignoré");
    assert!(ecrit.split("\r\n").all(|l| l.len() <= 75));
    // Relue, la fiche est celle saisie.
    let relue = lire(&ecrit).unwrap();
    assert_eq!(relue.note, f.note);
    assert_eq!(relue.organisation, f.organisation);
    assert_eq!(relue.adresse, f.adresse);
    // Sans rien, ou avec une adresse fausse : refusée.
    assert!(creer(&Fiche::default(), "x", le_26_octobre()).is_err());
    let fausse = Fiche { courriels: vec![Coordonnee { genre: "autre".into(), valeur: "pas une adresse".into(), cle: None }], ..Default::default() };
    assert!(creer(&fausse, "x", le_26_octobre()).unwrap_err().contains("invalide"));
    // Un nom seul ou une adresse seule suffit ; le nom affiché suit.
    let seule = Fiche { courriels: vec![Coordonnee { genre: "autre".into(), valeur: "x@y.fr".into(), cle: None }], ..Default::default() };
    let l = lignes(&creer(&seule, "x", le_26_octobre()).unwrap());
    assert!(l.contains(&"FN:x@y.fr".to_string()) && l.contains(&"N:;;;;".to_string()) && l.contains(&"EMAIL:x@y.fr".to_string()));
}

#[test]
fn modification_sur_une_version_plus_recente() {
    // Quelqu'un a changé la fonction sur le serveur ; la saisie, partie de
    // l'ancienne version, change l'organisation : les deux restent.
    let origine = lire(SOGO).unwrap();
    let recente = SOGO.replace("TITLE:Secrétaire", "TITLE:Directrice");
    let mut saisie = origine.clone();
    saisie.organisation = "Martin SAS".into();
    let apres = lignes(&modifier(&recente, &origine, &saisie, le_26_octobre()).unwrap());
    assert!(apres.contains(&"TITLE:Directrice".to_string()));
    assert!(apres.contains(&"ORG:Martin SAS;Accueil".to_string()));
}

#[test]
fn reprise_apres_conflit() {
    let origine = lire(SOGO).unwrap();
    let mut courant = origine.clone();
    courant.fonction = "Directrice".into();
    courant.organisation = "Martin Transports".into();
    courant.telephones.push(Coordonnee { genre: "domicile".into(), valeur: "05 00 00 00 09".into(), cle: Some(2) });
    courant.etag = "\"2\"".into();
    let mut saisie = origine.clone();
    saisie.organisation = "Martin SAS".into();
    saisie.note = "rappeler".into();
    saisie.telephones.remove(0);
    let (f, conflits) = reprendre(&courant, &origine, &saisie);
    // Changé des deux côtés : la saisie l'emporte, et c'est dit.
    assert_eq!(conflits, vec!["organisation".to_string(), "téléphones".to_string()]);
    assert_eq!(f.organisation, "Martin SAS");
    // Changé d'un seul côté : chacun garde le sien.
    assert_eq!(f.fonction, "Directrice");
    assert_eq!(f.note, "rappeler");
    assert_eq!(f.etag, "\"2\"");
    // Le portable retiré par la saisie, le numéro ajouté sur le serveur gardé.
    let t: Vec<&str> = f.telephones.iter().map(|c| c.valeur.as_str()).collect();
    assert_eq!(t, vec!["05 53 00 00 00", "05 00 00 00 09"]);
    assert_eq!(f.telephones[0].cle, Some(1), "clés de la version du serveur");
    // Rien de changé des deux côtés : aucun conflit.
    assert!(reprendre(&courant, &origine, &origine).1.is_empty());
}

#[test]
fn fiches_vides_et_doublons() {
    let fiche = |vcard: &str| lire(&format!("BEGIN:VCARD\nVERSION:3.0\n{vcard}\nEND:VCARD")).unwrap();
    let fiches = vec![
        (1, fiche("FN:Jean Martin\nEMAIL:jean@martin.fr")),
        (2, fiche("FN:\nN:;;;;")),
        (3, fiche("FN:J. Martin\nEMAIL:JEAN@martin.fr\nTEL:06 00 00 00 01\nORG:Martin SA")),
        (4, fiche("FN:Paul Durand\nTEL:+33 6 11 22 33 44")),
        (5, fiche("FN:DURAND Paul\nTEL:06 11 22 33 44")),
        (6, fiche("FN:Paul Durand\nEMAIL:paul@ailleurs.fr")),
        (7, fiche("FN:Lucie Bernard")),
        (8, fiche("FN:Lucie  Bernard")),
        (9, fiche("FN:Autre\nEMAIL:autre@x.fr")),
        (10, fiche("FN:\nN:;;;;")),
    ];
    assert!(est_vide(&fiches[1].1) && !est_vide(&fiches[6].1));
    assert!(sans_coordonnee(&fiches[6].1) && !sans_coordonnee(&fiches[0].1) && !sans_coordonnee(&fiches[1].1));
    // 1 et 3 : même adresse (casse ignorée), la plus complète gardée. 4 et 5 :
    // même nom, même numéro (+33 = 0). 6 a le même nom mais une adresse à
    // lui et pas de numéro commun : à part. 7 et 8 : même nom, sans adresse.
    // Les fiches vides ne sont d'aucun groupe.
    assert_eq!(doublons(&fiches), vec![vec![3, 1], vec![4, 5], vec![7, 8]]);
}

#[test]
fn fusion() {
    let gardee = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:a\r\nFN:Jean Martin\r\nN:Martin;Jean;;;\r\nEMAIL;TYPE=work:jean@martin.fr\r\n\
TEL;TYPE=cell:06 00 00 00 01\r\nX-PROPRE:garde\r\nEND:VCARD\r\n";
    let autre = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:b\r\nFN:J. Martin\r\nEMAIL:JEAN@martin.fr\r\nEMAIL;TYPE=home:jean@perso.fr\r\n\
TEL:+33 6 00 00 00 01\r\nTEL;TYPE=work:05 53 00 00 00\r\nORG:Martin SA\r\nNOTE:client depuis 2019\r\n\
PHOTO;ENCODING=b;TYPE=PNG:iVBORw0KGgo=\r\nEND:VCARD\r\n";
    let ecrit = fusionner(&[gardee, autre], le_26_octobre()).unwrap();
    let l = lignes(&ecrit);
    for attendue in [
        "UID:a",
        "FN:Jean Martin",
        "X-PROPRE:garde",
        "EMAIL;TYPE=work:jean@martin.fr",
        "EMAIL;TYPE=home:jean@perso.fr",
        "TEL;TYPE=cell:06 00 00 00 01",
        "TEL;TYPE=work:05 53 00 00 00",
        "ORG:Martin SA",
        "NOTE:client depuis 2019",
        "PHOTO;ENCODING=b;TYPE=PNG:iVBORw0KGgo=",
    ] {
        assert!(l.contains(&attendue.to_string()), "{attendue} absente :\n{ecrit}");
    }
    assert_eq!(l.iter().filter(|x| x.starts_with("EMAIL")).count(), 2, "adresse en double comptée une fois");
    assert_eq!(l.iter().filter(|x| x.starts_with("TEL")).count(), 2, "+33 = 0");
    assert!(!l.contains(&"UID:b".to_string()));
}

#[test]
fn cles_et_correspondant() {
    assert_eq!(cle_telephone("+33 (0)6 12"), "0612");
    assert_eq!(cle_telephone("+33 6 12 34 56 78"), cle_telephone("06.12.34.56.78"));
    assert_eq!(cle_telephone("0033 6 12 34 56 78"), "0612345678");
    assert_eq!(cle_telephone("tel:+41221234567"), "41221234567");
    assert_eq!(cle_courriel(" mailto:Jean@X.FR "), "jean@x.fr");
    let f = depuis_adresse("DUPONT Jean", "jean@dupont.fr");
    assert_eq!((f.prenom.as_str(), f.nom.as_str(), f.nom_affiche.as_str()), ("Jean", "DUPONT", "DUPONT Jean"));
    let f = depuis_adresse("\"Marie Claire Durand\"", "mc@d.fr");
    assert_eq!((f.prenom.as_str(), f.nom.as_str()), ("Marie", "Claire Durand"));
    let f = depuis_adresse("jean@dupont.fr", "jean@dupont.fr");
    assert!(f.nom_affiche.is_empty() && f.nom.is_empty());
    assert_eq!(f.courriels[0].valeur, "jean@dupont.fr");
    assert_eq!(resume(&lire("BEGIN:VCARD\nFN:Jean\nEMAIL:j@x.fr\nEND:VCARD").unwrap()), "Jean · j@x.fr");
    assert_eq!(resume(&Fiche::default()), "(fiche vide)");
}
