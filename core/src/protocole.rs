// SPDX-License-Identifier: GPL-3.0-or-later
//! Analyse des réponses IMAP, sans réseau ni état.
//!
//! Le transport (`crate::imap`) lit une réponse complète et remplace chaque
//! littéral `{n}` par un marqueur, les octets étant rangés à part : ce qui reste
//! est du texte d'une seule ligne logique, que ces fonctions savent lire. C'est
//! ce découpage qui rend l'analyse vérifiable par des tests ordinaires.

/// Marqueur inséré à la place d'un littéral : un caractère que le protocole
/// n'emploie jamais, suivi de l'indice du littéral et du même caractère.
pub const MARQUEUR: char = '\u{1}';

/// Drapeaux d'un message répondu : `\Answered`, et le mot-clé `$Answered`
/// que posent certains clients (relevé sur une boîte partagée Mailcow).
pub const REPONDU: &[&str] = &["\\Answered", "$Answered"];

/// Drapeaux d'un message transféré. Il n'y a pas de drapeau système : c'est
/// le mot-clé `$Forwarded` (celui des téléphones, de Thunderbird, de MMail), et
/// `Forwarded`, sans `$`, de clients plus anciens.
pub const TRANSFERE: &[&str] = &["$Forwarded", "Forwarded"];

/// Réponse à tous : aucun drapeau standard ne la distingue d'une réponse. MMail
/// pose ce mot-clé, en plus de `\Answered`, quand on répond à tous depuis lui ;
/// une réponse à tous faite ailleurs reste une réponse.
pub const REPONDU_TOUS: &[&str] = &["$ReplyAll"];

/// Vrai si la liste porte l'un de ces drapeaux, sans égard à la casse.
pub fn porte(drapeaux: &[String], noms: &[&str]) -> bool {
    drapeaux.iter().any(|d| noms.iter().any(|n| d.eq_ignore_ascii_case(n)))
}

/// Un dossier tel que l'annonce `LIST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dossier {
    /// Chemin complet, tel qu'il est passé à `SELECT`.
    pub chemin: String,
    /// Séparateur de hiérarchie annoncé par le serveur (« / » chez Dovecot).
    pub separateur: String,
    /// Attributs : `\HasChildren`, `\Sent`, `\Trash`…
    pub attributs: Vec<String>,
}

impl Dossier {
    /// Nom affiché : le dernier segment du chemin, décodé de l'UTF-7 modifié
    /// dans lequel le serveur transporte les noms accentués (RFC 3501 § 5.1.3).
    pub fn nom(&self) -> String {
        let segment = match self.separateur.chars().next() {
            Some(sep) => self.chemin.rsplit(sep).next().unwrap_or(&self.chemin),
            None => &self.chemin,
        };
        decoder_utf7(segment)
    }

    /// Faux pour un nœud de hiérarchie qui ne contient pas de messages : il
    /// s'affiche, mais ne s'ouvre pas et ne reçoit rien.
    pub fn selectionnable(&self) -> bool {
        !self
            .attributs
            .iter()
            .any(|a| a.eq_ignore_ascii_case("\\Noselect") || a.eq_ignore_ascii_case("\\NonExistent"))
    }

    /// Profondeur dans l'arborescence, 0 pour un dossier de premier niveau.
    pub fn profondeur(&self) -> usize {
        match self.separateur.chars().next() {
            Some(sep) => self.chemin.matches(sep).count(),
            None => 0,
        }
    }

    /// Rôle particulier du dossier, d'après SPECIAL-USE (`\Sent` → « Sent »).
    pub fn role(&self) -> Option<&str> {
        for attribut in &self.attributs {
            if let Some(nom) = attribut.strip_prefix('\\') {
                if matches!(nom, "Sent" | "Drafts" | "Trash" | "Junk" | "Archive" | "All" | "Flagged")
                {
                    return Some(nom);
                }
            }
        }
        None
    }
}

/// État d'un dossier après `SELECT` : ce qu'il faut pour suivre sa synchronisation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatDossier {
    pub messages: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_mod_seq: u64,
}

