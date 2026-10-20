// SPDX-License-Identifier: GPL-3.0-or-later
//! Rédaction : fabriquer le message à envoyer, et préparer une réponse, un
//! transfert ou la reprise d'un brouillon à partir d'un message existant.
//!
//! Le message est fabriqué par `mail-builder`, le pendant de `mail-parser` :
//! encodage des en-têtes accentués, des noms de pièces jointes, du corps. Les
//! réponses et transferts suivent la présentation d'Outlook (décision 1) :
//! « RE : » et « TR : », et un bloc « De / Envoyé / À / Objet » au-dessus du
//! texte d'origine, plutôt que des lignes préfixées de « > ».

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use mail_builder::headers::address::Address as AdresseSortante;
use mail_builder::headers::content_type::ContentType;
use mail_builder::headers::date::Date;
use mail_builder::headers::raw::Raw;
use mail_builder::mime::MimePart;
use mail_builder::MessageBuilder;
use mail_parser::{Address, DateTime, MessageParser};
use serde::{Deserialize, Serialize};

/// Ce que la fenêtre de rédaction transmet au noyau, en JSON.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Redaction {
    /// Désigne la fenêtre de rédaction : les réponses du noyau le rappellent.
    pub jeton: String,
    /// Adresse de l'expéditeur (celle du compte), et son nom affiché.
    pub de: String,
    pub nom: String,
    /// Destinataires tels que saisis : « Nom <a@b.fr>; c@d.fr ».
    pub a: String,
    pub cc: String,
    pub cci: String,
    pub objet: String,
    pub texte: String,
    /// Corps HTML, vide pour un message en texte brut.
    pub html: String,
    /// Chemins des fichiers joints.
    pub pieces: Vec<String>,
    pub en_reponse_a: String,
    pub references: Vec<String>,
    /// Brouillon que cet envoi ou cet enregistrement remplace (0 : aucun), et
    /// le dossier qui le porte (vide : celui des brouillons).
    pub brouillon_uid: u32,
    pub brouillon_chemin: String,
    /// Importance : 1 haute, 0 normale, -1 basse.
    pub importance: i8,
    /// Accusé de remise, demandé au serveur (DSN).
    pub accuse_remise: bool,
    /// Confirmation de lecture, demandée au destinataire (MDN).
    pub confirmation_lecture: bool,
    /// Heure d'envoi voulue, en secondes Unix (0 : tout de suite).
    pub envoi_differe: i64,
    /// Message auquel on répond ou que l'on transfère : il reçoit `\Answered`
    /// ou `$Forwarded` une fois l'envoi fait.
    pub origine_chemin: String,
    pub origine_uid: u32,
    pub origine_mode: String,
}

/// Une image que le corps HTML affiche (signature), intégrée au message et
/// désignée par `cid:`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageIntegree {
    pub cid: String,
    pub type_mime: String,
    pub contenu: Vec<u8>,
}

/// Une pièce jointe lue sur le disque, prête à être jointe.
pub struct Fichier {
    pub nom: String,
    pub contenu: Vec<u8>,
}

/// Le message fabriqué, en deux versions : celle qui part (sans `Bcc`, que
/// seule l'enveloppe porte) et celle que l'on garde dans « Éléments envoyés »
/// ou les brouillons (avec `Bcc`, pour s'en souvenir).
#[derive(Debug)]
pub struct Fabrique {
    pub envoi: Vec<u8>,
    pub copie: Vec<u8>,
    pub destinataires: Vec<String>,
    pub message_id: String,
}

/// Découpe une liste d'adresses saisie à la main : séparateurs `,` ou `;`,
/// hors guillemets et chevrons. Rend `(nom, adresse)`, ou l'entrée fautive.
pub fn adresses(liste: &str) -> Result<Vec<(String, String)>, String> {
    let mut entrees = Vec::new();
    let mut courante = String::new();
    let (mut guillemets, mut chevrons) = (false, false);
    for c in liste.chars() {
        match c {
            '"' if !chevrons => guillemets = !guillemets,
            '<' if !guillemets => chevrons = true,
            '>' if !guillemets => chevrons = false,
            ',' | ';' if !guillemets && !chevrons => {
                entrees.push(std::mem::take(&mut courante));
                continue;
            }
            _ => {}
        }
        courante.push(c);
    }
    entrees.push(courante);

    let mut sortie = Vec::new();
    for entree in entrees.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let (nom, adresse) = match (entree.rfind('<'), entree.ends_with('>')) {
            (Some(i), true) => (
                entree[..i].trim().trim_matches('"').trim().to_string(),
                entree[i + 1..entree.len() - 1].trim().to_string(),
            ),
            _ => (String::new(), entree.to_string()),
        };
        if !adresse_valide(&adresse) {
            return Err(entree.to_string());
        }
        sortie.push((nom, adresse));
    }
    Ok(sortie)
}

fn adresse_valide(adresse: &str) -> bool {
    let Some((local, domaine)) = adresse.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && domaine.contains('.')
        && !domaine.starts_with('.')
        && !domaine.ends_with('.')
        && !adresse.chars().any(|c| c.is_whitespace() || "<>,;\"()[]".contains(c))
}

/// Type MIME d'une pièce jointe, d'après son extension.
pub fn type_de_fichier(nom: &str) -> &'static str {
    let ext = nom.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "ics" => "text/calendar",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Identifiant de message neuf : `<horodatage.aléa@domaine>`, le domaine
/// étant celui de l'expéditeur.
fn nouvel_identifiant(de: &str, maintenant: i64) -> String {
    let mut hacheur = RandomState::new().build_hasher();
    hacheur.write_i64(maintenant);
    hacheur.write_usize(std::process::id() as usize);
    let domaine = de.rsplit_once('@').map(|(_, d)| d).unwrap_or("mmail.invalid");
    format!("{:x}.{:016x}@{domaine}", maintenant, hacheur.finish())
}

/// Fabrique le message. `destinataires` peut être vide pour un brouillon ;
/// l'envoi, lui, le refuse.
pub fn fabriquer(r: &Redaction, maintenant: i64, fichiers: &[Fichier]) -> Result<Fabrique, String> {
    fabriquer_avec_images(r, maintenant, fichiers, &[])
}

