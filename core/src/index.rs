// SPDX-License-Identifier: GPL-3.0-or-later
//! Passage des en-têtes bruts du serveur à une ligne d'index affichable.
//!
//! C'est ici que l'on décode ce que le protocole transporte encodé : un sujet
//! en MIME (« =?UTF-8?Q?R=C3=A9union?= »), une adresse avec nom d'affichage,
//! une date au format courrier. L'analyse est confiée à `mail-parser` : c'est
//! la surface d'attaque du client, et elle n'a pas à être écrite à la main.

use mail_parser::{Address, Message, MessageParser, MimeHeaders};

use crate::magasin::MessageLocal;
use crate::protocole::Entete;

/// Convertit les en-têtes bruts d'un message en ligne d'index.
pub fn ligne_index(entete: &Entete) -> MessageLocal {
    let mut ligne = MessageLocal {
        uid: entete.uid,
        taille: entete.taille,
        lu: entete.lu(),
        repondu: entete.repondu(),
        transfere: entete.transfere(),
        repondu_tous: entete.repondu_tous(),
        mots_cles: entete.mots_cles(),
        ..Default::default()
    };

    let Some(message) = MessageParser::default().parse(&entete.brut) else {
        // En-têtes illisibles : la ligne reste, avec ce qu'on sait d'elle. Un
        // message incompréhensible ne doit pas disparaître de la liste.
        ligne.sujet = "(message illisible)".to_string();
        ligne.horodatage = horodatage(entete, None);
        ligne.pieces = pieces_annoncees(None, &entete.drapeaux);
        ligne.suivi = entete.suivi();
        return ligne;
    };
    ligne.pieces = pieces_annoncees(Some(&message), &entete.drapeaux);
    ligne.suivi = entete.suivi();
    ligne.importance = importance(&message);

    ligne.sujet = message.subject().unwrap_or("(sans objet)").to_string();
    ligne.message_id = message.message_id().unwrap_or_default().to_string();
    if let Some(from) = message.from() {
        let (nom, adresse) = premier_contact(from);
        ligne.adresse = adresse;
        // À défaut de nom d'affichage, l'adresse fait l'affaire.
        ligne.expediteur = if nom.is_empty() { ligne.adresse.clone() } else { nom };
    }
    ligne.destinataires = destinataires(&message);
    // La date interne du serveur sert de repli : un message peut arriver sans
    // en-tête Date, ou avec une date fantaisiste.
    ligne.date = match message.date() {
        Some(date) => date.to_rfc3339(),
        None => entete.date_interne.clone(),
    };
    ligne.horodatage = horodatage(entete, message.date().map(|d| d.to_timestamp()));
    ligne
}

/// Pièces jointes probables, d'après les seuls en-têtes : c'est tout ce que la
/// liste peut savoir sans télécharger le message. Le mot-clé `$HasAttachment`
/// (ou `$HasNoAttachment`), quand le serveur le pose, fait foi ; à défaut, un
/// message `multipart/mixed`, ou dont le corps entier n'est pas du texte, est
/// tenu pour en porter — la règle de Thunderbird. La lecture du message corrige
/// (cf. `Magasin::poser_pieces`).
fn pieces_annoncees(message: Option<&Message<'_>>, drapeaux: &[String]) -> bool {
    let a = |mot: &str| drapeaux.iter().any(|d| d.eq_ignore_ascii_case(mot));
    if a("$HasNoAttachment") {
        return false;
    }
    if a("$HasAttachment") {
        return true;
    }
    let Some(ct) = message.and_then(|m| m.content_type()) else {
        return false;
    };
    let sous = ct.subtype().unwrap_or("").to_ascii_lowercase();
    match ct.ctype().to_ascii_lowercase().as_str() {
        "multipart" => sous == "mixed",
        "text" => false,
        _ => true,
    }
}

/// Importance annoncée par l'expéditeur : 1 haute, -1 basse, 0 normale.
/// `Importance` (Outlook), à défaut `X-Priority` (1-2 haute, 4-5 basse), à
/// défaut `Priority` (RFC 2156).
pub fn importance(message: &Message<'_>) -> i8 {
    let champ = |nom: &str| message.header_raw(nom).map(|t| t.trim().to_ascii_lowercase());
    if let Some(v) = champ("Importance") {
        return match v.as_str() {
            "high" => 1,
            "low" => -1,
            _ => 0,
        };
    }
    if let Some(v) = champ("X-Priority") {
        return match v.chars().next() {
            Some('1') | Some('2') => 1,
            Some('4') | Some('5') => -1,
            _ => 0,
        };
    }
    match champ("Priority").as_deref() {
        Some("urgent") => 1,
        Some("non-urgent") => -1,
        _ => 0,
    }
}

