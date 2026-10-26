// SPDX-License-Identifier: GPL-3.0-or-later
//! Fiche de contact (0.6.20) : le formulaire d'un contact tel que l'interface
//! l'échange avec le noyau ; la vCard qui le porte, créée, modifiée ou
//! fusionnée ; les fiches vides et les doublons d'un carnet.
//!
//! Une modification part de la vCard telle que le serveur l'a rendue et ne
//! touche que ce que l'utilisateur a changé dans le formulaire : photo,
//! anniversaire, catégories, propriétés propres à un autre logiciel (`X-…`),
//! groupes (`item1.`) et paramètres des lignes gardées restent tels quels,
//! au caractère près. Le changement se mesure entre la fiche montrée
//! (`origine`) et la fiche saisie : appliqué à une version plus récente,
//! après un conflit, il garde ce que d'autres ont changé ailleurs entre-temps.
//!
//! vCard 3.0 (RFC 2426, celle de SOGo) et 4.0 (RFC 6350) ; une fiche neuve
//! s'écrit en 3.0.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agenda::echapper;

/// Une adresse de courriel ou un numéro de téléphone du formulaire.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Coordonnee {
    /// « travail », « domicile », « portable » (téléphone), « fax »
    /// (téléphone) ou « autre ».
    pub genre: String,
    pub valeur: String,
    /// Rang de la coordonnée dans la fiche d'origine ; absent pour une ligne
    /// ajoutée dans le formulaire.
    pub cle: Option<usize>,
}

/// L'adresse postale du formulaire : la première de la fiche.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Adresse {
    /// « travail », « domicile » ou « autre ».
    pub genre: String,
    pub rue: String,
    #[serde(rename = "codePostal")]
    pub code_postal: String,
    pub ville: String,
    pub pays: String,
}

impl Adresse {
    fn vide(&self) -> bool {
        [&self.rue, &self.code_postal, &self.ville, &self.pays].iter().all(|t| t.trim().is_empty())
    }
}

/// Le formulaire d'un contact.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Fiche {
    /// Fiche de l'index ; 0 pour un nouveau contact.
    pub id: i64,
    /// Carnet de l'index où la fiche est, ou sera créée.
    pub carnet: i64,
    /// ETag de la version montrée : une écriture ne passe que si le serveur
    /// a toujours celle-là.
    pub etag: String,
    #[serde(rename = "nomAffiche")]
    pub nom_affiche: String,
    pub prenom: String,
    pub nom: String,
    pub organisation: String,
    pub fonction: String,
    pub courriels: Vec<Coordonnee>,
    pub telephones: Vec<Coordonnee>,
    pub adresse: Adresse,
    pub note: String,
}

// ------------------------------------------------------------- vCard

/// Une ligne logique de la vCard : son texte, rendu tel quel tant qu'elle
/// n'est pas touchée, et ce qu'on en lit.
#[derive(Debug, Clone)]
struct Ligne {
    texte: String,
    /// Groupe (`item1` dans `item1.EMAIL`), vide sinon.
    groupe: String,
    /// Nom de la propriété, en majuscules.
    nom: String,
    /// Paramètres, noms en majuscules ; un paramètre nu de la vCard 2.1
    /// (`TEL;CELL:`) est un `TYPE`.
    params: Vec<(String, String)>,
    /// Valeur telle qu'écrite, échappements compris.
    valeur: String,
}