/// Comme `fabriquer`, avec les images que le HTML désigne par `cid:` : le HTML
/// et ses images vont ensemble dans un `multipart/related`, que les logiciels
/// de messagerie affichent dans le corps et non en pièces jointes.
pub fn fabriquer_avec_images(
    r: &Redaction,
    maintenant: i64,
    fichiers: &[Fichier],
    images: &[ImageIntegree],
) -> Result<Fabrique, String> {
    let lire = |champ: &str, valeur: &str| {
        adresses(valeur).map_err(|fautive| format!("{champ} : adresse invalide « {fautive} »"))
    };
    let a = lire("À", &r.a)?;
    let cc = lire("Cc", &r.cc)?;
    let cci = lire("Cci", &r.cci)?;
    if !adresse_valide(&r.de) {
        return Err(format!("expéditeur invalide : {}", r.de));
    }
    let message_id = nouvel_identifiant(&r.de, maintenant);

    let mut destinataires: Vec<String> = Vec::new();
    for (_, adresse) in a.iter().chain(&cc).chain(&cci) {
        if !destinataires.iter().any(|d| d.eq_ignore_ascii_case(adresse)) {
            destinataires.push(adresse.clone());
        }
    }

    let construire = |avec_cci: bool| -> Result<Vec<u8>, String> {
        let liste = |v: &[(String, String)]| {
            AdresseSortante::new_list(
                v.iter()
                    .map(|(n, a)| {
                        AdresseSortante::new_address((!n.is_empty()).then(|| n.clone()), a.clone())
                    })
                    .collect(),
            )
        };
        let de = AdresseSortante::new_address((!r.nom.is_empty()).then(|| r.nom.clone()), r.de.clone());
        let copie = avec_cci;
        let mut m = MessageBuilder::new()
            .from(de)
            .subject(r.objet.clone())
            .date(Date::new(maintenant))
            .message_id(message_id.clone())
            .header("User-Agent", Raw::new(format!("MMail/{}", env!("CARGO_PKG_VERSION"))));
        if !a.is_empty() {
            m = m.to(liste(&a));
        }
        if !cc.is_empty() {
            m = m.cc(liste(&cc));
        }
        if avec_cci && !cci.is_empty() {
            m = m.bcc(liste(&cci));
        }
        if !r.en_reponse_a.is_empty() {
            m = m.in_reply_to(r.en_reponse_a.clone());
        }
        if !r.references.is_empty() {
            m = m.references(r.references.clone());
        }
        match r.importance {
            1 => m = m.header("Importance", Raw::new("high")).header("X-Priority", Raw::new("1 (Highest)")),
            -1 => m = m.header("Importance", Raw::new("low")).header("X-Priority", Raw::new("5 (Lowest)")),
            _ => {}
        }
        if r.confirmation_lecture {
            m = m.header("Disposition-Notification-To", Raw::new(format!("<{}>", r.de)));
        }
        // Ce que l'enveloppe porte seule, et qu'on veut retrouver en reprenant
        // la copie : l'accusé de remise, l'heure d'envoi voulue.
        if copie && r.accuse_remise {
            m = m.header(ENTETE_ACCUSE, Raw::new("oui"));
        }
        if copie && r.envoi_differe > 0 {
            m = m.header(ENTETE_DIFFERE, Raw::new(r.envoi_differe.to_string()));
        }
        if !r.html.is_empty() && !images.is_empty() {
            // texte | (HTML + images), puis les pièces jointes à côté.
            let mut liees = vec![MimePart::new("text/html", r.html.as_str())];
            for i in images {
                liees.push(MimePart::new(i.type_mime.as_str(), i.contenu.as_slice()).inline().cid(i.cid.as_str()));
            }
            let alternative = MimePart::new(
                "multipart/alternative",
                vec![MimePart::new("text/plain", r.texte.as_str()), MimePart::new("multipart/related", liees)],
            );
            let corps = if fichiers.is_empty() {
                alternative
            } else {
                let mut parties = vec![alternative];
                for f in fichiers {
                    parties.push(
                        MimePart::new(type_de_fichier(&f.nom), f.contenu.as_slice()).attachment(f.nom.as_str()),
                    );
                }
                MimePart::new("multipart/mixed", parties)
            };
            m = m.body(corps);
        } else {
            m = m.text_body(r.texte.clone());
            if !r.html.is_empty() {
                m = m.html_body(r.html.clone());
            }
            for f in fichiers {
                m = m.attachment(type_de_fichier(&f.nom), f.nom.clone(), f.contenu.clone());
            }
        }
        m.write_to_vec().map_err(|e| format!("fabrication du message : {e}"))
    };
    Ok(Fabrique {
        envoi: construire(false)?,
        copie: construire(true)?,
        destinataires,
        message_id,
    })
}

/// Intègre au message les images locales que le HTML désigne (`src="file:…"`),
/// en les remplaçant par `cid:`. `lire` décide : il rend l'image d'un fichier
/// autorisé — ceux du profil —, ou rien, et l'image est alors retirée. Une
/// adresse `http(s)` reste telle quelle.
pub fn integrer_images(html: &str, lire: impl Fn(&str) -> Option<(String, Vec<u8>)>) -> (String, Vec<ImageIntegree>) {
    let mut sortie = String::with_capacity(html.len());
    let mut images: Vec<ImageIntegree> = Vec::new();
    let mut vues: Vec<(String, String)> = Vec::new();
    let graine = RandomState::new().build_hasher().finish();
    let mut reste = html;
    while let Some(i) = reste.find("src=\"") {
        let debut = i + 5;
        let Some(longueur) = reste[debut..].find('"') else { break };
        let adresse = &reste[debut..debut + longueur];
        sortie.push_str(&reste[..debut]);
        if adresse.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("file:")) {
            let deja = vues.iter().find(|(a, _)| a == adresse).map(|(_, c)| c.clone());
            let cid = deja.or_else(|| {
                let (type_mime, contenu) = lire(&adresse.replace("&amp;", "&"))?;
                let cid = format!("image{}.{:x}@mmail", images.len() + 1, graine);
                images.push(ImageIntegree { cid: cid.clone(), type_mime, contenu });
                vues.push((adresse.to_string(), cid.clone()));
                Some(cid)
            });
            if let Some(cid) = cid {
                sortie.push_str(&format!("cid:{cid}"));
            }
        } else {
            sortie.push_str(adresse);
        }
        reste = &reste[debut + longueur..];
    }
    sortie.push_str(reste);
    (sortie, images)
}