/// Clef de tri : la date de réception du serveur, à défaut celle de l'en-tête.
fn horodatage(entete: &Entete, date_entete: Option<i64>) -> i64 {
    crate::protocole::secondes_date_interne(&entete.date_interne)
        .or(date_entete)
        .unwrap_or(0)
}

/// Corps lisible d'un message entier : le texte, ou le HTML réduit au texte.
///
/// Un message sans partie texte rend une ligne qui le dit, plutôt que rien :
/// l'utilisateur doit pouvoir distinguer « vide » de « pas encore chargé ».
/// L'affichage du message brut (décision 6) reste accessible à côté.
pub fn corps_affichable(brut: &[u8]) -> String {
    let Some(message) = MessageParser::default().parse(brut) else {
        return "(message illisible)".to_string();
    };
    // Un message tout en HTML : mail-parser le présente comme corps texte,
    // mais sa conversion ne revient à la ligne qu'après <br> et </p> — les
    // blocs <div>, les tableaux et les listes d'Outlook et des lettres
    // d'information ressortaient en longues lignes collées.
    if let Some(partie) = message.text_part(0) {
        if partie.is_text_html() {
            if let Some(html) = partie.text_contents() {
                return html_vers_texte(html);
            }
        }
    }
    if let Some(texte) = message.body_text(0) {
        return texte.into_owned();
    }
    if let Some(html) = message.body_html(0) {
        return html_vers_texte(&html);
    }
    "(ce message n'a pas de partie texte)".to_string()
}

/// Une pièce jointe, telle que l'interface la présente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceJointe {
    /// Rang parmi les pièces jointes du message : c'est ce qui la désigne
    /// quand on la demande ensuite.
    pub indice: usize,
    /// Nom de fichier sûr : sans chemin ni caractère interdit.
    pub nom: String,
    pub type_mime: String,
    pub taille: usize,
    /// Vrai si l'ouvrir directement lancerait un programme (décision de
    /// prudence, comme Outlook) : l'interface ne propose alors que
    /// l'enregistrement.
    pub risquee: bool,
}

/// Pièces jointes d'un message entier, dans l'ordre du message. Une image
/// que le corps HTML affiche (`cid:`) — un logo de signature — n'en est pas
/// une, comme dans Outlook ; elle garde son rang, qui désigne les suivantes.
pub fn pieces_jointes(brut: &[u8]) -> Vec<PieceJointe> {
    let Some(message) = MessageParser::default().parse(brut) else {
        return Vec::new();
    };
    let affichees = cid_affiches(&message);
    message
        .attachments()
        .enumerate()
        .filter(|(_, partie)| {
            partie.content_id().map(|c| !affichees.contains(&crate::rendu::normaliser_cid(c))).unwrap_or(true)
        })
        .map(|(indice, partie)| {
            let type_mime = type_de(partie);
            let nom = nom_sur(partie.attachment_name().unwrap_or(""), indice, &type_mime);
            PieceJointe {
                indice,
                risquee: ouverture_risquee(&nom),
                nom,
                type_mime,
                taille: partie.contents().len(),
            }
        })
        .collect()
}

/// Content-ID des parties que les corps HTML du message affichent.
fn cid_affiches(message: &Message<'_>) -> std::collections::HashSet<String> {
    let mut cids = std::collections::HashSet::new();
    for partie in message.html_bodies().filter(|p| p.is_text_html()) {
        let Some(html) = partie.text_contents() else { continue };
        let minuscule = html.to_ascii_lowercase();
        let mut reste = minuscule.as_str();
        while let Some(i) = reste.find("cid:") {
            let apres = &reste[i + 4..];
            let fin = apres.find(|c: char| c == '"' || c == '\'' || c == ')' || c == '>' || c.is_whitespace()).unwrap_or(apres.len());
            cids.insert(crate::rendu::normaliser_cid(&apres[..fin]));
            reste = &apres[fin..];
        }
    }
    cids
}