/// En-têtes d'un message, tels que les rend un `FETCH` d'index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entete {
    pub uid: u32,
    pub taille: u32,
    pub drapeaux: Vec<String>,
    pub date_interne: String,
    /// Bloc d'en-têtes brut, à analyser par `mail-parser`.
    pub brut: Vec<u8>,
    /// `MODSEQ` du message (CONDSTORE), quand il a été demandé ; 0 sinon.
    pub modseq: u64,
}

impl Entete {
    pub fn lu(&self) -> bool {
        self.drapeaux.iter().any(|d| d == "\\Seen")
    }

    pub fn repondu(&self) -> bool {
        porte(&self.drapeaux, REPONDU)
    }

    pub fn transfere(&self) -> bool {
        porte(&self.drapeaux, TRANSFERE)
    }

    pub fn repondu_tous(&self) -> bool {
        porte(&self.drapeaux, REPONDU_TOUS)
    }

    /// Drapeau de suivi (`\Flagged`), celui qu'Outlook appelle « assurer un
    /// suivi ».
    pub fn suivi(&self) -> bool {
        self.drapeaux.iter().any(|d| d.eq_ignore_ascii_case("\\Flagged"))
    }

    /// Marqué pour suppression (`\Deleted`) : par ce client ou par un autre.
    pub fn supprime(&self) -> bool {
        self.drapeaux.iter().any(|d| d.eq_ignore_ascii_case("\\Deleted"))
    }
}

/// Compteurs d'un dossier rendus par `STATUS` (ou `LIST … RETURN (STATUS …)`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statut {
    pub chemin: String,
    pub messages: u32,
    pub non_lus: u32,
}

/// Découpe une ligne de réponse `LIST` : `* LIST (attributs) "sép" "chemin"`.
pub fn analyser_list(ligne: &str) -> Option<Dossier> {
    let reste = ligne.strip_prefix("* LIST ")?;
    let (attributs, reste) = entre_parentheses(reste)?;
    let attributs = attributs
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let (separateur, reste) = jeton(reste.trim_start())?;
    let (chemin, _) = jeton(reste.trim_start())?;
    Some(Dossier {
        chemin,
        separateur,
        attributs,
    })
}

/// Lit l'état d'un dossier dans les réponses non étiquetées d'un `SELECT`.
pub fn analyser_select(reponse: &str) -> EtatDossier {
    let mut etat = EtatDossier::default();
    for ligne in reponse.lines() {
        let ligne = ligne.trim();
        if let Some(reste) = ligne.strip_prefix("* ") {
            if let Some(nombre) = reste.strip_suffix(" EXISTS") {
                etat.messages = nombre.trim().parse().unwrap_or(0);
            }
        }
        // Les autres valeurs arrivent en code de réponse : « * OK [UIDNEXT 11] … »
        if let Some(valeur) = valeur_de_code(ligne, "UIDVALIDITY ") {
            etat.uid_validity = valeur.parse().unwrap_or(0);
        }
        if let Some(valeur) = valeur_de_code(ligne, "UIDNEXT ") {
            etat.uid_next = valeur.parse().unwrap_or(0);
        }
        if let Some(valeur) = valeur_de_code(ligne, "HIGHESTMODSEQ ") {
            etat.highest_mod_seq = valeur.parse().unwrap_or(0);
        }
    }
    etat
}

/// Valeur d'un code de réponse entre crochets : `[UIDNEXT 11]` → « 11 ».
fn valeur_de_code<'a>(ligne: &'a str, clef: &str) -> Option<&'a str> {
    let debut = ligne.find(&format!("[{clef}"))? + clef.len() + 1;
    let reste = &ligne[debut..];
    let fin = reste.find(']')?;
    Some(reste[..fin].trim())
}