/// Images qu'un brouillon enregistré porte dans son corps (`cid:`) : de quoi
/// les remettre dans la fenêtre de rédaction quand on le reprend.
pub fn images_du_brouillon(brut: &[u8]) -> Vec<(String, String, Vec<u8>)> {
    use mail_parser::MimeHeaders;
    let Some(m) = MessageParser::default().parse(brut) else {
        return Vec::new();
    };
    m.parts
        .iter()
        .filter_map(|partie| {
            let cid = partie.content_id()?;
            let ct = partie.content_type()?;
            let type_mime = format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("")).to_ascii_lowercase();
            type_mime.starts_with("image/").then(|| {
                (cid.trim().trim_start_matches('<').trim_end_matches('>').to_string(), type_mime, partie.contents().to_vec())
            })
        })
        .collect()
}

// ------------------------------------------------ réponses et transferts

/// En-têtes propres à MMail, sur les copies seulement (brouillons, envois
/// différés) : jamais dans ce qui part chez le destinataire.
pub const ENTETE_DIFFERE: &str = "X-MMail-Envoi-Differe";
pub const ENTETE_ACCUSE: &str = "X-MMail-Accuse-Remise";
/// Poste qui a programmé un envoi différé (`boite::poste_differe`) : sur un
/// serveur sans CONDSTORE, lui seul l'envoie à l'heure dite.
pub const ENTETE_POSTE: &str = "X-MMail-Poste-Differe";

/// Ce que le noyau rend à la fenêtre de rédaction pour la pré-remplir.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preparation {
    pub mode: String,
    /// Adresse d'où écrire : celle du brouillon repris ; pour une réponse ou un
    /// transfert, celle des adresses de l'utilisateur (`propres`) à qui le
    /// message était adressé — boîte partagée, alias — ou d'où il l'avait
    /// écrit. Vide : celle du compte.
    pub de: String,
    pub a: String,
    pub cc: String,
    pub cci: String,
    pub objet: String,
    pub texte: String,
    pub html: String,
    pub en_reponse_a: String,
    pub references: Vec<String>,
    /// Chemins des pièces reprises (transfert, brouillon), remplis par le fil
    /// de travail après extraction.
    pub pieces: Vec<String>,
    /// Images du message cité (réponse, transfert) : type MIME et octets,
    /// désignées dans `html` par `mmail-image:N`. Le fil de travail les écrit
    /// en fichiers avant de rendre la préparation à la fenêtre.
    #[serde(skip)]
    pub images: Vec<(String, Vec<u8>)>,
    pub brouillon_uid: u32,
    pub brouillon_chemin: String,
    pub origine_chemin: String,
    pub origine_uid: u32,
    pub origine_mode: String,
    pub importance: i8,
    pub accuse_remise: bool,
    pub confirmation_lecture: bool,
    pub envoi_differe: i64,
}

/// Préfixe « RE : » ou « TR : », sauf si l'objet le porte déjà, sous l'une de
/// ses formes courantes.
pub fn prefixer(objet: &str, prefixe: &str) -> String {
    let objet = objet.trim();
    let bas = objet.to_lowercase();
    let connus: &[&str] = if prefixe == "RE" {
        &["re:", "re :", "aw:", "rép:", "rép :"]
    } else {
        &["tr:", "tr :", "fw:", "fw :", "fwd:", "fwd :"]
    };
    if connus.iter().any(|p| bas.starts_with(p)) {
        objet.to_string()
    } else {
        format!("{prefixe} : {objet}")
    }
}

/// Une adresse telle qu'on la réécrit dans un champ : « Nom <a@b.fr> », le
/// nom entre guillemets s'il contient un séparateur.
fn affichable(nom: &str, adresse: &str) -> String {
    if nom.is_empty() || nom.eq_ignore_ascii_case(adresse) {
        adresse.to_string()
    } else if nom.contains([',', ';', '<', '>', '"']) {
        format!("\"{}\" <{adresse}>", nom.replace('"', ""))
    } else {
        format!("{nom} <{adresse}>")
    }
}

fn paires(adresses: Option<&Address<'_>>) -> Vec<(String, String)> {
    adresses
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let adresse = x.address()?.to_string();
                    Some((x.name().unwrap_or_default().to_string(), adresse))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn joindre(v: &[(String, String)]) -> String {
    v.iter().map(|(n, a)| affichable(n, a)).collect::<Vec<_>>().join("; ")
}

const JOURS: [&str; 7] = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre",
    "octobre", "novembre", "décembre",
];

/// « mardi 29 septembre 2026 10:33 », dans le fuseau de l'en-tête d'origine.
pub fn date_longue(d: &DateTime) -> String {
    let jours = crate::protocole::jours_depuis_epoque(d.year as i64, d.month as i64, d.day as i64);
    // Le 1er janvier 1970 était un jeudi.
    let jour = JOURS[(jours + 4).rem_euclid(7) as usize];
    let mois = MOIS.get((d.month as usize).wrapping_sub(1)).copied().unwrap_or("?");
    format!("{jour} {} {mois} {} {:02}:{:02}", d.day, d.year, d.hour, d.minute)
}