/// Nom sûr et contenu d'une pièce jointe désignée par son rang.
pub fn extraire_piece(brut: &[u8], indice: usize) -> Option<(String, Vec<u8>)> {
    let message = MessageParser::default().parse(brut)?;
    let partie = message.attachments().nth(indice)?;
    let nom = nom_sur(partie.attachment_name().unwrap_or(""), indice, &type_de(partie));
    Some((nom, partie.contents().to_vec()))
}

fn type_de(partie: &mail_parser::MessagePart<'_>) -> String {
    match partie.content_type() {
        Some(ct) => match ct.subtype() {
            Some(sous) => format!("{}/{}", ct.ctype(), sous).to_ascii_lowercase(),
            None => ct.ctype().to_ascii_lowercase(),
        },
        None if partie.is_message() => "message/rfc822".to_string(),
        None => "application/octet-stream".to_string(),
    }
}

/// Nom de fichier sûr pour une pièce jointe : le nom annoncé par l'expéditeur
/// est une donnée non fiable. On n'en garde que le dernier segment — jamais un
/// chemin, qui pourrait faire écrire hors du dossier choisi —, sans caractère
/// de contrôle ni caractère interdit sous Windows, et borné en longueur. Un
/// nom absent ou vide devient « piece-jointe-N » avec l'extension du type.
pub fn nom_sur(nom: &str, indice: usize, type_mime: &str) -> String {
    let dernier = nom.rsplit(['/', '\\']).next().unwrap_or("");
    let propre: String = dernier
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let propre = propre.trim().trim_matches('.').trim().to_string();
    // Noms réservés de Windows : « CON », « NUL.txt »… y désignent un
    // périphérique, pas un fichier.
    let racine = propre.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserve = matches!(racine.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (racine.len() == 4
            && (racine.starts_with("COM") || racine.starts_with("LPT"))
            && racine.as_bytes()[3].is_ascii_digit());
    let propre = if propre.is_empty() || reserve {
        format!("piece-jointe-{}{}", indice + 1, extension_de(type_mime))
    } else {
        propre
    };
    // 150 caractères au plus, en gardant l'extension.
    if propre.chars().count() <= 150 {
        return propre;
    }
    let extension = propre.rfind('.').map(|i| propre[i..].to_string()).unwrap_or_default();
    let extension: String = extension.chars().take(16).collect();
    let tronque: String = propre.chars().take(150 - extension.chars().count()).collect();
    format!("{tronque}{extension}")
}

fn extension_de(type_mime: &str) -> &'static str {
    match type_mime {
        "application/pdf" => ".pdf",
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "text/plain" => ".txt",
        "text/html" => ".html",
        "text/calendar" => ".ics",
        "message/rfc822" => ".eml",
        _ => ".bin",
    }
}

/// Vrai si ouvrir ce fichier d'un double clic lancerait un programme ou un
/// script. Ces pièces jointes ne s'ouvrent pas depuis MMail : on les
/// enregistre, et l'on décide ensuite en connaissance de cause.
pub fn ouverture_risquee(nom: &str) -> bool {
    const EXECUTABLES: &[&str] = &[
        "exe", "com", "bat", "cmd", "scr", "pif", "msi", "msp", "mst", "jar", "js", "jse",
        "vbs", "vbe", "wsf", "wsh", "wsc", "ps1", "psm1", "psd1", "hta", "cpl", "lnk", "url",
        "reg", "inf", "dll", "sys", "msc", "application", "appref-ms", "gadget", "appx",
        "appxbundle", "msix", "msixbundle", "iso", "img", "vhd", "vhdx", "scf", "sct",
        "settingcontent-ms", "sh", "run", "desktop", "appimage", "apk", "py", "pl",
        "command", "app", "pkg", "dmg",
        // Aide compilée, compléments Office qui exécutent du code dès
        // l'ouverture, connexions de données, bases Access (0.6.4).
        "chm", "hlp", "xll", "xla", "xlam", "ppam", "iqy", "slk", "mde", "mdb", "accde", "ade",
        "adp", "library-ms", "searchconnector-ms", "jnlp", "xbap", "msu", "ocx", "ws", "wiz",
        "diagcab", "mht", "mhtml",
    ];
    let Some((_, extension)) = nom.rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    EXECUTABLES.contains(&extension.as_str())
}

