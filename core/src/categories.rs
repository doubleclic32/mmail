// SPDX-License-Identifier: GPL-3.0-or-later
//! Catégories de couleur (0.6.15, « Catégoriser » d'Outlook ; demande de Manu
//! du 22/10/2026, pour ne plus passer par le webmail).
//!
//! Une catégorie posée sur un message est un **mot-clé IMAP** : le serveur le
//! garde, tous les postes et le webmail le voient, et sur une boîte partagée
//! (Mailcow, sans index privé) tous ses utilisateurs aussi. Les cinq
//! étiquettes du webmail SOGo (`$label1` à `$label5`, aussi celles de
//! Thunderbird) sont toujours là, sous leur nom et leur couleur de SOGo.
//!
//! Le **catalogue** — nom et couleur des catégories créées, et ce qui change
//! celles d'origine — se range sur la boîte de réception de la boîte, en
//! métadonnée (`METADATA`, RFC 5464) : une boîte partagée a ainsi le même
//! catalogue pour tous ceux qui l'ouvrent. Sans `METADATA`, il reste sur le
//! poste.
//!
//! Le stockage maildir de Mailcow ne garde dans ses fichiers que 26 mots-clés
//! par dossier, attribués au premier usage et jamais rendus ; au-delà, un
//! mot-clé ne vit que dans l'index du serveur et se perd s'il est reconstruit
//! (constaté sur Dovecot 2.3, 22/10/2026). D'où le plafond des catégories
//! créées, qui laisse la place aux mots-clés d'usage (`$Forwarded`…).

use serde::{Deserialize, Serialize};

/// Entrée de métadonnée du catalogue, sur la boîte de réception de la boîte.
pub const ENTREE: &str = "/shared/vendor/mmedia/mmail/categories";

/// Catégories créées au plus par catalogue, en plus des cinq d'origine.
pub const MAX_CREEES: usize = 15;

/// Couleur d'un mot-clé que le catalogue ne connaît pas.
pub const COULEUR_INCONNUE: &str = "#8A8F98";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Categorie {
    #[serde(rename = "motCle")]
    pub mot_cle: String,
    #[serde(default)]
    pub nom: String,
    #[serde(default)]
    pub couleur: String,
}

/// Les étiquettes d'origine du webmail SOGo, avec ses noms français et ses
/// couleurs (relevés dans les préférences des utilisateurs de Mailcow).
pub fn d_origine() -> Vec<Categorie> {
    [
        ("$label1", "Important", "#FF0000"),
        ("$label2", "Travail", "#FF9900"),
        ("$label3", "Personnel", "#009900"),
        ("$label4", "À faire", "#3333FF"),
        ("$label5", "Peut attendre", "#993399"),
    ]
    .iter()
    .map(|(m, n, c)| Categorie { mot_cle: m.to_string(), nom: n.to_string(), couleur: c.to_string() })
    .collect()
}

/// Vrai pour une catégorie d'origine, qui ne se retire pas.
pub fn est_d_origine(mot_cle: &str) -> bool {
    d_origine().iter().any(|c| c.mot_cle.eq_ignore_ascii_case(mot_cle))
}

/// Ce qui se range sur le serveur : les catégories créées et les catégories
/// d'origine renommées ou recolorées, dans l'ordre de création.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Catalogue {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub categories: Vec<Categorie>,
}

fn version() -> u32 {
    1
}

/// Ce qu'on change d'un catalogue : poser (créer ou modifier, d'après le
/// mot-clé) ou retirer une catégorie. Le fil du compte relit le catalogue du
/// serveur avant d'y appliquer le changement : deux postes qui créent chacun
/// une catégorie gardent les deux.
#[derive(Clone, Debug, PartialEq)]
pub enum Changement {
    Poser(Categorie),
    Retirer(String),
}

impl Catalogue {
    /// Lit un catalogue rangé ; un texte illisible donne un catalogue vide,
    /// et les entrées invalides sont écartées.
    pub fn lire(texte: &str) -> Catalogue {
        let mut catalogue: Catalogue = serde_json::from_str(texte).unwrap_or_default();
        catalogue.categories.retain(|c| crate::protocole::mot_cle_valide(&c.mot_cle));
        for c in &mut catalogue.categories {
            if !couleur_valide(&c.couleur) {
                c.couleur = COULEUR_INCONNUE.to_string();
            }
        }
        catalogue
    }

    /// JSON en ASCII seul (« \u00e0 » pour « à ») : il s'écrit sur tout
    /// serveur, même sans littéraux.
    pub fn ecrire(&self) -> String {
        let mut sortie = String::new();
        for c in serde_json::to_string(self).unwrap_or_default().chars() {
            if c.is_ascii() {
                sortie.push(c);
            } else {
                let mut unites = [0u16; 2];
                for u in c.encode_utf16(&mut unites) {
                    sortie.push_str(&format!("\\u{:04x}", u));
                }
            }
        }
        sortie
    }