impl Ligne {
    fn lire(texte: &str) -> Option<Ligne> {
        let mut guillemets = false;
        let mut coupures = Vec::new();
        let mut fin_tete = None;
        for (i, c) in texte.char_indices() {
            match c {
                '"' => guillemets = !guillemets,
                ';' if !guillemets => coupures.push(i),
                ':' if !guillemets => {
                    fin_tete = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let fin_tete = fin_tete?;
        let mut bornes = vec![0];
        bornes.extend(coupures.iter().map(|i| i + 1));
        let mut morceaux: Vec<&str> = Vec::new();
        for (k, debut) in bornes.iter().enumerate() {
            let fin = coupures.get(k).copied().unwrap_or(fin_tete);
            morceaux.push(&texte[*debut..fin]);
        }
        let premier = morceaux[0].trim();
        let (groupe, nom) = premier.rsplit_once('.').unwrap_or(("", premier));
        if nom.is_empty() {
            return None;
        }
        let params = morceaux[1..]
            .iter()
            .filter(|m| !m.trim().is_empty())
            .map(|m| match m.split_once('=') {
                Some((n, v)) => (n.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()),
                None => ("TYPE".to_string(), m.trim().to_string()),
            })
            .collect();
        Some(Ligne {
            texte: texte.to_string(),
            groupe: groupe.to_string(),
            nom: nom.to_ascii_uppercase(),
            params,
            valeur: texte[fin_tete + 1..].to_string(),
        })
    }

    fn nouvelle(nom: &str, params: Vec<(String, String)>, valeur: String) -> Ligne {
        let mut l = Ligne { texte: String::new(), groupe: String::new(), nom: nom.to_string(), params, valeur };
        l.reecrire();
        l
    }

    /// Recompose le texte d'une ligne touchée.
    fn reecrire(&mut self) {
        let mut t = String::new();
        if !self.groupe.is_empty() {
            t.push_str(&self.groupe);
            t.push('.');
        }
        t.push_str(&self.nom);
        for (n, v) in &self.params {
            t.push(';');
            t.push_str(n);
            t.push('=');
            // Une liste (`TYPE=work,pref`) reste une liste : seuls « : » et
            // « ; » imposent des guillemets.
            if v.contains([':', ';']) {
                t.push('"');
                t.push_str(&v.replace('"', ""));
                t.push('"');
            } else {
                t.push_str(v);
            }
        }
        t.push(':');
        t.push_str(&self.valeur);
        self.texte = t;
    }

    fn poser_valeur(&mut self, valeur: String) {
        self.valeur = valeur;
        self.reecrire();
    }

    /// Valeurs du paramètre `TYPE`, en minuscules.
    fn types(&self) -> Vec<String> {
        self.params
            .iter()
            .filter(|(n, _)| n == "TYPE")
            .flat_map(|(_, v)| v.split(','))
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    fn param(&self, nom: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str())
    }

    /// Remplace les types que le formulaire connaît par celui du genre ; les
    /// autres (`pref`, `internet`, `voice`…) restent.
    fn poser_genre(&mut self, connus: &[&str], nouveaux: &[&str]) {
        let mut types: Vec<String> = self.types().into_iter().filter(|t| !connus.contains(&t.as_str())).collect();
        for n in nouveaux.iter().rev() {
            types.insert(0, n.to_string());
        }
        let place = self.params.iter().position(|(n, _)| n == "TYPE");
        self.params.retain(|(n, _)| n != "TYPE");
        if !types.is_empty() {
            let p = ("TYPE".to_string(), types.join(","));
            match place {
                Some(i) => self.params.insert(i.min(self.params.len()), p),
                None => self.params.push(p),
            }
        }
        self.reecrire();
    }
}

/// Une vCard : ses lignes entre `BEGIN:VCARD` et `END:VCARD`.
#[derive(Debug, Clone)]
struct Carte {
    lignes: Vec<Ligne>,
}

impl Carte {
    fn analyser(texte: &str) -> Option<Carte> {
        // Lignes repliées : une ligne qui commence par une espace ou une
        // tabulation continue la précédente.
        let mut logiques: Vec<String> = Vec::new();
        for brute in texte.split('\n') {
            let brute = brute.strip_suffix('\r').unwrap_or(brute);
            if let (Some(' ' | '\t'), Some(derniere)) = (brute.chars().next(), logiques.last_mut()) {
                derniere.push_str(&brute[1..]);
            } else if !brute.trim().is_empty() {
                logiques.push(brute.to_string());
            }
        }
        let mut lignes = Vec::new();
        let mut profondeur = 0;
        for texte in logiques {
            let Some(l) = Ligne::lire(&texte) else { continue };
            let carte = l.valeur.trim().eq_ignore_ascii_case("VCARD");
            if l.nom == "BEGIN" && carte {
                profondeur += 1;
                if profondeur == 1 {
                    continue;
                }
            } else if l.nom == "END" && carte {
                profondeur -= 1;
                if profondeur == 0 {
                    return Some(Carte { lignes });
                }
            }
            if profondeur >= 1 {
                lignes.push(l);
            }
        }
        None
    }

    fn ecrire(&self) -> String {
        let mut sortie = String::from("BEGIN:VCARD\r\n");
        for l in &self.lignes {
            plier(&mut sortie, &l.texte);
        }
        sortie.push_str("END:VCARD\r\n");
        sortie
    }

    fn textes(&self) -> Vec<String> {
        self.lignes.iter().map(|l| l.texte.clone()).collect()
    }

    /// La date de révision suit un changement, si la fiche en porte une.
    fn reviser(&mut self, avant: &[String], maintenant: DateTime<Utc>) {
        if self.premiere("REV").is_some() && self.textes() != avant {
            self.poser_unique("REV", Some(rev(maintenant)));
        }
    }

    fn version(&self) -> String {
        self.premiere("VERSION").map(|l| l.valeur.trim().to_string()).unwrap_or_else(|| "3.0".into())
    }

    fn premiere(&self, nom: &str) -> Option<&Ligne> {
        self.lignes.iter().find(|l| l.nom == nom)
    }

    fn position(&self, nom: &str) -> Option<usize> {
        self.lignes.iter().position(|l| l.nom == nom)
    }

    /// Où ajouter une ligne `nom` : après la dernière de même nom, sinon en fin.
    fn place_pour(&self, nom: &str) -> usize {
        self.lignes.iter().rposition(|l| l.nom == nom).map_or(self.lignes.len(), |i| i + 1)
    }

    /// Pose une propriété à valeur unique : la première ligne prend la
    /// nouvelle valeur, paramètres gardés ; vide, toutes les lignes partent.
    fn poser_unique(&mut self, nom: &str, valeur: Option<String>) {
        match (self.position(nom), valeur) {
            (Some(i), Some(v)) => self.lignes[i].poser_valeur(v),
            (None, Some(v)) => {
                let place = self.place_pour(nom);
                self.lignes.insert(place, Ligne::nouvelle(nom, Vec::new(), v));
            }
            (_, None) => self.lignes.retain(|l| l.nom != nom),
        }
    }

    /// Retire les lignes `indices`, et les étiquettes (`X-ABLabel`…) d'un
    /// groupe qui n'a plus rien d'autre.
    fn retirer(&mut self, indices: &HashSet<usize>) {
        let groupes: HashSet<String> =
            indices.iter().map(|&i| self.lignes[i].groupe.to_ascii_lowercase()).filter(|g| !g.is_empty()).collect();
        let mut i = 0;
        self.lignes.retain(|_| {
            let garde = !indices.contains(&i);
            i += 1;
            garde
        });
        for g in groupes {
            let du_groupe = |l: &Ligne| l.groupe.eq_ignore_ascii_case(&g);
            if self.lignes.iter().filter(|l| du_groupe(l)).all(|l| l.nom.starts_with("X-")) {
                self.lignes.retain(|l| !du_groupe(l));
            }
        }
    }
}

/// Une ligne logique pliée à 75 octets, sans couper un caractère (RFC 6350
/// § 3.2).
fn plier(sortie: &mut String, texte: &str) {
    let mut longueur = 0;
    for c in texte.chars() {
        let n = c.len_utf8();
        if longueur + n > 75 {
            sortie.push_str("\r\n ");
            longueur = 1;
        }
        sortie.push(c);
        longueur += n;
    }
    sortie.push_str("\r\n");
}

/// Valeur de type texte déséchappée : `\n`, `\,`, `\;` et `\\`.
fn texte(valeur: &str) -> String {
    let mut sortie = String::with_capacity(valeur.len());
    let mut echappe = false;
    for c in valeur.chars() {
        if echappe {
            sortie.push(if c == 'n' || c == 'N' { '\n' } else { c });
            echappe = false;
        } else if c == '\\' {
            echappe = true;
        } else {
            sortie.push(c);
        }
    }
    sortie.trim().to_string()
}

/// Composantes d'une valeur structurée (`N`, `ADR`, `ORG`), coupées aux
/// points-virgules non échappés, chacune déséchappée.
pub(crate) fn composantes(valeur: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut echappe = false;
    for c in valeur.chars() {
        if echappe {
            let p = parts.last_mut().unwrap();
            p.push('\\');
            p.push(c);
            echappe = false;
        } else if c == '\\' {
            echappe = true;
        } else if c == ';' {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(c);
        }
    }
    parts.iter().map(|p| texte(p)).collect()
}

/// Valeur structurée écrite : chaque composante échappée.
fn structuree(parties: &[String]) -> String {
    parties.iter().map(|p| echapper(p.trim())).collect::<Vec<_>>().join(";")
}

// ------------------------------------------------------------- genres

const TYPES_COURRIEL: &[&str] = &["work", "home", "other"];
const TYPES_TELEPHONE: &[&str] = &["work", "home", "cell", "mobile", "iphone", "fax", "other"];
const TYPES_ADRESSE: &[&str] = &["work", "home", "other"];

fn genre_courriel(types: &[String]) -> String {
    let a = |t: &str| types.iter().any(|x| x == t);
    if a("work") {
        "travail"
    } else if a("home") {
        "domicile"
    } else {
        "autre"
    }
    .into()
}

fn genre_telephone(types: &[String]) -> String {
    let a = |t: &str| types.iter().any(|x| x == t);
    if a("cell") || a("mobile") || a("iphone") {
        "portable"
    } else if a("fax") {
        "fax"
    } else if a("work") {
        "travail"
    } else if a("home") {
        "domicile"
    } else {
        "autre"
    }
    .into()
}

fn types_de(genre: &str) -> &'static [&'static str] {
    match genre {
        "travail" => &["work"],
        "domicile" => &["home"],
        "portable" => &["cell"],
        "fax" => &["fax"],
        _ => &[],
    }
}

// ------------------------------------------------------------- clés

/// Adresse de courriel comparable : sans `mailto:`, en minuscules.
pub fn cle_courriel(adresse: &str) -> String {
    let a = adresse.trim();
    a.strip_prefix("mailto:").unwrap_or(a).trim().to_lowercase()
}

/// Numéro comparable : ses chiffres, l'indicatif français ramené au zéro
/// (« +33 6 12… » et « 06 12… » sont le même numéro).
pub fn cle_telephone(numero: &str) -> String {
    let n = numero.trim();
    let n = n.strip_prefix("tel:").unwrap_or(n);
    let plus = n.trim_start().starts_with('+');
    let chiffres: String = n.chars().filter(char::is_ascii_digit).collect();
    // « +33 (0)6 12… » : le zéro entre parenthèses n'en fait pas deux.
    if plus && chiffres.starts_with("33") && chiffres.len() > 2 {
        format!("0{}", chiffres[2..].trim_start_matches('0'))
    } else if let Some(reste) = chiffres.strip_prefix("0033") {
        format!("0{}", reste.trim_start_matches('0'))
    } else {
        chiffres
    }
}

/// Nom comparable : mots en minuscules, dans l'ordre alphabétique
/// (« Dupont Jean » et « Jean DUPONT » se confondent).
fn cle_nom(nom: &str) -> String {
    let mut mots: Vec<String> = nom
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
        .filter(|m| !m.is_empty())
        .map(|m| m.to_lowercase())
        .collect();
    mots.sort();
    mots.join(" ")
}

fn cle(propriete: &str, valeur: &str) -> String {
    if propriete == "TEL" {
        cle_telephone(valeur)
    } else {
        cle_courriel(valeur)
    }
}

// ------------------------------------------------------------- lecture

/// La fiche d'une vCard, telle que le formulaire la montre ; `None` si le
/// texte n'en est pas une.
pub fn lire(vcard: &str) -> Option<Fiche> {
    let carte = Carte::analyser(vcard)?;
    let mut f = Fiche::default();
    for l in &carte.lignes {
        match l.nom.as_str() {
            "FN" if f.nom_affiche.is_empty() => f.nom_affiche = texte(&l.valeur),
            "N" if f.nom.is_empty() && f.prenom.is_empty() => {
                let c = composantes(&l.valeur);
                f.nom = c.first().cloned().unwrap_or_default();
                f.prenom = c.get(1).cloned().unwrap_or_default();
            }
            "ORG" if f.organisation.is_empty() => f.organisation = composantes(&l.valeur).into_iter().next().unwrap_or_default(),
            "TITLE" if f.fonction.is_empty() => f.fonction = texte(&l.valeur),
            "NOTE" if f.note.is_empty() => f.note = texte(&l.valeur),
            "EMAIL" => {
                let v = texte(&l.valeur);
                let v = v.strip_prefix("mailto:").unwrap_or(&v).to_string();
                if !v.is_empty() {
                    let cle = Some(f.courriels.len());
                    f.courriels.push(Coordonnee { genre: genre_courriel(&l.types()), valeur: v, cle });
                }
            }
            "TEL" => {
                let v = texte(&l.valeur);
                let v = v.strip_prefix("tel:").unwrap_or(&v).to_string();
                if !v.is_empty() {
                    let cle = Some(f.telephones.len());
                    f.telephones.push(Coordonnee { genre: genre_telephone(&l.types()), valeur: v, cle });
                }
            }
            "ADR" if f.adresse.vide() => {
                let c = composantes(&l.valeur);
                let part = |i: usize| c.get(i).cloned().unwrap_or_default();
                f.adresse = Adresse {
                    genre: genre_courriel(&l.types()),
                    rue: part(2),
                    ville: part(3),
                    code_postal: part(5),
                    pays: part(6),
                };
            }
            _ => {}
        }
    }
    Some(f)
}

/// Le nom sous lequel la fiche se montre : le nom affiché, sinon prénom et
/// nom, l'organisation, la première adresse.
pub fn nom_montre(f: &Fiche) -> String {
    let compose = format!("{} {}", f.prenom.trim(), f.nom.trim()).trim().to_string();
    [f.nom_affiche.trim().to_string(), compose, f.organisation.trim().to_string()]
        .into_iter()
        .chain(f.courriels.iter().map(|c| c.valeur.trim().to_string()))
        .chain(f.telephones.iter().map(|c| c.valeur.trim().to_string()))
        .find(|t| !t.is_empty())
        .unwrap_or_default()
}

/// Une fiche sans rien : ni nom, ni organisation, ni fonction, ni
/// coordonnée, ni adresse, ni note.
pub fn est_vide(f: &Fiche) -> bool {
    [&f.nom_affiche, &f.prenom, &f.nom, &f.organisation, &f.fonction, &f.note].iter().all(|t| t.trim().is_empty())
        && f.courriels.iter().all(|c| c.valeur.trim().is_empty())
        && f.telephones.iter().all(|c| c.valeur.trim().is_empty())
        && f.adresse.vide()
}

/// Une fiche qui a un nom mais aucun moyen de joindre la personne : ni
/// adresse de courriel, ni téléphone, ni adresse postale.
pub fn sans_coordonnee(f: &Fiche) -> bool {
    !est_vide(f)
        && f.courriels.iter().all(|c| c.valeur.trim().is_empty())
        && f.telephones.iter().all(|c| c.valeur.trim().is_empty())
        && f.adresse.vide()
}

/// Ce que porte une fiche : de quoi garder la plus complète d'un doublon.
pub fn completude(f: &Fiche) -> usize {
    [&f.nom_affiche, &f.prenom, &f.nom, &f.organisation, &f.fonction, &f.note].iter().filter(|t| !t.trim().is_empty()).count()
        + 2 * f.courriels.len()
        + 2 * f.telephones.len()
        + if f.adresse.vide() { 0 } else { 2 }
}

/// La fiche d'un correspondant à ajouter : le nom de l'en-tête, coupé en
/// prénom et nom (« DUPONT Jean » : le nom en capitales vient d'abord), et
/// son adresse.
pub fn depuis_adresse(nom: &str, adresse: &str) -> Fiche {
    let nom = nom.trim().trim_matches('"').trim();
    let nom = if nom.eq_ignore_ascii_case(adresse.trim()) { "" } else { nom };
    let mots: Vec<&str> = nom.split_whitespace().collect();
    let capitales = |m: &str| m.chars().filter(|c| c.is_alphabetic()).count() > 1 && !m.chars().any(char::is_lowercase);
    let (prenom, famille) = match mots.as_slice() {
        [] | [_] => (String::new(), nom.to_string()),
        [premier, reste @ ..] if capitales(premier) && !reste.iter().all(|m| capitales(m)) => (reste.join(" "), premier.to_string()),
        [premier, reste @ ..] => (premier.to_string(), reste.join(" ")),
    };
    Fiche {
        nom_affiche: nom.to_string(),
        prenom,
        nom: famille,
        courriels: vec![Coordonnee { genre: "travail".into(), valeur: adresse.trim().to_string(), cle: None }],
        ..Default::default()
    }
}

// ------------------------------------------------------------- écriture

/// Une nouvelle fiche, en vCard 3.0 ; rend une erreur pour un formulaire
/// sans rien.
pub fn creer(f: &Fiche, uid: &str, maintenant: DateTime<Utc>) -> Result<String, String> {
    let mut propre = f.clone();
    for c in propre.courriels.iter_mut().chain(propre.telephones.iter_mut()) {
        c.cle = None;
    }
    if est_vide(&propre) {
        return Err("fiche vide : donnez au moins un nom ou une coordonnée".into());
    }
    verifier(&propre)?;
    let squelette = format!(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nPRODID:-//MMEDIA//MMail {}//FR\r\nUID:{}\r\nEND:VCARD\r\n",
        env!("CARGO_PKG_VERSION"),
        echapper(uid)
    );
    let mut carte = Carte::analyser(&squelette).ok_or("squelette illisible")?;
    appliquer(&mut carte, &Fiche::default(), &propre);
    carte.poser_unique("REV", Some(rev(maintenant)));
    Ok(carte.ecrire())
}

/// Applique à `vcard` ce qui distingue `saisie` d'`origine`, et rien d'autre.
pub fn modifier(vcard: &str, origine: &Fiche, saisie: &Fiche, maintenant: DateTime<Utc>) -> Result<String, String> {
    let mut carte = Carte::analyser(vcard).ok_or("fiche illisible sur le serveur")?;
    verifier(saisie)?;
    if est_vide(saisie) {
        return Err("fiche vide : donnez au moins un nom ou une coordonnée, ou supprimez-la".into());
    }
    let avant = carte.textes();
    appliquer(&mut carte, origine, saisie);
    carte.reviser(&avant, maintenant);
    Ok(carte.ecrire())
}

fn rev(maintenant: DateTime<Utc>) -> String {
    maintenant.format("%Y%m%dT%H%M%SZ").to_string()
}

/// Refuse une adresse de courriel manifestement fausse.
fn verifier(f: &Fiche) -> Result<(), String> {
    for c in &f.courriels {
        let v = c.valeur.trim();
        if !v.is_empty() && (!v.contains('@') || v.contains(char::is_whitespace) || v.starts_with('@') || v.ends_with('@')) {
            return Err(format!("adresse de courriel invalide : {v}"));
        }
    }
    Ok(())
}

fn appliquer(carte: &mut Carte, origine: &Fiche, saisie: &Fiche) {
    let v4 = carte.version() == "4.0";
    // Nom affiché : obligatoire ; à défaut de saisie, il suit le nom.
    let (fn_avant, fn_apres) = (nom_montre(origine), nom_montre(saisie));
    if fn_avant != fn_apres || carte.premiere("FN").is_none() {
        carte.poser_unique("FN", Some(echapper(&fn_apres)));
    }
    if (origine.nom.trim(), origine.prenom.trim()) != (saisie.nom.trim(), saisie.prenom.trim()) || (!v4 && carte.premiere("N").is_none()) {
        let mut parts = carte.premiere("N").map(|l| composantes(&l.valeur)).unwrap_or_default();
        parts.resize(parts.len().max(5), String::new());
        parts[0] = saisie.nom.trim().to_string();
        parts[1] = saisie.prenom.trim().to_string();
        let vide = parts.iter().all(|p| p.is_empty());
        carte.poser_unique("N", if vide && v4 { None } else { Some(structuree(&parts)) });
    }
    if origine.organisation.trim() != saisie.organisation.trim() {
        let mut parts = carte.premiere("ORG").map(|l| composantes(&l.valeur)).unwrap_or_default();
        if parts.is_empty() {
            parts.push(String::new());
        }
        parts[0] = saisie.organisation.trim().to_string();
        let vide = parts.iter().all(|p| p.is_empty());
        carte.poser_unique("ORG", if vide { None } else { Some(structuree(&parts)) });
    }
    for (nom, avant, apres) in [("TITLE", &origine.fonction, &saisie.fonction), ("NOTE", &origine.note, &saisie.note)] {
        if avant.trim() != apres.trim() {
            carte.poser_unique(nom, Some(apres.trim()).filter(|t| !t.is_empty()).map(echapper));
        }
    }
    appliquer_adresse(carte, &origine.adresse, &saisie.adresse);
    appliquer_liste(carte, "EMAIL", &origine.courriels, &saisie.courriels);
    appliquer_liste(carte, "TEL", &origine.telephones, &saisie.telephones);
}

fn appliquer_adresse(carte: &mut Carte, avant: &Adresse, apres: &Adresse) {
    let norme = |a: &Adresse| {
        (a.rue.trim().to_string(), a.code_postal.trim().to_string(), a.ville.trim().to_string(), a.pays.trim().to_string())
    };
    let genre_change = !apres.vide() && avant.genre != apres.genre;
    if norme(avant) == norme(apres) && !genre_change {
        return;
    }
    match carte.position("ADR") {
        Some(i) if apres.vide() => carte.retirer(&HashSet::from([i])),
        Some(i) => {
            let mut parts = composantes(&carte.lignes[i].valeur);
            parts.resize(parts.len().max(7), String::new());
            parts[2] = apres.rue.trim().to_string();
            parts[3] = apres.ville.trim().to_string();
            parts[5] = apres.code_postal.trim().to_string();
            parts[6] = apres.pays.trim().to_string();
            let ligne = &mut carte.lignes[i];
            ligne.poser_valeur(structuree(&parts));
            if genre_change {
                ligne.poser_genre(TYPES_ADRESSE, types_de(&apres.genre));
            }
        }
        None if apres.vide() => {}
        None => {
            let parts =
                ["", "", apres.rue.trim(), apres.ville.trim(), "", apres.code_postal.trim(), apres.pays.trim()].map(String::from);
            let types = types_de(&apres.genre);
            let params = if types.is_empty() { Vec::new() } else { vec![("TYPE".to_string(), types.join(","))] };
            let place = carte.place_pour("ADR");
            carte.lignes.insert(place, Ligne::nouvelle("ADR", params, structuree(&parts)));
        }
    }
}

/// Adresses ou numéros : chaque coordonnée de l'origine retrouvée dans la
/// vCard par sa valeur ; retirée, changée ou gardée selon la saisie, qui
/// la désigne par sa clé. Les nouvelles s'ajoutent après les autres.
fn appliquer_liste(carte: &mut Carte, propriete: &str, origine: &[Coordonnee], saisie: &[Coordonnee]) {
    let connus = if propriete == "TEL" { TYPES_TELEPHONE } else { TYPES_COURRIEL };
    let mut prises: HashSet<usize> = HashSet::new();
    let mut a_retirer: HashSet<usize> = HashSet::new();
    let mut a_ajouter: Vec<&Coordonnee> = Vec::new();
    let trouver = |carte: &Carte, prises: &HashSet<usize>, valeur: &str| {
        let c = cle(propriete, valeur);
        carte.lignes.iter().enumerate().position(|(i, l)| {
            l.nom == propriete && !prises.contains(&i) && cle(propriete, &texte(&l.valeur)) == c
        })
    };
    for (k, o) in origine.iter().enumerate() {
        let s = saisie.iter().find(|s| s.cle == Some(k));
        let ligne = trouver(carte, &prises, &o.valeur);
        if let Some(i) = ligne {
            prises.insert(i);
        }
        match (s, ligne) {
            (None, Some(i)) => {
                a_retirer.insert(i);
            }
            (Some(s), Some(i)) if s.valeur.trim().is_empty() => {
                a_retirer.insert(i);
            }
            (Some(s), Some(i)) => {
                if cle(propriete, &s.valeur) != cle(propriete, &o.valeur) || s.valeur.trim() != o.valeur.trim() {
                    let ligne = &mut carte.lignes[i];
                    let valeur = if ligne.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("uri")) {
                        let prefixe = if propriete == "TEL" { "tel:" } else { "mailto:" };
                        format!("{prefixe}{}", s.valeur.trim().replace(' ', ""))
                    } else {
                        echapper(s.valeur.trim())
                    };
                    ligne.poser_valeur(valeur);
                }
                if s.genre != o.genre {
                    carte.lignes[i].poser_genre(connus, types_de(&s.genre));
                }
            }
            // Retirée du serveur entre-temps : elle revient seulement si la
            // saisie l'a changée.
            (Some(s), None) if !s.valeur.trim().is_empty() && (s.valeur.trim() != o.valeur.trim() || s.genre != o.genre) => {
                a_ajouter.push(s)
            }
            _ => {}
        }
    }
    for s in saisie {
        if s.valeur.trim().is_empty() || s.cle.is_some_and(|k| k < origine.len()) {
            continue;
        }
        if trouver(carte, &HashSet::new(), &s.valeur).is_none_or(|i| a_retirer.contains(&i)) {
            a_ajouter.push(s);
        }
    }
    carte.retirer(&a_retirer);
    let mut deja: HashSet<String> = HashSet::new();
    for s in a_ajouter {
        if !deja.insert(cle(propriete, &s.valeur)) {
            continue;
        }
        let types = types_de(&s.genre);
        let params = if types.is_empty() { Vec::new() } else { vec![("TYPE".to_string(), types.join(","))] };
        let place = carte.place_pour(propriete);
        carte.lignes.insert(place, Ligne::nouvelle(propriete, params, echapper(s.valeur.trim())));
    }
}

// ------------------------------------------------------------- conflit

/// Après un conflit : la fiche du serveur (`courant`), à laquelle s'ajoute ce
/// que l'utilisateur avait changé depuis `origine`. Rend la fiche à montrer
/// — elle prend l'ETag et les clés de `courant` — et le nom des rubriques
/// changées des deux côtés, où la saisie l'emporte.
pub fn reprendre(courant: &Fiche, origine: &Fiche, saisie: &Fiche) -> (Fiche, Vec<String>) {
    let mut f = courant.clone();
    let mut conflits = Vec::new();
    let mut texte_fusion = |libelle: &str, c: &String, o: &String, s: &String, sortie: &mut String| {
        if s.trim() != o.trim() {
            if c.trim() != o.trim() && c.trim() != s.trim() {
                conflits.push(libelle.to_string());
            }
            *sortie = s.clone();
        }
    };
    texte_fusion("nom affiché", &courant.nom_affiche, &origine.nom_affiche, &saisie.nom_affiche, &mut f.nom_affiche);
    texte_fusion("prénom", &courant.prenom, &origine.prenom, &saisie.prenom, &mut f.prenom);
    texte_fusion("nom", &courant.nom, &origine.nom, &saisie.nom, &mut f.nom);
    texte_fusion("organisation", &courant.organisation, &origine.organisation, &saisie.organisation, &mut f.organisation);
    texte_fusion("fonction", &courant.fonction, &origine.fonction, &saisie.fonction, &mut f.fonction);
    texte_fusion("note", &courant.note, &origine.note, &saisie.note, &mut f.note);
    if saisie.adresse != origine.adresse {
        if courant.adresse != origine.adresse && courant.adresse != saisie.adresse {
            conflits.push("adresse postale".into());
        }
        f.adresse = saisie.adresse.clone();
    }
    for (libelle, propriete, c, o, s, sortie) in [
        ("courriels", "EMAIL", &courant.courriels, &origine.courriels, &saisie.courriels, &mut f.courriels),
        ("téléphones", "TEL", &courant.telephones, &origine.telephones, &saisie.telephones, &mut f.telephones),
    ] {
        let resume = |l: &[Coordonnee]| l.iter().map(|x| (x.genre.clone(), cle(propriete, &x.valeur))).collect::<Vec<_>>();
        if resume(s) == resume(o) {
            continue;
        }
        if resume(c) != resume(o) {
            conflits.push(libelle.into());
        }
        *sortie = fusion_liste(propriete, c, o, s);
    }
    (f, conflits)
}

/// Les coordonnées du serveur, plus ce que la saisie a changé depuis
/// l'origine ; clés renvoyant aux coordonnées du serveur.
fn fusion_liste(propriete: &str, courant: &[Coordonnee], origine: &[Coordonnee], saisie: &[Coordonnee]) -> Vec<Coordonnee> {
    let mut sortie: Vec<Coordonnee> = courant.to_vec();
    let mut retirees: HashSet<String> = HashSet::new();
    for (k, o) in origine.iter().enumerate() {
        let s = saisie.iter().find(|s| s.cle == Some(k));
        let place = sortie.iter().position(|x| cle(propriete, &x.valeur) == cle(propriete, &o.valeur));
        match (s, place) {
            (None, Some(i)) => {
                retirees.insert(cle(propriete, &sortie[i].valeur));
            }
            (Some(s), Some(i)) => {
                sortie[i].valeur = s.valeur.clone();
                sortie[i].genre = s.genre.clone();
            }
            (Some(s), None) if s.valeur.trim() != o.valeur.trim() || s.genre != o.genre => {
                sortie.push(Coordonnee { cle: None, ..s.clone() });
            }
            _ => {}
        }
    }
    for s in saisie.iter().filter(|s| s.cle.is_none_or(|k| k >= origine.len())) {
        if !sortie.iter().any(|x| cle(propriete, &x.valeur) == cle(propriete, &s.valeur)) {
            sortie.push(Coordonnee { cle: None, ..s.clone() });
        }
    }
    sortie.retain(|x| !retirees.contains(&cle(propriete, &x.valeur)) && !x.valeur.trim().is_empty());
    sortie
}

// ------------------------------------------------------------- doublons

/// Les doublons d'un carnet, par groupes d'identifiants, la fiche à garder
/// en tête : même adresse de courriel ; ou même nom et, au choix, un même
/// numéro, ou ni l'un ni l'autre d'adresse de courriel. Une fiche vide n'est
/// d'aucun groupe : elle se retire, elle ne se fusionne pas.
pub fn doublons(fiches: &[(i64, Fiche)]) -> Vec<Vec<i64>> {
    let n = fiches.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn racine(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut unir = |a: usize, b: usize| {
        let (ra, rb) = (racine(&mut parent, a), racine(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    let mut par_courriel: HashMap<String, usize> = HashMap::new();
    let mut par_nom: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (_, f)) in fiches.iter().enumerate() {
        if est_vide(f) {
            continue;
        }
        for c in &f.courriels {
            let k = cle_courriel(&c.valeur);
            if k.is_empty() {
                continue;
            }
            match par_courriel.get(&k) {
                Some(&j) => unir(i, j),
                None => {
                    par_courriel.insert(k, i);
                }
            }
        }
        let nom = cle_nom(&nom_montre(f));
        if !nom.is_empty() {
            par_nom.entry(nom).or_default().push(i);
        }
    }
    for indices in par_nom.values() {
        for (a, &i) in indices.iter().enumerate() {
            for &j in &indices[a + 1..] {
                let (fi, fj) = (&fiches[i].1, &fiches[j].1);
                let numeros = |f: &Fiche| f.telephones.iter().map(|t| cle_telephone(&t.valeur)).filter(|t| !t.is_empty()).collect::<HashSet<_>>();
                let meme_numero = !numeros(fi).is_disjoint(&numeros(fj));
                let sans_courriel = fi.courriels.is_empty() && fj.courriels.is_empty();
                if meme_numero || sans_courriel {
                    unir(i, j);
                }
            }
        }
    }
    let mut groupes: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        if !est_vide(&fiches[i].1) {
            let r = racine(&mut parent, i);
            groupes.entry(r).or_default().push(i);
        }
    }
    let mut sortie: Vec<Vec<i64>> = groupes
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            // La plus complète d'abord ; à égalité, la plus ancienne dans
            // l'index.
            g.sort_by(|&a, &b| completude(&fiches[b].1).cmp(&completude(&fiches[a].1)).then(fiches[a].0.cmp(&fiches[b].0)));
            g.into_iter().map(|i| fiches[i].0).collect()
        })
        .collect();
    sortie.sort();
    sortie
}

/// Propriétés reprises telles quelles d'une fiche fusionnée quand la fiche
/// gardée n'en a pas.
const REPRISES: &[&str] = &["PHOTO", "BDAY", "NICKNAME", "URL"];

/// Fusionne des doublons : la première vCard est gardée et reçoit des autres
/// les adresses et numéros qui lui manquent, les rubriques vides qu'elles
/// remplissent, leurs notes, et leur photo s'il lui en manque une. Rend la
/// nouvelle vCard de la fiche gardée.
pub fn fusionner(vcards: &[&str], maintenant: DateTime<Utc>) -> Result<String, String> {
    let fiches: Vec<Fiche> = vcards.iter().map(|v| lire(v).ok_or("fiche illisible")).collect::<Result<_, _>>()?;
    let (garde, autres) = fiches.split_first().ok_or("rien à fusionner")?;
    let mut f = garde.clone();
    for a in autres {
        for (champ, valeur) in [
            (&mut f.nom_affiche, &a.nom_affiche),
            (&mut f.organisation, &a.organisation),
            (&mut f.fonction, &a.fonction),
        ] {
            if champ.trim().is_empty() {
                *champ = valeur.clone();
            }
        }
        if f.nom.trim().is_empty() && f.prenom.trim().is_empty() {
            f.nom = a.nom.clone();
            f.prenom = a.prenom.clone();
        }
        if f.adresse.vide() {
            f.adresse = a.adresse.clone();
        }
        if !a.note.trim().is_empty() && !f.note.contains(a.note.trim()) {
            f.note = [f.note.trim(), a.note.trim()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
        }
        for (liste, ajouts, propriete) in [(&mut f.courriels, &a.courriels, "EMAIL"), (&mut f.telephones, &a.telephones, "TEL")] {
            for c in ajouts {
                if !liste.iter().any(|x| cle(propriete, &x.valeur) == cle(propriete, &c.valeur)) {
                    liste.push(Coordonnee { cle: None, ..c.clone() });
                }
            }
        }
    }
    let mut carte = Carte::analyser(vcards[0]).ok_or("fiche illisible")?;
    let avant = carte.textes();
    let version = carte.version();
    appliquer(&mut carte, garde, &f);
    for nom in REPRISES {
        if carte.premiere(nom).is_some() {
            continue;
        }
        let reprise = vcards[1..]
            .iter()
            .filter_map(|v| Carte::analyser(v))
            .filter(|c| c.version() == version)
            .find_map(|c| c.premiere(nom).cloned());
        if let Some(mut l) = reprise {
            l.groupe.clear();
            l.reecrire();
            let place = carte.place_pour(nom);
            carte.lignes.insert(place, l);
        }
    }
    carte.reviser(&avant, maintenant);
    Ok(carte.ecrire())
}

/// Une ligne de résumé d'une fiche, pour les listes de confirmation : nom,
/// première adresse, premier numéro.
pub fn resume(f: &Fiche) -> String {
    let mut parts = vec![nom_montre(f)];
    if let Some(c) = f.courriels.first() {
        if !parts.contains(&c.valeur) {
            parts.push(c.valeur.clone());
        }
    }
    if let Some(t) = f.telephones.first() {
        if !parts.contains(&t.valeur) {
            parts.push(t.valeur.clone());
        }
    }
    parts.retain(|p| !p.trim().is_empty());
    if parts.is_empty() {
        "(fiche vide)".into()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests;
