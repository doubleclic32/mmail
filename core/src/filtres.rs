// SPDX-License-Identifier: GPL-3.0-or-later
//! Filtres du courrier, appliqués par MMail lui-même (0.6.0 ; décision 11,
//! tranchée par Manu le 18/10/2026) : sur tous les comptes — Mailcow,
//! Microsoft, Gmail, OVH —, à l'arrivée des messages dans la boîte de
//! réception d'un MMail ouvert. Les filtres du serveur (Sieve, ceux du
//! webmail SOGo) restent possibles à côté, pour un tri quand aucun MMail
//! n'est ouvert.
//!
//! Un filtre : des conditions sur l'expéditeur (nom ou adresse) et l'objet —
//! toutes, ou une au moins —, puis des actions : marquer lu, suivre, et une
//! destination (un dossier du compte, la corbeille, les indésirables). Les
//! filtres se lisent dans l'ordre ; le premier qui donne une destination la
//! fixe ; « arrêter » coupe la lecture des suivants. La liste part avec la
//! sauvegarde de la configuration : elle suit l'utilisateur d'un poste à
//! l'autre.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Filtre {
    #[serde(default)]
    pub nom: String,
    #[serde(default = "vrai")]
    pub actif: bool,
    /// Adresse du compte visé ; vide : tous les comptes.
    #[serde(default)]
    pub compte: String,
    /// Vrai : toutes les conditions ; faux : une au moins.
    #[serde(default = "vrai")]
    pub tout: bool,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub lu: bool,
    #[serde(default)]
    pub suivi: bool,
    /// `""` (le message reste), `dossier`, `corbeille` ou `indesirables`.
    #[serde(default)]
    pub destination: String,
    /// Chemin du dossier, pour la destination `dossier` — un dossier du
    /// compte visé.
    #[serde(default)]
    pub dossier: String,
    #[serde(default)]
    pub arreter: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Condition {
    /// `expediteur` (nom ou adresse) ou `objet`.
    pub champ: String,
    /// `contient`, `est`, `commence` ou `finit`.
    pub operateur: String,
    pub valeur: String,
}

fn vrai() -> bool {
    true
}

/// Un message reçu, tel que l'index le connaît.
#[derive(Clone, Debug, Default)]
pub struct Recu {
    pub uid: u32,
    pub message_id: String,
    pub expediteur: String,
    pub adresse: String,
    pub sujet: String,
}

/// Ce que les filtres demandent pour des messages d'un même dossier.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub lus: Vec<u32>,
    pub suivis: Vec<u32>,
    /// Par dossier de destination, chemin du compte.
    pub deplacements: BTreeMap<String, Vec<u32>>,
    pub corbeille: Vec<u32>,
    pub indesirables: Vec<u32>,
    /// Les messages qu'un filtre a touchés, par leur `Message-ID` (vide s'il
    /// n'en a pas).
    pub touches: Vec<String>,
}

impl Plan {
    pub fn vide(&self) -> bool {
        self.touches.is_empty()
    }
}

/// Lit et contrôle une liste de filtres (JSON). Rend un message lisible si un
/// filtre est incomplet.
pub fn lire(json: &str) -> Result<Vec<Filtre>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let filtres: Vec<Filtre> = serde_json::from_str(json).map_err(|e| format!("filtres illisibles : {e}"))?;
    for (i, f) in filtres.iter().enumerate() {
        let nom = if f.nom.trim().is_empty() { format!("n° {}", i + 1) } else { format!("« {} »", f.nom.trim()) };
        if f.conditions.is_empty() {
            return Err(format!("le filtre {nom} n'a pas de condition"));
        }
        for c in &f.conditions {
            if !matches!(c.champ.as_str(), "expediteur" | "objet") {
                return Err(format!("le filtre {nom} porte sur un champ inconnu : {}", c.champ));
            }
            if !matches!(c.operateur.as_str(), "contient" | "est" | "commence" | "finit") {
                return Err(format!("le filtre {nom} a une comparaison inconnue : {}", c.operateur));
            }
            if c.valeur.trim().is_empty() {
                return Err(format!("le filtre {nom} a une condition sans texte"));
            }
        }
        match f.destination.as_str() {
            "" | "corbeille" | "indesirables" => {}
            "dossier" if f.dossier.trim().is_empty() || f.compte.trim().is_empty() => {
                return Err(format!("le filtre {nom} déplace vers un dossier sans le nommer, ou pour tous les comptes"));
            }
            "dossier" => {}
            autre => return Err(format!("le filtre {nom} a une destination inconnue : {autre}")),
        }
        if !f.lu && !f.suivi && f.destination.is_empty() {
            return Err(format!("le filtre {nom} n'a pas d'action"));
        }
    }
    Ok(filtres)
}