/// Prépare une réponse, une réponse à tous, un transfert ou la reprise d'un
/// brouillon. `propres` : les adresses de l'utilisateur, jamais remises en
/// destinataires d'une réponse à tous.
pub fn preparer(brut: &[u8], mode: &str, propres: &[String]) -> Preparation {
    let mut p = Preparation { mode: mode.to_string(), ..Default::default() };
    let Some(m) = MessageParser::default().parse(brut) else {
        return p;
    };
    let objet = m.subject().unwrap_or("").to_string();
    let message_id = m.message_id().unwrap_or("").to_string();
    let de = paires(m.from());
    let a = paires(m.to());
    let cc = paires(m.cc());
    let texte = crate::index::corps_affichable(brut);

    if mode == "brouillon" {
        p.de = de.first().map(|(_, x)| x.clone()).unwrap_or_default();
        p.a = joindre(&a);
        p.cc = joindre(&cc);
        p.cci = joindre(&paires(m.bcc()));
        p.objet = objet;
        p.texte = m.body_text(0).map(|t| t.into_owned()).unwrap_or_default();
        // Un vrai corps HTML seulement : mail-parser en fabrique un à partir
        // du texte quand le message n'en a pas.
        if let Some(html) = m.html_part(0).filter(|part| part.is_text_html()) {
            p.html = html.text_contents().unwrap_or("").to_string();
        }
        p.en_reponse_a = m.in_reply_to().as_text().unwrap_or("").to_string();
        p.references = references(&m);
        p.importance = crate::index::importance(&m);
        p.confirmation_lecture = m.header_raw("Disposition-Notification-To").is_some();
        p.accuse_remise = m.header_raw(ENTETE_ACCUSE).is_some();
        p.envoi_differe = echeance(brut).unwrap_or(0);
        return p;
    }

    let est_propre = |adresse: &str| propres.iter().any(|x| x.eq_ignore_ascii_case(adresse));
    // D'où écrire (0.6.7) : l'adresse de l'utilisateur à qui le message était
    // adressé, ou d'où il l'avait lui-même écrit ; à défaut, celle où le
    // serveur l'a remis (alias, liste, copie cachée).
    p.de = de
        .iter()
        .chain(&a)
        .chain(&cc)
        .map(|(_, x)| x.clone())
        .chain(remis_a(&m))
        .find(|x| est_propre(x))
        .unwrap_or_default();

    let citation = {
        let mut bloc = String::from("\n\n________________________________\n");
        bloc.push_str(&format!("De : {}\n", joindre(&de)));
        if let Some(date) = m.date() {
            bloc.push_str(&format!("Envoyé : {}\n", date_longue(date)));
        }
        bloc.push_str(&format!("À : {}\n", joindre(&a)));
        if !cc.is_empty() {
            bloc.push_str(&format!("Cc : {}\n", joindre(&cc)));
        }
        bloc.push_str(&format!("Objet : {objet}\n\n"));
        bloc.push_str(&texte);
        bloc
    };
    p.texte = citation;
    // Un message d'origine en HTML est cité en HTML — tableaux, mise en forme
    // et images intégrées compris —, comme dans Outlook ; le texte reste la
    // version brute de la rédaction.
    if let Some(corps) = crate::rendu::corps_html(brut) {
        p.html = citation_html(&de, m.date(), &a, &cc, &objet, &corps.html);
        p.images = corps.images;
    }

    if mode == "transferer" {
        p.objet = prefixer(&objet, "TR");
        p.origine_mode = "transferer".into();
        return p;
    }

    p.objet = prefixer(&objet, "RE");
    // « repondre_tous » gardé jusqu'à l'envoi : le message d'origine y reçoit
    // `$ReplyAll` en plus de `\Answered`.
    p.origine_mode = if mode == "repondre_tous" { "repondre_tous" } else { "repondre" }.into();
    let cible = match paires(m.reply_to()) {
        v if !v.is_empty() => v,
        _ => de,
    };
    // Répondre à son propre message : on écrit à ses destinataires d'origine.
    let cible = if !cible.is_empty() && cible.iter().all(|(_, x)| est_propre(x)) { a.clone() } else { cible };
    p.a = joindre(&cible);
    if mode == "repondre_tous" {
        let mut autres: Vec<(String, String)> = Vec::new();
        for (n, x) in a.iter().chain(&cc) {
            let deja = cible.iter().chain(&autres).any(|(_, y)| y.eq_ignore_ascii_case(x));
            if !est_propre(x) && !deja {
                autres.push((n.clone(), x.clone()));
            }
        }
        p.cc = joindre(&autres);
    }
    if !message_id.is_empty() {
        p.en_reponse_a = message_id.clone();
        p.references = references(&m);
        if !p.references.iter().any(|r| r == &message_id) {
            p.references.push(message_id);
        }
    }
    p
}