    /// Catégories à proposer : celles d'origine, changées par le catalogue,
    /// puis celles créées.
    pub fn liste(&self) -> Vec<Categorie> {
        let mut liste = d_origine();
        for c in &mut liste {
            if let Some(changee) = self.categories.iter().find(|x| x.mot_cle.eq_ignore_ascii_case(&c.mot_cle)) {
                if !changee.nom.trim().is_empty() {
                    c.nom = changee.nom.clone();
                }
                c.couleur = changee.couleur.clone();
            }
        }
        liste.extend(self.categories.iter().filter(|c| !est_d_origine(&c.mot_cle)).cloned());
        liste
    }

    /// Nombre de catégories créées, celles d'origine non comprises.
    pub fn creees(&self) -> usize {
        self.categories.iter().filter(|c| !est_d_origine(&c.mot_cle)).count()
    }

    /// Applique un changement. Refusé : un nom vide, une couleur invalide, le
    /// plafond atteint, le retrait d'une catégorie d'origine.
    pub fn appliquer(&mut self, changement: &Changement) -> Result<(), String> {
        match changement {
            Changement::Poser(c) => {
                if c.nom.trim().is_empty() {
                    return Err("une catégorie doit avoir un nom".into());
                }
                if !couleur_valide(&c.couleur) || !crate::protocole::mot_cle_valide(&c.mot_cle) {
                    return Err("catégorie invalide".into());
                }
                let c = Categorie { nom: c.nom.trim().to_string(), ..c.clone() };
                match self.categories.iter_mut().find(|x| x.mot_cle.eq_ignore_ascii_case(&c.mot_cle)) {
                    Some(x) => *x = c,
                    None => {
                        if !est_d_origine(&c.mot_cle) && self.creees() >= MAX_CREEES {
                            return Err(format!("{MAX_CREEES} catégories au plus, en plus des cinq d'origine"));
                        }
                        self.categories.push(c);
                    }
                }
                Ok(())
            }
            Changement::Retirer(mot_cle) => {
                if est_d_origine(mot_cle) {
                    // Une catégorie d'origine reprend son nom et sa couleur.
                    self.categories.retain(|x| !x.mot_cle.eq_ignore_ascii_case(mot_cle));
                    return Ok(());
                }
                self.categories.retain(|x| !x.mot_cle.eq_ignore_ascii_case(mot_cle));
                Ok(())
            }
        }
    }
}

/// « #RRGGBB ».
pub fn couleur_valide(couleur: &str) -> bool {
    couleur.len() == 7 && couleur.starts_with('#') && couleur[1..].bytes().all(|o| o.is_ascii_hexdigit())
}

/// Mot-clé d'une catégorie créée, tiré de son nom : minuscules sans accents,
/// « _ » entre les mots, ce qui n'est ni lettre ni chiffre retiré — comme
/// Thunderbird. « Devis à faire » → `devis_a_faire`. Un mot-clé déjà pris
/// (sans égard à la casse) reçoit un numéro : `devis_2`. Renommer une
/// catégorie ne change pas son mot-clé : les messages la gardent.
pub fn mot_cle_pour(nom: &str, pris: &[String]) -> String {
    let mut base = String::new();
    for c in nom.trim().chars().flat_map(sans_accent) {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_' || c == '\'') && !base.ends_with('_') && !base.is_empty() {
            base.push('_');
        }
    }
    let mut base = base.trim_end_matches('_').to_string();
    base.truncate(40);
    // Ni vide, ni confondu avec un mot-clé d'usage : un nom tout en signes, ou
    // « $… », donne « categorie ».
    if base.is_empty() || crate::protocole::mots_cles(&[base.clone()]).is_empty() {
        base = "categorie".to_string();
    }
    let libre = |m: &str| !pris.iter().any(|p| p.eq_ignore_ascii_case(m)) && !est_d_origine(m);
    if libre(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|m| libre(m)).unwrap_or(base)
}

/// Lettre sans son accent, pour les noms en français ; « œ » et « æ » en deux.
fn sans_accent(c: char) -> Vec<char> {
    let simple = match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
        'À' | 'Â' | 'Ä' | 'Á' | 'Ã' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'í' | 'ì' | 'Î' | 'Ï' | 'Í' | 'Ì' => 'i',
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' | 'Ô' | 'Ö' | 'Ó' | 'Ò' | 'Õ' => 'o',
        'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' | 'Ú' => 'u',
        'ÿ' | 'Ÿ' | 'ý' | 'Ý' => 'y',
        'ñ' | 'Ñ' => 'n',
        'œ' | 'Œ' => return vec!['o', 'e'],
        'æ' | 'Æ' => return vec!['a', 'e'],
        c => c,
    };
    vec![simple]
}