/// Découpe les réponses `FETCH` d'un relevé d'en-têtes.
///
/// `litteraux` porte les blocs extraits par le transport, dans l'ordre.
pub fn analyser_fetch(reponse: &str, litteraux: &[Vec<u8>]) -> Vec<Entete> {
    let mut entetes = Vec::new();
    for ligne in reponse.lines() {
        let ligne = ligne.trim();
        if !ligne.starts_with("* ") || !ligne.contains(" FETCH ") {
            continue;
        }
        let Some((corps, _)) = ligne.find(" FETCH ").and_then(|i| entre_parentheses(&ligne[i + 7..]))
        else {
            continue;
        };
        let mut entete = Entete::default();
        entete.uid = nombre_apres(&corps, "UID ").unwrap_or(0) as u32;
        entete.taille = nombre_apres(&corps, "RFC822.SIZE ").unwrap_or(0) as u32;
        entete.modseq = nombre_apres(&corps, "MODSEQ (").unwrap_or(0);
        if let Some(reste) = corps.split_once("FLAGS ").map(|(_, r)| r) {
            if let Some((drapeaux, _)) = entre_parentheses(reste) {
                entete.drapeaux = drapeaux.split_whitespace().map(str::to_string).collect();
            }
        }
        if let Some(reste) = corps.split_once("INTERNALDATE ").map(|(_, r)| r) {
            if let Some((date, _)) = jeton(reste) {
                entete.date_interne = date;
            }
        }
        if let Some(indice) = indice_litteral(&corps) {
            entete.brut = litteraux.get(indice).cloned().unwrap_or_default();
        }
        if entete.uid != 0 {
            entetes.push(entete);
        }
    }
    entetes
}

/// Indice du premier littéral cité dans un fragment de réponse.
pub fn indice_litteral(texte: &str) -> Option<usize> {
    let debut = texte.find(MARQUEUR)? + MARQUEUR.len_utf8();
    let reste = &texte[debut..];
    let fin = reste.find(MARQUEUR)?;
    reste[..fin].parse().ok()
}