/// Adresses de remise posées par les serveurs (`Delivered-To`, `X-Original-To`) :
/// la boîte où le message est arrivé, quand ni À ni Cc ne la nomment.
fn remis_a(m: &mail_parser::Message<'_>) -> Vec<String> {
    ["Delivered-To", "X-Original-To"]
        .iter()
        .filter_map(|&nom| m.header_raw(nom))
        .map(|v| v.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Bloc cité en HTML : deux lignes pour écrire, le trait et l'en-tête « De /
/// Envoyé / À / Cc / Objet », puis le corps assaini de l'original. Ses
/// feuilles de style sont écartées — elles régiraient toute la rédaction —,
/// et ses images distantes aussi : la fenêtre de rédaction irait les
/// chercher, et dirait à l'expéditeur que le message a été rouvert.
fn citation_html(
    de: &[(String, String)],
    date: Option<&DateTime>,
    a: &[(String, String)],
    cc: &[(String, String)],
    objet: &str,
    corps: &str,
) -> String {
    let e = crate::rendu::echapper;
    let mut html = String::from("<p><br></p><p><br></p><p>________________________________<br>");
    html.push_str(&format!("<b>De :</b> {}<br>", e(&joindre(de))));
    if let Some(date) = date {
        html.push_str(&format!("<b>Envoyé :</b> {}<br>", e(&date_longue(date))));
    }
    html.push_str(&format!("<b>À :</b> {}<br>", e(&joindre(a))));
    if !cc.is_empty() {
        html.push_str(&format!("<b>Cc :</b> {}<br>", e(&joindre(cc))));
    }
    html.push_str(&format!("<b>Objet :</b> {}</p>", e(objet)));
    html.push_str("<div>");
    html.push_str(&sans_images_distantes(sans_feuilles_de_style(corps)));
    html.push_str("</div>");
    html
}

/// Le corps assaini sans les feuilles de style que `rendu` place en tête.
fn sans_feuilles_de_style(corps: &str) -> &str {
    match corps.strip_prefix("<style>").and_then(|reste| reste.find("</style>").map(|i| &reste[i + 8..])) {
        Some(reste) => reste,
        None => corps,
    }
}

/// Retire les balises `<img>` des images distantes (repère `mmail-distante:`).
fn sans_images_distantes(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut reste = html;
    while let Some(i) = reste.find("<img") {
        sortie.push_str(&reste[..i]);
        let Some(fin) = reste[i..].find('>') else {
            reste = "";
            break;
        };
        let balise = &reste[i..i + fin + 1];
        if !balise.contains(crate::rendu::REPERE_DISTANTE) {
            sortie.push_str(balise);
        }
        reste = &reste[i + fin + 1..];
    }
    sortie.push_str(reste);
    sortie
}

// ------------------------------------------------ confirmation de lecture

/// Adresse à laquelle l'expéditeur demande une confirmation de lecture, si
/// le message en porte la demande (`Disposition-Notification-To`).
pub fn confirmation_demandee(brut: &[u8]) -> Option<String> {
    let m = MessageParser::default().parse_headers(brut)?;
    // Champ que mail-parser ne tient pas pour une adresse : lu brut, puis
    // analysé comme une saisie (« Nom <a@b.fr> »), encodage des noms compris.
    let champ = m.header_raw("Disposition-Notification-To")?.replace(['\r', '\n'], " ");
    let champ = match champ.rfind('<') {
        Some(i) => champ[i..].to_string(),
        None => champ,
    };
    adresses(champ.trim()).ok()?.into_iter().map(|(_, adresse)| adresse).next()
}

/// Confirmation de lecture (MDN, RFC 8098) pour un message reçu :
/// `multipart/report` avec une partie lisible et une partie
/// `message/disposition-notification`. Rend le destinataire et le message.
pub fn confirmation_lecture(
    brut: &[u8],
    de: &str,
    nom: &str,
    maintenant: i64,
) -> Option<(String, Vec<u8>)> {
    let destinataire = confirmation_demandee(brut)?;
    let m = MessageParser::default().parse(brut)?;
    let objet = m.subject().unwrap_or("").to_string();
    let envoye = m.date().map(date_longue).unwrap_or_default();
    let lisible = format!(
        "Votre message\n\n  À : {de}\n  Objet : {objet}\n  Envoyé : {envoye}\n\na été lu le {}.\n",
        date_longue(&DateTime::from_timestamp(maintenant))
    );
    let mut rapport = format!(
        "Reporting-UA: MMail/{}\r\nFinal-Recipient: rfc822;{de}\r\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(id) = m.message_id() {
        rapport.push_str(&format!("Original-Message-ID: <{id}>\r\n"));
    }
    rapport.push_str("Disposition: manual-action/MDN-sent-manually; displayed\r\n");
    let corps = MimePart::new(
        ContentType::new("multipart/report").attribute("report-type", "disposition-notification"),
        vec![
            MimePart::new("text/plain", lisible),
            // `message/*` : 7 bits seulement (RFC 2046 § 5.2) ; sans cela,
            // mail-builder passe en quoted-printable et coupe les lignes longues.
            MimePart::new("message/disposition-notification", rapport).transfer_encoding("7bit"),
        ],
    );
    let expediteur = AdresseSortante::new_address((!nom.is_empty()).then(|| nom.to_string()), de.to_string());
    let octets = MessageBuilder::new()
        .from(expediteur)
        .to(destinataire.as_str())
        .subject(format!("Lu : {objet}"))
        .date(Date::new(maintenant))
        .message_id(nouvel_identifiant(de, maintenant))
        .body(corps)
        .write_to_vec()
        .ok()?;
    Some((destinataire, octets))
}

// -------------------------------------------------------- envoi différé

/// Heure d'envoi voulue d'une copie différée (en-tête `X-MMail-Envoi-Differe`).
pub fn echeance(brut: &[u8]) -> Option<i64> {
    let m = MessageParser::default().parse_headers(brut)?;
    m.header_raw(ENTETE_DIFFERE).and_then(|t| t.trim().parse().ok())
}

/// Ce qu'il faut pour envoyer une copie différée le moment venu.
#[derive(Debug)]
pub struct EnvoiDiffere {
    pub de: String,
    pub destinataires: Vec<String>,
    pub accuse: bool,
    /// Ce qui part : sans `Bcc` ni en-têtes de MMail, daté de l'envoi.
    pub envoi: Vec<u8>,
    /// Ce qui va dans « Éléments envoyés » : avec `Bcc`, daté de l'envoi.
    pub copie: Vec<u8>,
}

/// Prépare l'envoi d'une copie différée : les destinataires sont relus dans
/// ses en-têtes (`Bcc` compris), la date devient celle de l'envoi réel.
pub fn preparer_envoi_differe(brut: &[u8], maintenant: i64) -> Result<EnvoiDiffere, String> {
    let m = MessageParser::default().parse(brut).ok_or("message illisible")?;
    let de = paires(m.from()).into_iter().next().map(|(_, a)| a).ok_or("message sans expéditeur")?;
    let mut destinataires: Vec<String> = Vec::new();
    for (_, adresse) in paires(m.to()).into_iter().chain(paires(m.cc())).chain(paires(m.bcc())) {
        if !destinataires.iter().any(|d| d.eq_ignore_ascii_case(&adresse)) {
            destinataires.push(adresse);
        }
    }
    if destinataires.is_empty() {
        return Err("message sans destinataire".into());
    }
    let accuse = m.header_raw(ENTETE_ACCUSE).is_some();
    let date = format!("Date: {}", Date::new(maintenant).to_rfc822());
    let propres = [ENTETE_DIFFERE, ENTETE_ACCUSE, ENTETE_POSTE, "Date"];
    Ok(EnvoiDiffere {
        de,
        destinataires,
        accuse,
        envoi: reecrire_entetes(brut, &[&propres[..], &["Bcc"]].concat(), &date),
        copie: reecrire_entetes(brut, &propres, &date),
    })
}

/// Retire des en-têtes (lignes de suite comprises) et en ajoute un en tête,
/// sans toucher au corps.
fn reecrire_entetes(brut: &[u8], retirer: &[&str], ajout: &str) -> Vec<u8> {
    let fin = brut.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 2).unwrap_or(brut.len());
    let (tete, corps) = brut.split_at(fin);
    let tete = String::from_utf8_lossy(tete);
    let mut sortie = format!("{ajout}\r\n");
    let mut garder = true;
    for ligne in tete.split_inclusive("\r\n") {
        let suite = ligne.starts_with(' ') || ligne.starts_with('\t');
        if !suite {
            let nom = ligne.split(':').next().unwrap_or("").trim();
            garder = !retirer.iter().any(|r| r.eq_ignore_ascii_case(nom));
        }
        if garder {
            sortie.push_str(ligne);
        }
    }
    let mut octets = sortie.into_bytes();
    octets.extend_from_slice(corps);
    octets
}

fn references(m: &mail_parser::Message<'_>) -> Vec<String> {
    let valeur = m.references();
    if let Some(liste) = valeur.as_text_list() {
        liste.iter().map(|r| r.to_string()).collect()
    } else {
        valeur.as_text().map(|r| vec![r.to_string()]).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MimeHeaders;

    const ORIGINAL: &str = "From: \"Martin, Hélène\" <helene@exemple.fr>\r\n\
        To: moi@exemple.fr, Noël <noel@exemple.fr>\r\n\
        Cc: compta@exemple.fr\r\n\
        Subject: =?UTF-8?Q?Devis_toiture?=\r\n\
        Date: Tue, 29 Sep 2026 10:33:00 +0200\r\n\
        Message-ID: <origine.1@exemple.fr>\r\n\
        References: <avant.0@exemple.fr>\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\r\n\
        Bonjour,\r\nVoici le devis.\r\n";

    #[test]
    fn listes_d_adresses() {
        assert_eq!(
            adresses("a@b.fr; Noël Durand <noel@exemple.fr>, \"Martin, Hélène\" <h@x.fr>").unwrap(),
            vec![
                ("".into(), "a@b.fr".into()),
                ("Noël Durand".into(), "noel@exemple.fr".into()),
                ("Martin, Hélène".into(), "h@x.fr".into()),
            ]
        );
        assert_eq!(adresses(" ; , ").unwrap(), vec![]);
        assert_eq!(adresses("a@b.fr, pas une adresse").unwrap_err(), "pas une adresse");
        assert!(adresses("a@localhost").is_err());
        assert!(adresses("Nom <a b@c.fr>").is_err());
    }

    #[test]
    fn objets_prefixes_une_seule_fois() {
        assert_eq!(prefixer("Devis", "RE"), "RE : Devis");
        assert_eq!(prefixer("RE : Devis", "RE"), "RE : Devis");
        assert_eq!(prefixer("Re: Devis", "RE"), "Re: Devis");
        assert_eq!(prefixer("Devis", "TR"), "TR : Devis");
        assert_eq!(prefixer("Fwd: Devis", "TR"), "Fwd: Devis");
        assert_eq!(prefixer("RE : Devis", "TR"), "TR : RE : Devis");
    }

    #[test]
    fn reponse_simple_et_a_tous() {
        let propres = vec!["moi@exemple.fr".to_string()];
        let r = preparer(ORIGINAL.as_bytes(), "repondre", &propres);
        assert_eq!(r.a, "\"Martin, Hélène\" <helene@exemple.fr>");
        assert_eq!(r.cc, "");
        assert_eq!(r.objet, "RE : Devis toiture");
        assert_eq!(r.en_reponse_a, "origine.1@exemple.fr");
        assert_eq!(r.references, vec!["avant.0@exemple.fr", "origine.1@exemple.fr"]);
        assert!(r.texte.starts_with("\n\n________________________________\nDe : "));
        assert!(r.texte.contains("Envoyé : mardi 29 septembre 2026 10:33\n"));
        assert!(r.texte.contains("Objet : Devis toiture\n\nBonjour,"));
        assert_eq!(r.origine_mode, "repondre");

        let t = preparer(ORIGINAL.as_bytes(), "repondre_tous", &propres);
        assert_eq!(t.cc, "Noël <noel@exemple.fr>; compta@exemple.fr", "sans soi-même");
        assert_eq!(t.origine_mode, "repondre_tous");
        assert_eq!(t.de, "moi@exemple.fr");
    }

    /// Autres adresses d'envoi (0.6.7) : on répond de celle à qui le message
    /// était adressé, et elle ne revient pas en copie d'une réponse à tous.
    #[test]
    fn reponse_depuis_l_adresse_destinataire() {
        let propres = vec!["moi@exemple.fr".to_string(), "Compta@Exemple.fr".to_string()];
        let original = ORIGINAL.replace("To: moi@exemple.fr, Noël", "To: Noël");
        let r = preparer(original.as_bytes(), "repondre", &propres);
        assert_eq!(r.de, "compta@exemple.fr", "la boîte partagée en copie de l'original");
        let t = preparer(original.as_bytes(), "repondre_tous", &propres);
        assert_eq!(t.de, "compta@exemple.fr");
        assert_eq!(t.cc, "Noël <noel@exemple.fr>", "sans l'adresse d'où l'on répond");
        let f = preparer(original.as_bytes(), "transferer", &propres);
        assert_eq!(f.de, "compta@exemple.fr");
        // Adressé à soi et à la boîte partagée : la première nommée.
        assert_eq!(preparer(ORIGINAL.as_bytes(), "repondre", &propres).de, "moi@exemple.fr");
        // Personne de connu : l'adresse du compte, choisie par l'interface.
        assert_eq!(preparer(original.as_bytes(), "repondre", &[]).de, "");
        // Ni À ni Cc : l'en-tête de remise du serveur.
        let liste = "Delivered-To: <compta@exemple.fr>\r\nFrom: liste@exemple.org\r\nTo: abonnes@exemple.org\r\n\
            Subject: Lettre\r\nMessage-ID: <l.1@exemple.org>\r\n\r\nTexte\r\n";
        assert_eq!(preparer(liste.as_bytes(), "repondre", &propres).de, "compta@exemple.fr");
        // Son propre message, écrit depuis la boîte partagée : on en repart.
        let envoye = "From: compta@exemple.fr\r\nTo: helene@exemple.fr\r\nSubject: Relance\r\n\
            Message-ID: <e.1@exemple.fr>\r\n\r\nTexte\r\n";
        let r = preparer(envoye.as_bytes(), "repondre", &propres);
        assert_eq!(r.de, "compta@exemple.fr");
        assert_eq!(r.a, "helene@exemple.fr");
    }

    #[test]
    fn reponse_a_un_message_html() {
        let original = "From: \"Martin, Hélène\" <helene@exemple.fr>\r\n\
            To: moi@exemple.fr\r\n\
            Subject: Devis <toiture>\r\n\
            Date: Tue, 29 Sep 2026 10:33:00 +0200\r\n\
            Message-ID: <origine.2@exemple.fr>\r\n\
            MIME-Version: 1.0\r\n\
            Content-Type: multipart/related; boundary=\"b\"\r\n\r\n\
            --b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
            <html><head><style>p { font-size: 6pt }</style></head><body>\
            <table border=\"1\"><tr><td>Lot</td><td>Prix</td></tr><tr><td>Toiture</td><td>12 000 €</td></tr></table>\
            <img src=\"cid:logo@x\"><img src=\"https://suivi.exemple.com/pixel.gif\" width=\"1\">\
            </body></html>\r\n\
            --b\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\n\
            iVBORw0KGgo=\r\n--b--\r\n";
        let r = preparer(original.as_bytes(), "repondre", &["moi@exemple.fr".to_string()]);
        assert!(r.html.starts_with("<p><br></p><p><br></p><p>________________________________<br><b>De :</b> "), "{}", r.html);
        assert!(r.html.contains("&quot;Martin, Hélène&quot;") || r.html.contains("\"Martin, Hélène\""), "{}", r.html);
        assert!(r.html.contains("<b>Objet :</b> Devis &lt;toiture&gt;</p>"), "objet échappé : {}", r.html);
        assert!(r.html.contains("<td>Toiture</td>"), "le tableau est cité : {}", r.html);
        assert!(r.html.contains("\"mmail-image:0\""), "l'image intégrée est gardée : {}", r.html);
        assert_eq!(r.images.len(), 1);
        assert_eq!(r.images[0].0, "image/png");
        assert!(!r.html.contains("mmail-distante"), "image distante retirée : {}", r.html);
        assert!(!r.html.contains("suivi.exemple.com"));
        assert!(!r.html.contains("<style>"), "feuilles de style de l'original écartées : {}", r.html);
        assert!(r.texte.contains("Objet : Devis <toiture>"), "la version texte reste");

        let t = preparer(original.as_bytes(), "transferer", &[]);
        assert!(t.html.contains("<td>Toiture</td>"));
        assert_eq!(t.images.len(), 1);

        let simple = preparer(ORIGINAL.as_bytes(), "repondre", &[]);
        assert_eq!(simple.html, "", "un original en texte brut est cité en texte");
    }

    #[test]
    fn transfert() {
        let t = preparer(ORIGINAL.as_bytes(), "transferer", &[]);
        assert_eq!(t.a, "");
        assert_eq!(t.objet, "TR : Devis toiture");
        assert_eq!(t.en_reponse_a, "");
        assert_eq!(t.origine_mode, "transferer");
    }

    #[test]
    fn fabrication_relue_par_mail_parser() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            nom: "Émilie Bernard".into(),
            a: "Noël <noel@exemple.fr>".into(),
            cc: "compta@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Réunion d'équipe — été".into(),
            texte: "Bonjour,\n.ligne commençant par un point\nÀ bientôt".into(),
            en_reponse_a: "origine.1@exemple.fr".into(),
            references: vec!["origine.1@exemple.fr".into()],
            ..Default::default()
        };
        let pj = [Fichier { nom: "Relevé n°12.pdf".into(), contenu: b"%PDF-1.4 essai".to_vec() }];
        let f = fabriquer(&r, 1_790_000_000, &pj).unwrap();
        assert_eq!(f.destinataires, vec!["noel@exemple.fr", "compta@exemple.fr", "archive@exemple.fr"]);

        let envoi = MessageParser::default().parse(&f.envoi).unwrap();
        assert_eq!(envoi.subject(), Some("Réunion d'équipe — été"));
        assert_eq!(envoi.from().unwrap().first().unwrap().name(), Some("Émilie Bernard"));
        assert!(envoi.bcc().is_none(), "le Cci ne part pas dans le message");
        assert_eq!(envoi.in_reply_to().as_text(), Some("origine.1@exemple.fr"));
        assert!(envoi.body_text(0).unwrap().contains(".ligne commençant par un point"));
        let piece = envoi.attachments().next().unwrap();
        assert_eq!(piece.attachment_name(), Some("Relevé n°12.pdf"));
        assert_eq!(piece.contents(), b"%PDF-1.4 essai");

        let copie = MessageParser::default().parse(&f.copie).unwrap();
        assert_eq!(copie.bcc().unwrap().first().unwrap().address(), Some("archive@exemple.fr"));
        assert_eq!(copie.message_id(), envoi.message_id());
    }

    #[test]
    fn fabrication_refusee() {
        let r = Redaction { de: "moi@exemple.fr".into(), a: "pas-une-adresse".into(), ..Default::default() };
        assert!(fabriquer(&r, 0, &[]).unwrap_err().contains("pas-une-adresse"));
        let r = Redaction { de: "".into(), ..Default::default() };
        assert!(fabriquer(&r, 0, &[]).is_err());
    }

    #[test]
    fn reprise_d_un_brouillon() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            a: "noel@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Brouillon".into(),
            texte: "Texte en cours".into(),
            ..Default::default()
        };
        let f = fabriquer(&r, 1_790_000_000, &[]).unwrap();
        let b = preparer(&f.copie, "brouillon", &[]);
        assert_eq!(b.html, "", "brouillon en texte brut : pas de HTML inventé");
        assert_eq!(b.de, "moi@exemple.fr", "repris depuis l'adresse où il a été écrit");
        assert_eq!(b.a, "noel@exemple.fr");
        assert_eq!(b.cci, "archive@exemple.fr");
        assert_eq!(b.objet, "Brouillon");
        assert_eq!(b.texte.trim_end(), "Texte en cours");
    }

    #[test]
    fn reprise_d_un_brouillon_html() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            objet: "Mis en forme".into(),
            texte: "Texte gras".into(),
            html: "<p>Texte <b>gras</b></p>".into(),
            ..Default::default()
        };
        let f = fabriquer(&r, 1_790_000_000, &[]).unwrap();
        let b = preparer(&f.copie, "brouillon", &[]);
        assert!(b.html.contains("<b>gras</b>"));
        assert_eq!(b.texte.trim_end(), "Texte gras");
    }

    #[test]
    fn signature_a_images_integrees() {
        let (html, images) = integrer_images(
            "<p>Bien à vous</p><img src=\"file:///p/signatures/logo.png\" width=\"344\" />\
             <img src=\"file:///p/signatures/logo.png\" /><img src=\"file:///etc/passwd\" />\
             <img src=\"https://exemple.fr/x.png\" />",
            |adresse| adresse.contains("/p/signatures/").then(|| ("image/png".to_string(), b"\x89PNG".to_vec())),
        );
        assert_eq!(images.len(), 1);
        let cid = &images[0].cid;
        assert_eq!(html.matches(&format!("src=\"cid:{cid}\"")).count(), 2, "{html}");
        assert!(html.contains("src=\"\""), "{html}");
        assert!(!html.contains("passwd"), "{html}");
        assert!(html.contains("src=\"https://exemple.fr/x.png\""), "{html}");

        let r = Redaction {
            de: "moi@exemple.fr".into(),
            a: "toi@exemple.fr".into(),
            objet: "Signature".into(),
            texte: "Bien à vous".into(),
            html: html.clone(),
            ..Default::default()
        };
        let pj = [Fichier { nom: "note.txt".into(), contenu: b"note".to_vec() }];
        let f = fabriquer_avec_images(&r, 1_790_000_000, &pj, &images).unwrap();
        let brut = String::from_utf8_lossy(&f.envoi).to_string();
        assert!(brut.contains("multipart/mixed") && brut.contains("multipart/related"), "{brut}");
        let m = MessageParser::default().parse(&f.envoi).unwrap();
        // Une seule pièce jointe : le logo est dans le corps, pas à côté.
        assert_eq!(crate::index::pieces_jointes(&f.envoi).len(), 1);
        assert!(m.body_html(0).unwrap().contains(&format!("cid:{cid}")));
        // Repris comme brouillon, le logo se retrouve par son Content-ID.
        let reprises = images_du_brouillon(&f.copie);
        assert_eq!(reprises.len(), 1);
        assert_eq!(&reprises[0].0, cid);
        assert_eq!(reprises[0].2, b"\x89PNG");
    }

    #[test]
    fn options_d_envoi() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            a: "noel@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Urgent".into(),
            texte: "x".into(),
            importance: 1,
            accuse_remise: true,
            confirmation_lecture: true,
            envoi_differe: 1_790_100_000,
            ..Default::default()
        };
        let f = fabriquer(&r, 1_790_100_000, &[]).unwrap();
        let envoi = String::from_utf8_lossy(&f.envoi).to_string();
        assert!(envoi.contains("Importance: high") && envoi.contains("X-Priority: 1"));
        assert!(envoi.contains("Disposition-Notification-To: <moi@exemple.fr>"));
        assert!(!envoi.contains("X-MMail"), "rien de propre à MMail ne part");
        // La copie garde de quoi reprendre le message tel qu'il a été réglé.
        let b = preparer(&f.copie, "brouillon", &[]);
        assert_eq!((b.importance, b.accuse_remise, b.confirmation_lecture, b.envoi_differe), (1, true, true, 1_790_100_000));
        assert_eq!(echeance(&f.copie), Some(1_790_100_000));
        assert_eq!(echeance(&f.envoi), None);
    }

    #[test]
    fn envoi_differe_le_moment_venu() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            a: "noel@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Plus tard".into(),
            texte: "Corps\nsur deux lignes".into(),
            accuse_remise: true,
            envoi_differe: 1_790_100_000,
            ..Default::default()
        };
        let f = fabriquer(&r, 1_790_000_000, &[]).unwrap();
        let e = preparer_envoi_differe(&f.copie, 1_790_100_060).unwrap();
        assert_eq!(e.de, "moi@exemple.fr");
        assert_eq!(e.destinataires, vec!["noel@exemple.fr", "archive@exemple.fr"]);
        assert!(e.accuse);
        let envoi = MessageParser::default().parse(&e.envoi).unwrap();
        assert!(envoi.bcc().is_none());
        assert_eq!(envoi.date().unwrap().to_timestamp(), 1_790_100_060, "daté de l'envoi réel");
        assert!(envoi.body_text(0).unwrap().contains("sur deux lignes"));
        let brut = String::from_utf8_lossy(&e.envoi).to_string();
        assert!(!brut.contains("X-MMail") && brut.matches("Date:").count() == 1);
        let copie = MessageParser::default().parse(&e.copie).unwrap();
        assert_eq!(copie.bcc().unwrap().first().unwrap().address(), Some("archive@exemple.fr"));
    }

    #[test]
    fn confirmation_de_lecture() {
        let recu = "From: Hélène <helene@exemple.fr>\r\nTo: moi@exemple.fr\r\nSubject: Devis\r\n\
                    Date: Tue, 29 Sep 2026 10:33:00 +0200\r\nMessage-ID: <devis.1@exemple.fr>\r\n\
                    Disposition-Notification-To: Hélène <helene@exemple.fr>\r\n\r\nCorps\r\n";
        assert_eq!(confirmation_demandee(recu.as_bytes()).as_deref(), Some("helene@exemple.fr"));
        assert_eq!(confirmation_demandee(ORIGINAL.as_bytes()), None);
        let (a, octets) = confirmation_lecture(recu.as_bytes(), "moi@exemple.fr", "", 1_790_000_000).unwrap();
        assert_eq!(a, "helene@exemple.fr");
        let texte = String::from_utf8_lossy(&octets).to_string();
        assert!(texte.contains("multipart/report"));
        assert!(texte.contains("report-type=") && texte.contains("disposition-notification"));
        assert!(texte.contains("Original-Message-ID: <devis.1@exemple.fr>"));
        assert!(texte.contains("Disposition: manual-action/MDN-sent-manually; displayed"));
        let m = MessageParser::default().parse(&octets).unwrap();
        assert_eq!(m.subject(), Some("Lu : Devis"));
        // Un identifiant long reste d'un seul tenant.
        let long = recu.replace("devis.1@exemple.fr", "179069362582.4099638.5162412060862382223@lab.exemple.fr");
        let (_, octets) = confirmation_lecture(long.as_bytes(), "moi@exemple.fr", "", 1_790_000_000).unwrap();
        let texte = String::from_utf8_lossy(&octets).to_string();
        assert!(texte.contains("Original-Message-ID: <179069362582.4099638.5162412060862382223@lab.exemple.fr>"));
        // La partie lisible, accentuée, peut être en quoted-printable ; le
        // rapport, jamais.
        let rapport = &texte[texte.find("message/disposition-notification").unwrap()..];
        assert!(rapport[..rapport.find("\r\n\r\n").unwrap()].contains("Content-Transfer-Encoding: 7bit"));
    }

    #[test]
    fn dates_en_francais() {
        let d = DateTime { year: 2026, month: 9, day: 29, hour: 8, minute: 5, second: 0, tz_before_gmt: false, tz_hour: 2, tz_minute: 0 };
        assert_eq!(date_longue(&d), "mardi 29 septembre 2026 08:05");
        let d = DateTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, tz_before_gmt: false, tz_hour: 0, tz_minute: 0 };
        assert!(date_longue(&d).starts_with("jeudi 1 janvier 1970"));
    }
}