/// Réduction d'un HTML à son texte, en gardant sa structure de lignes : ce
/// n'est pas un rendu — l'affichage HTML viendra plus tard —, mais de quoi lire
/// un message mis en page. Le contenu invisible (en-tête, styles, scripts,
/// commentaires) est écarté, les blocs et les lignes de tableau passent à la
/// ligne, les éléments de liste sont marqués, les entités sont décodées.
///
/// Un bloc (`<div>`, `<p>`…) impose d'être en début de ligne, sans ajouter de
/// ligne vide : Gmail et Outlook mettent chaque ligne d'un message dans son
/// propre bloc, et une ligne vide y est un bloc contenant un `<br>`. Seul
/// `<br>` ajoute toujours un saut.
fn html_vers_texte(html: &str) -> String {
    let mut texte = String::with_capacity(html.len());
    let mut reste = html;
    // Élément dont on saute le contenu jusqu'à sa balise fermante.
    let mut ignorer: Option<String> = None;
    let ajouter = |texte: &mut String, fragment: &str| {
        // Dans un HTML, un saut de ligne du source n'est qu'un espace.
        for c in fragment.chars() {
            texte.push(if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c });
        }
    };
    while let Some(debut) = reste.find('<') {
        if ignorer.is_none() {
            ajouter(&mut texte, &reste[..debut]);
        }
        let apres = &reste[debut..];
        if apres.starts_with("<!--") {
            reste = apres.find("-->").map(|f| &apres[f + 3..]).unwrap_or("");
            continue;
        }
        let Some(fin) = apres.find('>') else {
            if ignorer.is_none() {
                ajouter(&mut texte, apres);
            }
            reste = "";
            break;
        };
        let balise = &apres[1..fin];
        reste = &apres[fin + 1..];
        let fermante = balise.starts_with('/');
        let nom: String = balise
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some(n) = &ignorer {
            if fermante && &nom == n {
                ignorer = None;
            }
            continue;
        }
        match nom.as_str() {
            "head" | "style" | "script" | "title" | "template" if !fermante => {
                ignorer = Some(nom);
            }
            "br" => texte.push('\n'),
            "p" | "div" | "tr" | "table" | "blockquote" | "ul" | "ol" | "hr" | "section"
            | "article" | "header" | "footer" | "center" | "pre" | "address" | "h1" | "h2"
            | "h3" | "h4" | "h5" | "h6" => a_la_ligne(&mut texte),
            "li" if !fermante => {
                a_la_ligne(&mut texte);
                texte.push_str("• ");
            }
            "td" | "th" if fermante => texte.push_str("   "),
            _ => {}
        }
    }
    if ignorer.is_none() {
        ajouter(&mut texte, reste);
    }
    let texte = decoder_entites(&texte);

    // Espaces résumés dans chaque ligne ; une seule ligne vide entre deux
    // paragraphes ; ni en tête ni en fin.
    let mut lignes: Vec<String> = Vec::new();
    for ligne in texte.lines() {
        let propre = ligne.split_whitespace().collect::<Vec<_>>().join(" ");
        if propre.is_empty() && lignes.last().map(|l| l.is_empty()).unwrap_or(true) {
            continue;
        }
        lignes.push(propre);
    }
    while lignes.last().map(|l| l.is_empty()).unwrap_or(false) {
        lignes.pop();
    }
    lignes.join("\n")
}

/// Passe à la ligne, sauf si la ligne en cours est encore vide (ou blanche) :
/// deux blocs qui se suivent ne font pas de ligne vide entre eux.
fn a_la_ligne(texte: &mut String) {
    let ligne = texte.rsplit('\n').next().unwrap_or("");
    if !ligne.trim().is_empty() {
        texte.push('\n');
    }
}

/// Décode les entités HTML : numériques (`&#233;`, `&#xE9;`) et nommées les
/// plus courantes. Une entité inconnue est laissée telle quelle.
fn decoder_entites(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut reste = texte;
    while let Some(i) = reste.find('&') {
        sortie.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        // Une entité tient en moins de 12 caractères avant son « ; ».
        let fin = apres.char_indices().take(12).find(|(_, c)| *c == ';').map(|(j, _)| j);
        match fin.and_then(|f| entite(&apres[..f]).map(|v| (f, v))) {
            Some((f, valeur)) => {
                sortie.push_str(&valeur);
                reste = &apres[f + 1..];
            }
            None => {
                sortie.push('&');
                reste = apres;
            }
        }
    }
    sortie.push_str(reste);
    sortie
}