/// Mots-clés d'une liste séparée par des espaces (colonne `mots_cles` de
/// l'index).
pub fn decouper(mots_cles: &str) -> Vec<String> {
    mots_cles.split_whitespace().map(str::to_string).collect()
}

/// Ajoute ou retire un mot-clé d'une liste séparée par des espaces, triée
/// comme `protocole::mots_cles`.
pub fn modifier(mots_cles: &str, mot_cle: &str, present: bool) -> String {
    let mut liste: Vec<String> = decouper(mots_cles).into_iter().filter(|m| !m.eq_ignore_ascii_case(mot_cle)).collect();
    if present {
        liste.push(mot_cle.to_string());
    }
    crate::protocole::mots_cles(&liste)
}

/// Sépare d'un texte cherché les critères de catégorie (0.6.15) :
/// « catégorie:Devis », « categorie:"Devis à faire" » — sans égard à la casse
/// ni à l'accent du mot « catégorie ». Rend le reste du texte et les noms.
pub fn separer_criteres(texte: &str) -> (String, Vec<String>) {
    let mut reste = Vec::new();
    let mut noms = Vec::new();
    let mut i = 0;
    let caracteres: Vec<char> = texte.chars().collect();
    while i < caracteres.len() {
        if caracteres[i].is_whitespace() {
            i += 1;
            continue;
        }
        let debut = i;
        while i < caracteres.len() && !caracteres[i].is_whitespace() && caracteres[i] != ':' {
            i += 1;
        }
        let mot: String = caracteres[debut..i].iter().collect::<String>().to_lowercase();
        if i < caracteres.len() && caracteres[i] == ':' && (mot == "catégorie" || mot == "categorie") {
            i += 1;
            let mut nom = String::new();
            if i < caracteres.len() && caracteres[i] == '"' {
                i += 1;
                while i < caracteres.len() && caracteres[i] != '"' {
                    nom.push(caracteres[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < caracteres.len() && !caracteres[i].is_whitespace() {
                    nom.push(caracteres[i]);
                    i += 1;
                }
            }
            if !nom.trim().is_empty() {
                noms.push(nom.trim().to_string());
            }
            continue;
        }
        while i < caracteres.len() && !caracteres[i].is_whitespace() {
            i += 1;
        }
        reste.push(caracteres[debut..i].iter().collect::<String>());
    }
    (reste.join(" "), noms)
}

/// Mots-clés désignés par un nom de catégorie, dans les catalogues donnés
/// (valeurs rangées) et ceux d'origine : par leur nom, sans égard à la casse,
/// ou par le mot-clé lui-même. Un nom inconnu ne désigne rien.
pub fn mots_cles_nommes(nom: &str, catalogues: &[String]) -> Vec<String> {
    let cherche = nom.trim().to_lowercase();
    let mut trouves: Vec<String> = Vec::new();
    let mut toutes = d_origine();
    for valeur in catalogues {
        toutes.extend(Catalogue::lire(valeur).liste());
    }
    for c in toutes {
        if (c.nom.to_lowercase() == cherche || c.mot_cle.to_lowercase() == cherche)
            && !trouves.iter().any(|t| t.eq_ignore_ascii_case(&c.mot_cle))
        {
            trouves.push(c.mot_cle);
        }
    }
    trouves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(m: &str, n: &str, c: &str) -> Categorie {
        Categorie { mot_cle: m.into(), nom: n.into(), couleur: c.into() }
    }

    #[test]
    fn origine_puis_creees() {
        let mut catalogue = Catalogue::default();
        assert_eq!(catalogue.liste().len(), 5);
        assert_eq!(catalogue.liste()[4].nom, "Peut attendre");
        catalogue.appliquer(&Changement::Poser(cat("devis", "Devis", "#0B8043"))).unwrap();
        catalogue.appliquer(&Changement::Poser(cat("$label1", "Urgent", "#D50000"))).unwrap();
        let liste = catalogue.liste();
        assert_eq!(liste.len(), 6);
        assert_eq!(liste[0], cat("$label1", "Urgent", "#D50000"));
        assert_eq!(liste[5], cat("devis", "Devis", "#0B8043"));
        assert_eq!(catalogue.creees(), 1);
        // Retirer une catégorie d'origine lui rend son nom.
        catalogue.appliquer(&Changement::Retirer("$label1".into())).unwrap();
        assert_eq!(catalogue.liste()[0].nom, "Important");
        catalogue.appliquer(&Changement::Retirer("DEVIS".into())).unwrap();
        assert_eq!(catalogue.liste().len(), 5);
    }

    #[test]
    fn refus() {
        let mut catalogue = Catalogue::default();
        assert!(catalogue.appliquer(&Changement::Poser(cat("a", " ", "#000000"))).is_err());
        assert!(catalogue.appliquer(&Changement::Poser(cat("a", "A", "rouge"))).is_err());
        assert!(catalogue.appliquer(&Changement::Poser(cat("a b", "A", "#000000"))).is_err());
        for i in 0..MAX_CREEES {
            catalogue.appliquer(&Changement::Poser(cat(&format!("c{i}"), "C", "#000000"))).unwrap();
        }
        assert!(catalogue.appliquer(&Changement::Poser(cat("de_trop", "C", "#000000"))).is_err());
        // Modifier une catégorie existante reste possible au plafond.
        catalogue.appliquer(&Changement::Poser(cat("c0", "Zéro", "#111111"))).unwrap();
        assert_eq!(catalogue.creees(), MAX_CREEES);
    }

    #[test]
    fn lecture_tolerante_et_aller_retour() {
        assert_eq!(Catalogue::lire("pas du json"), Catalogue::default());
        let lu = Catalogue::lire(r#"{"categories":[{"motCle":"ok","nom":"Ok","couleur":"bleu"},{"motCle":"a b","nom":"Non"}]}"#);
        assert_eq!(lu.categories, vec![cat("ok", "Ok", COULEUR_INCONNUE)]);
        let mut catalogue = Catalogue::default();
        catalogue.appliquer(&Changement::Poser(cat("devis_a_faire", "Devis à faire", "#0B8043"))).unwrap();
        assert_eq!(Catalogue::lire(&catalogue.ecrire()), catalogue);
        assert!(catalogue.ecrire().contains(r#""motCle":"devis_a_faire""#));
        assert!(catalogue.ecrire().is_ascii());
        assert!(catalogue.ecrire().contains(r#"Devis \u00e0 faire"#));
    }

    #[test]
    fn mots_cles_tires_du_nom() {
        assert_eq!(mot_cle_pour("Devis à faire", &[]), "devis_a_faire");
        assert_eq!(mot_cle_pour("  Œuvre d'art  ", &[]), "oeuvre_d_art");
        assert_eq!(mot_cle_pour("Saisie", &[]), "saisie");
        assert_eq!(mot_cle_pour("Saisie", &["SAISIE".into()]), "saisie_2");
        assert_eq!(mot_cle_pour("Saisie", &["saisie".into(), "saisie_2".into()]), "saisie_3");
        assert_eq!(mot_cle_pour("€€€", &[]), "categorie");
        assert_eq!(mot_cle_pour("Forwarded", &[]), "categorie");
        assert!(crate::protocole::mot_cle_valide(&mot_cle_pour("A (b) [c] {d} % * \" \\", &[])));
    }

    #[test]
    fn modifier_une_liste() {
        assert_eq!(modifier("", "$label1", true), "$label1");
        assert_eq!(modifier("devis $label1", "Devis", false), "$label1");
        assert_eq!(modifier("$label1", "devis", true), "$label1 devis");
        assert_eq!(modifier("$label1", "$LABEL1", true), "$LABEL1");
    }

    #[test]
    fn criteres_de_recherche() {
        assert_eq!(separer_criteres("facture mars"), ("facture mars".into(), vec![]));
        assert_eq!(
            separer_criteres("facture Catégorie:Devis mars"),
            ("facture mars".into(), vec!["Devis".to_string()])
        );
        assert_eq!(
            separer_criteres(r#"categorie:"Devis à faire" CATÉGORIE:Important"#),
            (String::new(), vec!["Devis à faire".to_string(), "Important".to_string()])
        );
        assert_eq!(separer_criteres("heure:10 catégorie:"), ("heure:10".into(), vec![]));
        let mut catalogue = Catalogue::default();
        catalogue.appliquer(&Changement::Poser(cat("devis_a_faire", "Devis à faire", "#0B8043"))).unwrap();
        let mut autre = Catalogue::default();
        autre.appliquer(&Changement::Poser(cat("devis_x", "DEVIS À FAIRE", "#0B8043"))).unwrap();
        let catalogues = vec![catalogue.ecrire(), autre.ecrire()];
        assert_eq!(mots_cles_nommes("devis à faire", &catalogues), vec!["devis_a_faire", "devis_x"]);
        assert_eq!(mots_cles_nommes("important", &[]), vec!["$label1"]);
        assert_eq!(mots_cles_nommes("$LABEL2", &[]), vec!["$label2"]);
        assert!(mots_cles_nommes("inconnue", &catalogues).is_empty());
    }
}