/// Contenu du premier groupe entre parenthèses, et ce qui le suit.
fn entre_parentheses(texte: &str) -> Option<(String, &str)> {
    let debut = texte.find('(')?;
    let mut profondeur = 0usize;
    for (i, c) in texte[debut..].char_indices() {
        match c {
            '(' => profondeur += 1,
            ')' => {
                profondeur -= 1;
                if profondeur == 0 {
                    let fin = debut + i;
                    return Some((texte[debut + 1..fin].to_string(), &texte[fin + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Premier jeton : chaîne entre guillemets, `NIL`, ou suite sans espace.
fn jeton(texte: &str) -> Option<(String, &str)> {
    let texte = texte.trim_start();
    if let Some(reste) = texte.strip_prefix('"') {
        // Chaîne citée : « \" » et « \\ » y sont des échappements.
        let mut valeur = String::new();
        let mut echappe = false;
        for (i, c) in reste.char_indices() {
            if echappe {
                valeur.push(c);
                echappe = false;
            } else if c == '\\' {
                echappe = true;
            } else if c == '"' {
                return Some((valeur, &reste[i + 1..]));
            } else {
                valeur.push(c);
            }
        }
        return None;
    }
    let fin = texte.find(' ').unwrap_or(texte.len());
    if fin == 0 {
        return None;
    }
    Some((texte[..fin].to_string(), &texte[fin..]))
}

/// UID retirés du dossier, tels que les annonce `VANISHED` (QRESYNC,
/// RFC 7162) : « * VANISHED (EARLIER) 1:3,7 ». Rendus en intervalles, sans les
/// déplier — un intervalle peut couvrir des UID qui n'ont jamais existé.
pub fn analyser_vanished(reponse: &str) -> Vec<(u32, u32)> {
    let mut intervalles = Vec::new();
    for ligne in reponse.lines() {
        let Some(reste) = ligne.trim().strip_prefix("* VANISHED ") else {
            continue;
        };
        let reste = reste.trim_start_matches("(EARLIER)").trim();
        intervalles.extend(lire_ensemble(reste));
    }
    intervalles
}

/// Lit un ensemble d'UID : « 1:3,7,9:12 ».
pub fn lire_ensemble(texte: &str) -> Vec<(u32, u32)> {
    texte
        .split(',')
        .filter_map(|morceau| {
            let morceau = morceau.trim();
            match morceau.split_once(':') {
                Some((a, b)) => {
                    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
                    Some((a.min(b), a.max(b)))
                }
                None => morceau.parse().ok().map(|n| (n, n)),
            }
        })
        .collect()
}

/// Écrit un ensemble d'UID sous sa forme compacte : [1,2,3,7] → « 1:3,7 ».
pub fn ensemble_uid(uids: &[u32]) -> String {
    let mut tries: Vec<u32> = uids.to_vec();
    tries.sort_unstable();
    tries.dedup();
    let mut morceaux = Vec::new();
    let mut i = 0;
    while i < tries.len() {
        let debut = tries[i];
        let mut fin = debut;
        while i + 1 < tries.len() && tries[i + 1] == fin + 1 {
            i += 1;
            fin = tries[i];
        }
        morceaux.push(if debut == fin { debut.to_string() } else { format!("{debut}:{fin}") });
        i += 1;
    }
    morceaux.join(",")
}

/// Compteurs des réponses `STATUS` : « * STATUS "Essais" (MESSAGES 5 UNSEEN 2) ».
pub fn analyser_status(reponse: &str) -> Vec<Statut> {
    let mut statuts = Vec::new();
    for ligne in reponse.lines() {
        let Some(reste) = ligne.trim().strip_prefix("* STATUS ") else {
            continue;
        };
        let Some((chemin, reste)) = jeton(reste) else {
            continue;
        };
        let Some((valeurs, _)) = entre_parentheses(reste) else {
            continue;
        };
        statuts.push(Statut {
            chemin,
            messages: nombre_apres(&valeurs, "MESSAGES ").unwrap_or(0) as u32,
            non_lus: nombre_apres(&valeurs, "UNSEEN ").unwrap_or(0) as u32,
        });
    }
    statuts
}

/// UID attribué par la cible d'un `APPEND` (UIDPLUS, RFC 4315) :
/// « OK [APPENDUID 1789577698 12] » → (1789577698, 12).
pub fn analyser_appenduid(reponse: &str) -> Option<(u32, u32)> {
    let valeur = reponse.lines().find_map(|l| valeur_de_code(l, "APPENDUID "))?;
    let mut parties = valeur.split_whitespace();
    let validite = parties.next()?.parse().ok()?;
    let uid = parties.next()?.parse().ok()?;
    Some((validite, uid))
}

/// UID rendus par `UID SEARCH` : « * SEARCH 3 5 7 ».
pub fn analyser_search(reponse: &str) -> Vec<u32> {
    reponse
        .lines()
        .filter_map(|l| l.trim().strip_prefix("* SEARCH"))
        .flat_map(|reste| reste.split_whitespace().filter_map(|n| n.parse().ok()))
        .collect()
}

/// Date interne IMAP (« 16-Sep-2026 18:00:00 +0200 ») en secondes depuis
/// l'époque Unix. Sert de clef de tri : elle est posée par le serveur à la
/// réception, et `APPEND` la conserve lors d'un déplacement entre boîtes.
pub fn secondes_date_interne(date: &str) -> Option<i64> {
    let date = date.trim();
    let (jour_mois_an, reste) = date.split_once(' ')?;
    let (heure, zone) = reste.trim().split_once(' ')?;
    let mut jma = jour_mois_an.trim().split('-');
    let jour: i64 = jma.next()?.trim().parse().ok()?;
    let mois = match jma.next()? {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4, "May" => 5, "Jun" => 6,
        "Jul" => 7, "Aug" => 8, "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    };
    let an: i64 = jma.next()?.parse().ok()?;
    let mut hms = heure.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let m: i64 = hms.next()?.parse().ok()?;
    let s: i64 = hms.next()?.parse().ok()?;
    let signe = match zone.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let zh: i64 = zone.get(1..3)?.parse().ok()?;
    let zm: i64 = zone.get(3..5)?.parse().ok()?;
    let decalage = signe * (zh * 3600 + zm * 60);
    Some(jours_depuis_epoque(an, mois, jour) * 86400 + h * 3600 + m * 60 + s - decalage)
}

/// Jours écoulés depuis le 1er janvier 1970 (calendrier grégorien proleptique).
pub(crate) fn jours_depuis_epoque(an: i64, mois: i64, jour: i64) -> i64 {
    let a = if mois <= 2 { an - 1 } else { an };
    let ere = a.div_euclid(400);
    let annee_ere = a - ere * 400;
    let m = if mois > 2 { mois - 3 } else { mois + 9 };
    let jour_annee = (153 * m + 2) / 5 + jour - 1;
    let jour_ere = annee_ere * 365 + annee_ere / 4 - annee_ere / 100 + jour_annee;
    ere * 146097 + jour_ere - 719468
}

/// Décode un nom de dossier transporté en UTF-7 modifié (RFC 3501 § 5.1.3) :
/// « &AMk-l&AOk-ments » → « Éléments ». Un nom mal formé est rendu tel quel
/// plutôt que perdu.
pub fn decoder_utf7(nom: &str) -> String {
    if !nom.contains('&') {
        return nom.to_string();
    }
    let mut sortie = String::new();
    let mut reste = nom;
    while let Some(debut) = reste.find('&') {
        sortie.push_str(&reste[..debut]);
        let apres = &reste[debut + 1..];
        let Some(fin) = apres.find('-') else {
            return nom.to_string();
        };
        let code = &apres[..fin];
        if code.is_empty() {
            sortie.push('&');
        } else {
            match base64_utf7(code) {
                Some(texte) => sortie.push_str(&texte),
                None => return nom.to_string(),
            }
        }
        reste = &apres[fin + 1..];
    }
    sortie.push_str(reste);
    sortie
}

/// Chemin, sur le serveur, du sous-dossier `nom` — tel que l'utilisateur l'a
/// tapé — de `parent`, chemin du serveur déjà en UTF-7 modifié. L'erreur se
/// lit telle quelle dans l'interface.
pub fn chemin_de_sous_dossier(parent: &str, separateur: &str, nom: &str) -> Result<String, String> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err("indiquez le nom du dossier".into());
    }
    // « NIL » : le serveur ne connaît qu'un niveau de dossiers.
    if separateur.is_empty() || separateur.eq_ignore_ascii_case("NIL") {
        return Err("ce serveur ne range pas de dossiers dans d'autres".into());
    }
    if nom.contains(separateur) {
        return Err(format!("le nom ne peut pas contenir « {separateur} »"));
    }
    // « % » et « * » sont les jokers de LIST : un dossier qui en porte se
    // relit mal.
    if let Some(c) = nom.chars().find(|c| c.is_control() || *c == '%' || *c == '*') {
        return Err(if c.is_control() {
            "le nom contient un caractère invisible".into()
        } else {
            format!("le nom ne peut pas contenir « {c} »")
        });
    }
    Ok(format!("{parent}{separateur}{}", encoder_utf7(nom)))
}

/// Encode un nom de dossier en UTF-7 modifié, pour le créer sur le serveur :
/// « Envoi différé » → « Envoi diff&AOk-r&AOk- ».
pub fn encoder_utf7(nom: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";
    let mut sortie = String::new();
    let mut attente: Vec<u16> = Vec::new();
    let vider = |attente: &mut Vec<u16>, sortie: &mut String| {
        if attente.is_empty() {
            return;
        }
        let octets: Vec<u8> = attente.iter().flat_map(|u| u.to_be_bytes()).collect();
        sortie.push('&');
        for bloc in octets.chunks(3) {
            let n = (bloc[0] as u32) << 16
                | (*bloc.get(1).unwrap_or(&0) as u32) << 8
                | *bloc.get(2).unwrap_or(&0) as u32;
            let utiles = bloc.len() + 1;
            for (i, decalage) in [18, 12, 6, 0].into_iter().enumerate() {
                if i < utiles {
                    sortie.push(TABLE[(n >> decalage & 63) as usize] as char);
                }
            }
        }
        sortie.push('-');
        attente.clear();
    };
    for c in nom.chars() {
        if (' '..='~').contains(&c) {
            vider(&mut attente, &mut sortie);
            if c == '&' {
                sortie.push_str("&-");
            } else {
                sortie.push(c);
            }
        } else {
            let mut tampon = [0u16; 2];
            attente.extend_from_slice(c.encode_utf16(&mut tampon));
        }
    }
    vider(&mut attente, &mut sortie);
    sortie
}

/// Base64 modifié (« , » au lieu de « / », sans remplissage) vers UTF-16BE.
fn base64_utf7(code: &str) -> Option<String> {
    let mut bits: u32 = 0;
    let mut nb_bits = 0;
    let mut octets = Vec::new();
    for c in code.bytes() {
        let valeur = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b',' => 63,
            _ => return None,
        } as u32;
        bits = (bits << 6) | valeur;
        nb_bits += 6;
        if nb_bits >= 8 {
            nb_bits -= 8;
            octets.push((bits >> nb_bits) as u8);
            bits &= (1 << nb_bits) - 1;
        }
    }
    let unites: Vec<u16> = octets
        .chunks_exact(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16(&unites).ok()
}

fn nombre_apres(texte: &str, clef: &str) -> Option<u64> {
    let reste = texte.split_once(clef)?.1;
    let fin = reste
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(reste.len());
    reste[..fin].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossiers_et_roles() {
        let d = analyser_list(r#"* LIST (\HasNoChildren \Sent) "/" "Sent""#).unwrap();
        assert_eq!(d.chemin, "Sent");
        assert_eq!(d.role(), Some("Sent"));
        assert_eq!(d.profondeur(), 0);

        let d = analyser_list(r#"* LIST (\HasNoChildren) "/" "Essais/Factures""#).unwrap();
        assert_eq!(d.nom(), "Factures");
        assert_eq!(d.profondeur(), 1);
        assert_eq!(d.role(), None);

        // Dovecot rend le chemin sans guillemets quand il n'en a pas besoin.
        let d = analyser_list(r#"* LIST (\HasChildren \Marked) "/" Essais"#).unwrap();
        assert_eq!(d.chemin, "Essais");
        assert!(d.attributs.contains(&"\\HasChildren".to_string()));
    }

    #[test]
    fn drapeaux_repondu_et_transfere() {
        let e = |d: &[&str]| Entete { drapeaux: d.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        assert!(e(&["\\Answered"]).repondu());
        assert!(e(&["\\ANSWERED"]).repondu());
        assert!(e(&["$Answered"]).repondu());
        assert!(!e(&["\\Seen", "$Forwarded"]).repondu());
        assert!(e(&["$Forwarded"]).transfere());
        assert!(e(&["$forwarded"]).transfere());
        assert!(e(&["Forwarded"]).transfere());
        assert!(!e(&["\\Answered"]).transfere());
        assert!(e(&["\\Answered", "$ReplyAll"]).repondu_tous());
        assert!(!e(&["\\Answered"]).repondu_tous());
    }

    #[test]
    fn etat_apres_select() {
        let reponse = concat!(
            "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n",
            "* 10 EXISTS\r\n",
            "* 0 RECENT\r\n",
            "* OK [UIDVALIDITY 1789577698] UIDs valid\r\n",
            "* OK [UIDNEXT 11] Predicted next UID\r\n",
            "* OK [HIGHESTMODSEQ 11] Highest\r\n",
        );
        let etat = analyser_select(reponse);
        assert_eq!(etat.messages, 10);
        assert_eq!(etat.uid_validity, 1789577698);
        assert_eq!(etat.uid_next, 11);
        assert_eq!(etat.highest_mod_seq, 11);
    }

    #[test]
    fn entetes_de_fetch() {
        let reponse = format!(
            "* 1 FETCH (UID 3 RFC822.SIZE 412 FLAGS (\\Seen) \
             INTERNALDATE \"16-Sep-2026 18:00:00 +0200\" \
             BODY[HEADER.FIELDS (FROM SUBJECT)] {m}0{m})\r\n",
            m = MARQUEUR
        );
        let litteraux = vec![b"From: a@b.fr\r\nSubject: Objet\r\n\r\n".to_vec()];
        let entetes = analyser_fetch(&reponse, &litteraux);
        assert_eq!(entetes.len(), 1);
        let e = &entetes[0];
        assert_eq!(e.uid, 3);
        assert_eq!(e.taille, 412);
        assert!(e.lu());
        assert!(!e.repondu());
        assert_eq!(e.date_interne, "16-Sep-2026 18:00:00 +0200");
        assert!(String::from_utf8_lossy(&e.brut).contains("Objet"));
    }

    #[test]
    fn modseq_et_suppression_lus_dans_fetch() {
        let reponse = "* 4 FETCH (UID 9 MODSEQ (12345) FLAGS (\\Seen \\Deleted))\r\n";
        let e = &analyser_fetch(reponse, &[])[0];
        assert_eq!(e.uid, 9);
        assert_eq!(e.modseq, 12345);
        assert!(e.supprime());
        // Sans MODSEQ demandé, rien n'est inventé.
        let e = &analyser_fetch("* 4 FETCH (UID 9 FLAGS ())\r\n", &[])[0];
        assert_eq!(e.modseq, 0);
        assert!(!e.supprime());
    }

    #[test]
    fn fetch_sans_uid_est_ignore() {
        // Une réponse FETCH peut annoncer un simple changement de drapeaux.
        let reponse = "* 2 FETCH (FLAGS (\\Seen))\r\n";
        assert!(analyser_fetch(reponse, &[]).is_empty());
    }

    #[test]
    fn parentheses_imbriquees() {
        let (contenu, reste) = entre_parentheses("(a (b c) d) suite").unwrap();
        assert_eq!(contenu, "a (b c) d");
        assert_eq!(reste.trim(), "suite");
    }

    #[test]
    fn noms_accentues_en_utf7() {
        assert_eq!(decoder_utf7("&AMk-l&AOk-ments supprim&AOk-s"), "Éléments supprimés");
        assert_eq!(decoder_utf7("Factures"), "Factures");
        assert_eq!(decoder_utf7("R&AOk-sum&AOk- &- notes"), "Résumé & notes");
        // Mal formé : rendu tel quel.
        assert_eq!(decoder_utf7("&AMk"), "&AMk");
        // Et dans l'autre sens.
        for nom in ["Envoi différé", "Éléments supprimés", "Résumé & notes", "Factures", "日本語 €"] {
            assert_eq!(decoder_utf7(&encoder_utf7(nom)), nom, "{nom}");
        }
        assert_eq!(encoder_utf7("Éléments supprimés"), "&AMk-l&AOk-ments supprim&AOk-s");
        assert_eq!(encoder_utf7("a&b"), "a&-b");
        let d = analyser_list(r#"* LIST (\HasNoChildren) "." "INBOX.&AMk-l&AOk-ments envoy&AOk-s""#).unwrap();
        assert_eq!(d.nom(), "Éléments envoyés");
        assert_eq!(d.profondeur(), 1);
    }

    #[test]
    fn chemins_de_sous_dossiers() {
        assert_eq!(chemin_de_sous_dossier("INBOX", "/", " Factures 2026 ").unwrap(), "INBOX/Factures 2026");
        assert_eq!(
            chemin_de_sous_dossier("INBOX.&AMk-l&AOk-ments envoy&AOk-s", ".", "Réglés").unwrap(),
            "INBOX.&AMk-l&AOk-ments envoy&AOk-s.R&AOk-gl&AOk-s"
        );
        assert_eq!(chemin_de_sous_dossier("Clients", "/", "A & B").unwrap(), "Clients/A &- B");
        assert!(chemin_de_sous_dossier("INBOX", "/", "  ").is_err());
        assert_eq!(
            chemin_de_sous_dossier("INBOX", ".", "v1.2").unwrap_err(),
            "le nom ne peut pas contenir « . »"
        );
        assert!(chemin_de_sous_dossier("INBOX", "/", "a/b").is_err());
        assert!(chemin_de_sous_dossier("INBOX", "/", "tout*").is_err());
        assert!(chemin_de_sous_dossier("INBOX", "/", "50%").is_err());
        assert!(chemin_de_sous_dossier("INBOX", "/", "a\tb").is_err());
        assert!(chemin_de_sous_dossier("INBOX", "NIL", "Factures").is_err());
        assert!(chemin_de_sous_dossier("INBOX", "", "Factures").is_err());
    }

    #[test]
    fn dossier_non_selectionnable() {
        let d = analyser_list(r#"* LIST (\Noselect \HasChildren) "." "INBOX""#).unwrap();
        assert!(!d.selectionnable());
        let d = analyser_list(r#"* LIST (\HasNoChildren) "/" "Sent""#).unwrap();
        assert!(d.selectionnable());
    }

    #[test]
    fn chemin_cite_avec_echappements() {
        let d = analyser_list(r#"* LIST () "/" "Dossier \"cité\"""#).unwrap();
        assert_eq!(d.chemin, "Dossier \"cité\"");
    }

    #[test]
    fn ensembles_d_uid() {
        assert_eq!(ensemble_uid(&[7, 1, 2, 3, 9, 10]), "1:3,7,9:10");
        assert_eq!(ensemble_uid(&[5]), "5");
        assert_eq!(ensemble_uid(&[4, 4]), "4");
        assert_eq!(lire_ensemble("1:3,7,12:10"), vec![(1, 3), (7, 7), (10, 12)]);
    }

    #[test]
    fn vanished_en_intervalles() {
        let reponse = "* VANISHED (EARLIER) 1:3,7\r\n* 2 FETCH (UID 9 FLAGS (\\Seen))\r\n* VANISHED 12\r\n";
        assert_eq!(analyser_vanished(reponse), vec![(1, 3), (7, 7), (12, 12)]);
    }

    #[test]
    fn statuts_de_list_status() {
        let reponse = concat!(
            "* LIST (\\HasNoChildren) \"/\" INBOX\r\n",
            "* STATUS INBOX (MESSAGES 10 UNSEEN 4)\r\n",
            "* STATUS \"Archives 2026\" (MESSAGES 0 UNSEEN 0)\r\n",
        );
        let statuts = analyser_status(reponse);
        assert_eq!(statuts.len(), 2);
        assert_eq!(statuts[0], Statut { chemin: "INBOX".into(), messages: 10, non_lus: 4 });
        assert_eq!(statuts[1].chemin, "Archives 2026");
    }

    #[test]
    fn appenduid_et_search() {
        assert_eq!(
            analyser_appenduid("m0007 OK [APPENDUID 1789577698 12] Append completed.\n"),
            Some((1789577698, 12))
        );
        assert_eq!(analyser_appenduid("m0007 OK Append completed.\n"), None);
        assert_eq!(analyser_search("* SEARCH 3 5 7\r\nm0008 OK\r\n"), vec![3, 5, 7]);
        assert!(analyser_search("* SEARCH\r\nm0008 OK\r\n").is_empty());
    }

    #[test]
    fn dates_internes() {
        // 16/09/2026 16:00:00 UTC = 1789574400.
        assert_eq!(secondes_date_interne("16-Sep-2026 18:00:00 +0200"), Some(1789574400));
        assert_eq!(secondes_date_interne(" 6-Sep-2026 18:00:00 +0200"), Some(1789574400 - 10 * 86400));
        assert_eq!(secondes_date_interne("01-Jan-1970 00:00:00 +0000"), Some(0));
        assert_eq!(secondes_date_interne("n'importe quoi"), None);
    }
}