fn entite(nom: &str) -> Option<String> {
    if let Some(nombre) = nom.strip_prefix('#') {
        let code = match nombre.strip_prefix(['x', 'X']) {
            Some(hexa) => u32::from_str_radix(hexa, 16).ok()?,
            None => nombre.parse().ok()?,
        };
        let c = char::from_u32(code)?;
        return Some(match c {
            // Invisibles : césure conditionnelle, liants.
            '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' => String::new(),
            c => c.to_string(),
        });
    }
    let valeur = match nom {
        "nbsp" | "ensp" | "emsp" | "thinsp" => " ",
        "shy" | "zwnj" | "zwj" => "",
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "eacute" => "é",
        "egrave" => "è",
        "ecirc" => "ê",
        "euml" => "ë",
        "agrave" => "à",
        "aacute" => "á",
        "acirc" => "â",
        "auml" => "ä",
        "ccedil" => "ç",
        "icirc" => "î",
        "iuml" => "ï",
        "ocirc" => "ô",
        "ouml" => "ö",
        "ugrave" => "ù",
        "ucirc" => "û",
        "uuml" => "ü",
        "Eacute" => "É",
        "Egrave" => "È",
        "Ecirc" => "Ê",
        "Agrave" => "À",
        "Ccedil" => "Ç",
        "oelig" => "œ",
        "OElig" => "Œ",
        "aelig" => "æ",
        "rsquo" => "’",
        "lsquo" => "‘",
        "rdquo" => "”",
        "ldquo" => "“",
        "laquo" => "«",
        "raquo" => "»",
        "hellip" => "…",
        "ndash" => "–",
        "mdash" => "—",
        "euro" => "€",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "deg" => "°",
        "middot" => "·",
        "bull" => "•",
        "times" => "×",
        _ => return None,
    };
    Some(valeur.to_string())
}

/// Nom d'affichage et adresse du premier contact d'un champ d'adresses.
fn premier_contact(adresses: &Address<'_>) -> (String, String) {
    adresses
        .iter()
        .next()
        .map(|a| {
            (
                a.name().unwrap_or_default().to_string(),
                a.address().unwrap_or_default().to_string(),
            )
        })
        .unwrap_or_default()
}