/// Comparaison sans égard aux majuscules ni aux espaces de bord.
fn plier(texte: &str) -> String {
    texte.trim().to_lowercase()
}

fn correspond(c: &Condition, m: &Recu) -> bool {
    let valeur = plier(&c.valeur);
    let teste = |texte: &str| {
        let texte = plier(texte);
        match c.operateur.as_str() {
            "contient" => texte.contains(&valeur),
            "est" => texte == valeur,
            "commence" => texte.starts_with(&valeur),
            "finit" => texte.ends_with(&valeur),
            _ => false,
        }
    };
    match c.champ.as_str() {
        "expediteur" => teste(&m.expediteur) || teste(&m.adresse),
        "objet" => teste(&m.sujet),
        _ => false,
    }
}

/// Plan des filtres pour les messages `recus` d'un dossier du compte
/// d'adresse `compte`. `deja` : vrai pour un `Message-ID` qu'un filtre a
/// déjà touché — ce message est laissé tel quel : ramené à la main dans la
/// boîte de réception, il n'en repart pas.
pub fn planifier(filtres: &[Filtre], compte: &str, recus: &[Recu], deja: &dyn Fn(&str) -> bool) -> Plan {
    let mut plan = Plan::default();
    let actifs: Vec<&Filtre> =
        filtres.iter().filter(|f| f.actif && (f.compte.is_empty() || f.compte.eq_ignore_ascii_case(compte))).collect();
    for m in recus {
        if !m.message_id.is_empty() && deja(&m.message_id) {
            continue;
        }
        let (mut touche, mut lu, mut suivi) = (false, false, false);
        let mut destination: Option<(&str, &str)> = None;
        for f in &actifs {
            let ok = if f.tout {
                f.conditions.iter().all(|c| correspond(c, m))
            } else {
                f.conditions.iter().any(|c| correspond(c, m))
            };
            if !ok {
                continue;
            }
            touche = true;
            lu |= f.lu;
            suivi |= f.suivi;
            if destination.is_none() && !f.destination.is_empty() {
                destination = Some((f.destination.as_str(), f.dossier.as_str()));
            }
            if f.arreter {
                break;
            }
        }
        if !touche {
            continue;
        }
        plan.touches.push(m.message_id.clone());
        if lu {
            plan.lus.push(m.uid);
        }
        if suivi {
            plan.suivis.push(m.uid);
        }
        match destination {
            Some(("dossier", dossier)) => plan.deplacements.entry(dossier.to_string()).or_default().push(m.uid),
            Some(("corbeille", _)) => plan.corbeille.push(m.uid),
            Some(("indesirables", _)) => plan.indesirables.push(m.uid),
            _ => {}
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recu(uid: u32, expediteur: &str, adresse: &str, sujet: &str) -> Recu {
        Recu { uid, message_id: format!("<{uid}@essai>"), expediteur: expediteur.into(), adresse: adresse.into(), sujet: sujet.into() }
    }

    fn filtre(json: &str) -> Filtre {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn controle_des_filtres() {
        assert!(lire("").unwrap().is_empty());
        assert!(lire("[]").unwrap().is_empty());
        let bon = r#"[{"nom":"Factures","compte":"a@x.fr","conditions":[{"champ":"objet","operateur":"contient","valeur":"facture"}],"destination":"dossier","dossier":"Factures"}]"#;
        assert_eq!(lire(bon).unwrap().len(), 1);
        for (json, attendu) in [
            (r#"[{"nom":"A","lu":true}]"#, "pas de condition"),
            (r#"[{"nom":"A","conditions":[{"champ":"corps","operateur":"contient","valeur":"x"}],"lu":true}]"#, "champ inconnu"),
            (r#"[{"nom":"A","conditions":[{"champ":"objet","operateur":"ressemble","valeur":"x"}],"lu":true}]"#, "comparaison inconnue"),
            (r#"[{"nom":"A","conditions":[{"champ":"objet","operateur":"est","valeur":"  "}],"lu":true}]"#, "sans texte"),
            (r#"[{"nom":"A","conditions":[{"champ":"objet","operateur":"est","valeur":"x"}]}]"#, "pas d'action"),
            (r#"[{"nom":"A","conditions":[{"champ":"objet","operateur":"est","valeur":"x"}],"destination":"dossier","dossier":"F"}]"#, "pour tous les comptes"),
            (r#"[{"nom":"A","conditions":[{"champ":"objet","operateur":"est","valeur":"x"}],"destination":"lune"}]"#, "destination inconnue"),
            ("{", "illisibles"),
        ] {
            let e = lire(json).unwrap_err();
            assert!(e.contains(attendu), "{json} : {e}");
        }
    }

    #[test]
    fn conditions_et_actions() {
        let filtres = vec![
            filtre(r#"{"nom":"Banque","conditions":[{"champ":"expediteur","operateur":"finit","valeur":"@banque.fr"}],"lu":true,"destination":"dossier","dossier":"INBOX/banque","compte":"moi@x.fr"}"#),
            filtre(r#"{"nom":"Promo","tout":false,"conditions":[{"champ":"objet","operateur":"commence","valeur":"PROMO"},{"champ":"expediteur","operateur":"contient","valeur":"newsletter"}],"destination":"indesirables"}"#),
            filtre(r#"{"nom":"Suivi","conditions":[{"champ":"objet","operateur":"contient","valeur":"urgent"}],"suivi":true}"#),
        ];
        let recus = vec![
            recu(1, "Ma Banque", "alerte@banque.fr", "Votre relevé"),
            recu(2, "Boutique", "info@boutique.fr", "Promo d'automne"),
            recu(3, "Lettre", "newsletter@site.fr", "Nouveautés"),
            recu(4, "Collègue", "c@x.fr", "URGENT : devis"),
            recu(5, "Collègue", "c@x.fr", "Bonjour"),
        ];
        let plan = planifier(&filtres, "moi@x.fr", &recus, &|_| false);
        assert_eq!(plan.lus, vec![1]);
        assert_eq!(plan.deplacements.get("INBOX/banque"), Some(&vec![1]));
        assert_eq!(plan.indesirables, vec![2, 3]);
        assert_eq!(plan.suivis, vec![4]);
        assert!(plan.corbeille.is_empty());
        assert_eq!(plan.touches.len(), 4);
        // Un autre compte : le filtre « Banque » ne le vise pas.
        let autre = planifier(&filtres, "autre@x.fr", &recus, &|_| false);
        assert!(autre.deplacements.is_empty() && autre.lus.is_empty());
        assert_eq!(autre.indesirables, vec![2, 3]);
    }

    #[test]
    fn ordre_arret_et_messages_deja_touches() {
        let filtres = vec![
            filtre(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"devis"}],"destination":"corbeille","arreter":true}"#),
            filtre(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"devis"}],"lu":true,"destination":"indesirables"}"#),
            filtre(r#"{"actif":false,"conditions":[{"champ":"objet","operateur":"contient","valeur":"bonjour"}],"destination":"corbeille"}"#),
            filtre(r#"{"conditions":[{"champ":"objet","operateur":"est","valeur":"facture"}],"lu":true}"#),
            filtre(r#"{"conditions":[{"champ":"objet","operateur":"est","valeur":"facture"}],"destination":"corbeille"}"#),
        ];
        let recus = vec![recu(1, "", "a@b.fr", "Devis 42"), recu(2, "", "a@b.fr", "Bonjour"), recu(3, "", "a@b.fr", "  Facture ")];
        let plan = planifier(&filtres, "moi@x.fr", &recus, &|_| false);
        // « arrêter » : le second filtre ne marque pas le devis comme lu.
        assert_eq!(plan.corbeille, vec![1, 3]);
        assert!(plan.indesirables.is_empty());
        assert_eq!(plan.lus, vec![3]);
        // Le filtre inactif ne touche pas « Bonjour ».
        assert_eq!(plan.touches.len(), 2);
        // Déjà touché : laissé tel quel.
        let deja = planifier(&filtres, "moi@x.fr", &recus, &|id| id == "<1@essai>");
        assert_eq!(deja.corbeille, vec![3]);
    }
}