/// Destinataires, nommés comme dans la liste d'Outlook (« Nom; Nom ») : ceux
/// de `To`, à défaut ceux de `Cc` ; chacun par son nom d'affichage, à défaut
/// son adresse. La liste les montre à la place de l'expéditeur dans les
/// éléments envoyés, où l'expéditeur est partout soi-même.
fn destinataires(message: &Message<'_>) -> String {
    let noms = |adresses: Option<&Address<'_>>| -> Vec<String> {
        adresses
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        let nom = c.name().unwrap_or_default().trim();
                        let texte = if nom.is_empty() { c.address().unwrap_or_default().trim() } else { nom };
                        (!texte.is_empty()).then(|| texte.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut liste = noms(message.to());
    if liste.is_empty() {
        liste = noms(message.cc());
    }
    liste.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entete(brut: &str) -> Entete {
        Entete {
            uid: 7,
            taille: 512,
            drapeaux: vec!["\\Seen".to_string()],
            date_interne: "16-Sep-2026 18:00:00 +0200".to_string(),
            brut: brut.as_bytes().to_vec(),
            modseq: 0,
        }
    }

    #[test]
    fn importance_et_suivi() {
        let avec = |champs: &str| ligne_index(&entete(&format!("Subject: a\r\n{champs}\r\n\r\n")));
        assert_eq!(avec("Importance: High").importance, 1);
        assert_eq!(avec("Importance: low").importance, -1);
        assert_eq!(avec("X-Priority: 1 (Highest)").importance, 1);
        assert_eq!(avec("X-Priority: 5 (Lowest)").importance, -1);
        assert_eq!(avec("X-Priority: 3 (Normal)").importance, 0);
        assert_eq!(avec("Priority: urgent").importance, 1);
        assert_eq!(avec("X-Mailer: rien").importance, 0);
        // Importance prime sur X-Priority.
        assert_eq!(avec("Importance: normal\r\nX-Priority: 1").importance, 0);
        let mut e = entete("Subject: a\r\n\r\n");
        assert!(!ligne_index(&e).suivi);
        e.drapeaux.push("\\Flagged".into());
        assert!(ligne_index(&e).suivi);
    }

    #[test]
    fn pieces_supposees_d_apres_les_en_tetes() {
        let avec = |ct: &str| ligne_index(&entete(&format!("Subject: a\r\nContent-Type: {ct}\r\n\r\n"))).pieces;
        assert!(avec("multipart/mixed; boundary=\"x\""));
        assert!(avec("Multipart/Mixed; boundary=x"));
        assert!(avec("application/pdf; name=\"f.pdf\""));
        assert!(avec("image/jpeg"));
        assert!(!avec("multipart/alternative; boundary=x"));
        assert!(!avec("multipart/related; boundary=x"));
        assert!(!avec("text/plain; charset=utf-8"));
        assert!(!avec("text/html"));
        // Sans Content-Type : texte, par défaut.
        assert!(!ligne_index(&entete("Subject: a\r\n\r\n")).pieces);
        // Le mot-clé du serveur fait foi.
        let mut e = entete("Subject: a\r\nContent-Type: text/plain\r\n\r\n");
        e.drapeaux.push("$HasAttachment".into());
        assert!(ligne_index(&e).pieces);
        let mut e = entete("Subject: a\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n");
        e.drapeaux.push("$HasNoAttachment".into());
        assert!(!ligne_index(&e).pieces);
    }

    #[test]
    fn sujet_et_expediteur_decodes() {
        let ligne = ligne_index(&entete(
            "From: \"Service compta\" <compta@exemple.fr>\r\n\
             Message-ID: <abc.123@exemple.fr>\r\n\
             Subject: =?UTF-8?Q?R=C3=A9union_d'=C3=A9quipe?=\r\n\
             Date: Tue, 16 Sep 2026 09:30:00 +0200\r\n\r\n",
        ));
        assert_eq!(ligne.uid, 7);
        assert_eq!(ligne.sujet, "Réunion d'équipe");
        assert_eq!(ligne.expediteur, "Service compta");
        assert_eq!(ligne.adresse, "compta@exemple.fr");
        assert_eq!(ligne.message_id, "abc.123@exemple.fr");
        assert!(ligne.date.starts_with("2026-09-16"));
        assert!(ligne.lu);
        // Tri sur la date interne du serveur, 16/09/2026 16:00 UTC.
        assert_eq!(ligne.horodatage, 1789574400);
    }

    #[test]
    fn sans_nom_d_affichage_l_adresse_suffit() {
        let ligne = ligne_index(&entete("From: brut@exemple.fr\r\nSubject: Objet\r\n\r\n"));
        assert_eq!(ligne.expediteur, "brut@exemple.fr");
        assert_eq!(ligne.adresse, "brut@exemple.fr");
    }

    #[test]
    fn destinataires_de_to_a_defaut_de_cc() {
        let ligne = ligne_index(&entete(
            "From: Moi <moi@exemple.fr>\r\n\
             To: \"Dupont, Jean\" <jean@exemple.fr>, marie@exemple.fr\r\n\
             Cc: Copie <copie@exemple.fr>\r\n\r\n",
        ));
        assert_eq!(ligne.destinataires, "Dupont, Jean; marie@exemple.fr");
        let ligne = ligne_index(&entete("From: moi@exemple.fr\r\nCc: =?UTF-8?Q?Ren=C3=A9?= <rene@exemple.fr>\r\n\r\n"));
        assert_eq!(ligne.destinataires, "René");
        // En copie cachée seulement : rien à montrer.
        assert_eq!(ligne_index(&entete("From: moi@exemple.fr\r\n\r\n")).destinataires, "");
    }

    #[test]
    fn message_sans_sujet_ni_date() {
        let ligne = ligne_index(&entete("From: a@b.fr\r\n\r\n"));
        assert_eq!(ligne.sujet, "(sans objet)");
        // Repli sur la date interne du serveur.
        assert_eq!(ligne.date, "16-Sep-2026 18:00:00 +0200");
    }

    #[test]
    fn corps_texte_et_corps_html() {
        let texte = corps_affichable(
            b"From: a@b.fr\r\nSubject: O\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBonjour \xc3\xa0 tous.\r\n",
        );
        assert!(texte.contains("Bonjour à tous."));

        let html = corps_affichable(
            b"From: a@b.fr\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Co\xc3\xbbt : 12&nbsp;\xe2\x82\xac</p>\r\n",
        );
        assert!(html.contains("Coût"), "html rendu : {html}");
        assert!(!html.contains('<'));
    }

    #[test]
    fn image_affichee_par_le_corps_n_est_pas_une_piece() {
        let brut = "From: a@exemple.fr\r\nSubject: x\r\nMIME-Version: 1.0\r\n\
            Content-Type: multipart/mixed; boundary=\"M\"\r\n\r\n\
            --M\r\nContent-Type: multipart/related; boundary=\"R\"\r\n\r\n\
            --R\r\nContent-Type: text/html\r\n\r\n<img src=\"cid:logo@x\">\r\n\
            --R\r\nContent-Type: image/png; name=\"logo.png\"\r\nContent-ID: <logo@x>\r\n\
            Content-Disposition: inline; filename=\"logo.png\"\r\n\r\nPNG\r\n--R--\r\n\
            --M\r\nContent-Type: application/pdf; name=\"facture.pdf\"\r\n\
            Content-Disposition: attachment; filename=\"facture.pdf\"\r\n\r\n%PDF\r\n--M--\r\n";
        let pieces = pieces_jointes(brut.as_bytes());
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].nom, "facture.pdf");
        // Le rang reste celui du message : c'est lui qui désigne la pièce.
        assert_eq!(extraire_piece(brut.as_bytes(), pieces[0].indice).unwrap().0, "facture.pdf");
    }

    #[test]
    fn message_sans_partie_texte() {
        let corps = corps_affichable(b"From: a@b.fr\r\nContent-Type: image/png\r\n\r\n\x89PNG");
        assert!(corps.contains("pas de partie texte"));
    }

    #[test]
    fn en_tetes_illisibles_gardent_une_ligne() {
        let mut e = entete("");
        e.brut = vec![0xff, 0xfe, 0x00];
        let ligne = ligne_index(&e);
        assert_eq!(ligne.uid, 7);
        assert!(!ligne.sujet.is_empty());
    }

    const AVEC_PIECES: &[u8] = b"From: a@b.fr\r\n\
Subject: Pieces\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"XX\"\r\n\
\r\n\
--XX\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Voir pi\xc3\xa8ces jointes.\r\n\
--XX\r\n\
Content-Type: application/pdf; name=\"facture.pdf\"\r\n\
Content-Disposition: attachment; filename=\"facture.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQKJcOkw7w=\r\n\
--XX\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"../../Windows/outil.exe\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
TVqQAA==\r\n\
--XX--\r\n";

    #[test]
    fn pieces_jointes_listees_et_extraites() {
        let pieces = pieces_jointes(AVEC_PIECES);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert_eq!(pieces[0].nom, "facture.pdf");
        assert_eq!(pieces[0].type_mime, "application/pdf");
        assert!(!pieces[0].risquee);
        // Le chemin annoncé par l'expéditeur est écarté, l'exécutable signalé.
        assert_eq!(pieces[1].nom, "outil.exe");
        assert!(pieces[1].risquee);

        let (nom, octets) = extraire_piece(AVEC_PIECES, 0).unwrap();
        assert_eq!(nom, "facture.pdf");
        assert!(octets.starts_with(b"%PDF-1.4"));
        assert_eq!(pieces[0].taille, octets.len());
        assert!(extraire_piece(AVEC_PIECES, 5).is_none());
        // Le corps affichable ne compte pas parmi les pièces jointes.
        assert!(corps_affichable(AVEC_PIECES).contains("Voir pièces jointes."));
    }

    #[test]
    fn noms_de_fichiers_assainis() {
        assert_eq!(nom_sur("rapport.pdf", 0, "application/pdf"), "rapport.pdf");
        assert_eq!(nom_sur("..\\..\\x.txt", 0, "text/plain"), "x.txt");
        assert_eq!(nom_sur("a<b>c:d|e?.txt", 0, "text/plain"), "a_b_c_d_e_.txt");
        assert_eq!(nom_sur("", 2, "image/png"), "piece-jointe-3.png");
        assert_eq!(nom_sur("...", 0, "application/zip"), "piece-jointe-1.bin");
        assert_eq!(nom_sur("CON.txt", 0, "text/plain"), "piece-jointe-1.txt");
        assert_eq!(nom_sur("com1", 0, "text/plain"), "piece-jointe-1.txt");
        assert_eq!(nom_sur("console.txt", 0, "text/plain"), "console.txt");
        let long = format!("{}.pdf", "é".repeat(300));
        let court = nom_sur(&long, 0, "application/pdf");
        assert_eq!(court.chars().count(), 150);
        assert!(court.ends_with(".pdf"));
    }

    #[test]
    fn executables_reconnus() {
        for nom in ["x.exe", "X.JS", "a.b.vbs", "lien.lnk", "s.ps1", "outil.AppImage"] {
            assert!(ouverture_risquee(nom), "{nom}");
        }
        for nom in ["facture.pdf", "photo.jpeg", "exe", "tableau.xlsx", "notes.txt"] {
            assert!(!ouverture_risquee(nom), "{nom}");
        }
    }

    #[test]
    fn html_d_outlook_remis_en_lignes() {
        let html = concat!(
            "<html><head><title>Titre</title><style>p { color: red; }</style></head>",
            "<body><!-- commentaire --><div>Bonjour,</div><div><br></div>",
            "<div>Veuillez trouver ci-joint la facture n&deg;&nbsp;12 d&#233;taill&eacute;e :</div>",
            "<ul><li>ligne A</li><li>ligne B</li></ul>",
            "<table><tr><td>Total</td><td>12,50&nbsp;&euro;</td></tr></table>",
            "<p>Cordialement,<br>L&rsquo;&eacute;quipe</p><script>alert(1)</script></body></html>"
        );
        let texte = html_vers_texte(html);
        assert!(!texte.contains("color"), "le CSS doit disparaître : {texte}");
        assert!(!texte.contains("alert"), "le script doit disparaître : {texte}");
        assert!(!texte.contains("Titre"));
        assert!(!texte.contains("commentaire"));
        let lignes: Vec<&str> = texte.lines().collect();
        assert_eq!(lignes[0], "Bonjour,");
        assert!(texte.contains("\nVeuillez trouver ci-joint la facture n° 12 détaillée :\n"), "{texte}");
        assert!(texte.contains("• ligne A\n• ligne B"), "{texte}");
        assert!(texte.contains("Total 12,50 €"), "{texte}");
        assert!(texte.contains("Cordialement,\nL’équipe"), "{texte}");
        assert!(!texte.contains("\n\n\n"), "pas plus d'une ligne vide : {texte:?}");
    }

    #[test]
    fn blocs_successifs_sans_ligne_vide() {
        // Outlook : un paragraphe par ligne, une ligne vide = paragraphe à
        // espace insécable. Gmail : un <div> par ligne, <div><br></div> pour
        // une ligne vide. Et le source indenté ne doit rien ajouter.
        let outlook = "<p class=MsoNormal>Bonjour,<o:p></o:p></p>\n  <p class=MsoNormal><o:p>&nbsp;</o:p></p>\n  <p class=MsoNormal>Suite.<o:p></o:p></p>";
        assert_eq!(html_vers_texte(outlook), "Bonjour,\n\nSuite.");
        let gmail = "<div dir=\"ltr\">\n  <div>Un</div>\n  <div>Deux</div>\n  <div><br></div>\n  <div>Trois</div>\n</div>";
        assert_eq!(html_vers_texte(gmail), "Un\nDeux\n\nTrois");
    }

    #[test]
    fn entites_inconnues_ou_tronquees_laissees() {
        assert_eq!(decoder_entites("A &amp; B"), "A & B");
        assert_eq!(decoder_entites("R&D, 5 &lt; 6"), "R&D, 5 < 6");
        assert_eq!(decoder_entites("&inconnue; &#x1F600; &#9999999999;"), "&inconnue; 😀 &#9999999999;");
        assert_eq!(decoder_entites("fin &"), "fin &");
        assert_eq!(decoder_entites("é&eacute"), "é&eacute");
    }

    #[test]
    fn message_tout_en_html_passe_par_la_conversion() {
        let brut = b"From: a@b.fr\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<div>Un</div><div>Deux</div>\r\n";
        assert_eq!(corps_affichable(brut), "Un\nDeux");
    }
}
