// SPDX-License-Identifier: GPL-3.0-or-later
//! Configurer un compte sans saisir le serveur, ou sans rien saisir du tout.
//!
//! Deux voies, indépendantes :
//!
//! - **configuration automatique** : le serveur IMAP se déduit de l'adresse,
//!   par le document que publie le domaine au format « autoconfig » de
//!   Thunderbird — celui que sert Mailcow. Seules les adresses du domaine
//!   lui-même sont interrogées, aucun annuaire tiers ;
//! - **lien de configuration** : un administrateur prépare le compte entier,
//!   mot de passe compris, derrière une adresse HTTPS à usage unique. MMail
//!   l'interroge en POST et reçoit un document JSON, que l'interface valide
//!   (format décrit dans le README).
//!
//! L'avis de nouvelle version est dans `mise_a_jour`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Configuration = super::ConfigurationRust;

        /// Cherche en arrière-plan le serveur IMAP d'une adresse. Issue :
        /// `serveurDecouvert`.
        #[qinvokable]
        #[cxx_name = "decouvrir"]
        fn decouvrir(self: Pin<&mut Configuration>, adresse: &QString);

        /// Interroge en arrière-plan un lien de configuration. Issue : `lienLu`
        /// ou `lienEchoue`.
        #[qinvokable]
        #[cxx_name = "lireLien"]
        fn lire_lien(self: Pin<&mut Configuration>, lien: &QString);
    }

    impl cxx_qt::Threading for Configuration {}

    unsafe extern "RustQt" {
        /// Serveur IMAPS trouvé pour `adresse` ; `hote` vide si rien n'a été
        /// trouvé.
        #[qsignal]
        #[cxx_name = "serveurDecouvert"]
        fn serveur_decouvert(self: Pin<&mut Configuration>, adresse: &QString, hote: &QString);

        /// Document rendu par le lien, tel quel : c'est l'interface qui le
        /// valide et crée les comptes.
        #[qsignal]
        #[cxx_name = "lienLu"]
        fn lien_lu(self: Pin<&mut Configuration>, contenu: &QString);

        #[qsignal]
        #[cxx_name = "lienEchoue"]
        fn lien_echoue(self: Pin<&mut Configuration>, message: &QString);
    }
}

use core::pin::Pin;
use std::thread;

use cxx_qt::Threading;
use cxx_qt_lib::QString;

use crate::http;
use crate::imap::{Erreur, Resultat};

/// Seul port que MMail sait joindre : IMAP sur TLS d'emblée.
pub const PORT_IMAPS: u16 = 993;

#[derive(Default)]
pub struct ConfigurationRust;

impl qobject::Configuration {
    pub fn decouvrir(self: Pin<&mut Self>, adresse: &QString) {
        let adresse = adresse.to_string().trim().to_string();
        let fil = self.qt_thread();
        let _ = thread::Builder::new().name("mmail-decouverte".into()).spawn(move || {
            let hote = decouvrir(&adresse).map(|s| s.hote).unwrap_or_default();
            let _ = fil.queue(move |objet: Pin<&mut qobject::Configuration>| {
                objet.serveur_decouvert(&QString::from(&adresse), &QString::from(&hote));
            });
        });
    }

    pub fn lire_lien(self: Pin<&mut Self>, lien: &QString) {
        let lien = lien.to_string();
        let fil = self.qt_thread();
        let _ = thread::Builder::new().name("mmail-lien".into()).spawn(move || {
            let issue = lire_lien(&lien);
            let _ = fil.queue(move |objet: Pin<&mut qobject::Configuration>| match issue {
                Ok(contenu) => objet.lien_lu(&QString::from(&contenu)),
                Err(e) => objet.lien_echoue(&QString::from(&e.to_string())),
            });
        });
    }
}

#[derive(Debug, PartialEq)]
pub struct ServeurImap {
    pub hote: String,
    pub port: u16,
}

/// Domaine d'une adresse, s'il a une forme qu'on peut interroger.
pub fn domaine(adresse: &str) -> Option<&str> {
    let (local, domaine) = adresse.trim().rsplit_once('@')?;
    let valide = !local.is_empty()
        && domaine.contains('.')
        && !domaine.starts_with('.')
        && !domaine.ends_with('.')
        && domaine.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valide.then_some(domaine)
}

/// Encodage d'un paramètre de requête : tout sauf les caractères non réservés.
fn encoder(texte: &str) -> String {
    texte
        .bytes()
        .map(|o| match o {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (o as char).to_string(),
            _ => format!("%{o:02X}"),
        })
        .collect()
}

/// Adresses interrogées, dans l'ordre : le sous-domaine `autoconfig`, puis le
/// domaine lui-même — l'ordre de Thunderbird, sans son annuaire central.
pub fn adresses_autoconfig(adresse: &str) -> Vec<String> {
    let Some(domaine) = domaine(adresse) else {
        return Vec::new();
    };
    let parametre = encoder(adresse.trim());
    vec![
        format!("https://autoconfig.{domaine}/mail/config-v1.1.xml?emailaddress={parametre}"),
        format!("https://{domaine}/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress={parametre}"),
    ]
}

/// Serveur IMAPS d'une adresse, d'après la configuration que publie son
/// domaine. `None` si aucune adresse ne répond, ou si aucun serveur annoncé
/// n'est utilisable par MMail.
pub fn decouvrir(adresse: &str) -> Option<ServeurImap> {
    adresses_autoconfig(adresse).into_iter().find_map(|url| {
        let reponse = http::requete("GET", &url, "application/xml, text/xml").ok()?;
        if reponse.statut != 200 {
            return None;
        }
        analyser_autoconfig(std::str::from_utf8(&reponse.corps).ok()?, adresse)
    })
}

/// Premier serveur entrant utilisable par MMail : IMAP, TLS d'emblée, port
/// 993, identifiant égal à l'adresse — c'est ce que MMail envoie à la connexion.
pub fn analyser_autoconfig(xml: &str, adresse: &str) -> Option<ServeurImap> {
    // Pas de DTD : roxmltree la refuse par défaut, et avec elle les entités.
    let document = roxmltree::Document::parse(xml).ok()?;
    let domaine = domaine(adresse)?;
    document
        .descendants()
        .filter(|n| n.has_tag_name("incomingServer") && n.attribute("type") == Some("imap"))
        .find_map(|serveur| {
            let champ = |nom: &str| {
                serveur
                    .children()
                    .find(|c| c.has_tag_name(nom))
                    .and_then(|c| c.text())
                    .map(str::trim)
            };
            if !champ("socketType")?.eq_ignore_ascii_case("SSL") {
                return None;
            }
            let port: u16 = champ("port")?.parse().ok()?;
            if port != PORT_IMAPS {
                return None;
            }
            match champ("username") {
                None | Some("%EMAILADDRESS%") => {}
                Some(nom) if nom.eq_ignore_ascii_case(adresse.trim()) => {}
                Some(_) => return None,
            }
            let hote = champ("hostname")?.replace("%EMAILDOMAIN%", domaine).to_ascii_lowercase();
            let valide = !hote.is_empty()
                && hote.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            valide.then_some(ServeurImap { hote, port })
        })
}

/// Adresse des liens de configuration du service de M-Media. `MMAIL_LIENS` la
/// remplace pour les essais — en HTTPS, comme elle.
fn prefixe_liens() -> String {
    std::env::var("MMAIL_LIENS")
        .ok()
        .filter(|p| p.starts_with("https://"))
        .unwrap_or_else(|| "https://agent.mmedia.fr/mmail/".to_string())
}

/// Vrai pour un lien du service de M-Media : `<préfixe><jeton>`, le jeton
/// fait de lettres, de chiffres, de `-` et de `_`. Un lien de configuration
/// apporte des mots de passe, que MMail garde : un document venu d'ailleurs
/// pourrait les détourner vers un serveur piégé (0.6.4).
pub fn lien_reconnu(lien: &str) -> bool {
    let prefixe = prefixe_liens();
    lien.strip_prefix(prefixe.as_str()).is_some_and(|jeton| {
        (8..=128).contains(&jeton.len()) && jeton.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    })
}

/// Interroge un lien de configuration et rend son document. POST : le serveur
/// qui l'a préparé le détruit en le remettant ; une simple visite dans un
/// navigateur, en GET, ne le consomme pas.
pub fn lire_lien(lien: &str) -> Resultat<String> {
    if !lien_reconnu(lien) {
        return Err(Erreur::Refuse(
            "ce lien ne vient pas de M-Media (il doit commencer par https://agent.mmedia.fr/mmail/)".into(),
        ));
    }
    let reponse = http::requete("POST", lien, "application/json")?;
    match reponse.statut {
        200 => String::from_utf8(reponse.corps)
            .map_err(|_| Erreur::Protocole("document de configuration illisible".into())),
        404 | 410 => Err(Erreur::Refuse("lien inconnu, déjà utilisé ou expiré".into())),
        statut => Err(Erreur::Refuse(format!("le serveur a répondu {statut}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait du document servi par Mailcow.
    const MAILCOW: &str = r#"<?xml version="1.0"?><clientConfig version="1.1">
    <emailProvider id="mail.exemple.fr">
      <domain>%EMAILDOMAIN%</domain>
      <incomingServer type="imap">
         <hostname>mail.exemple.fr</hostname>
         <port>993</port>
         <socketType>SSL</socketType>
         <username>%EMAILADDRESS%</username>
         <authentication>password-cleartext</authentication>
      </incomingServer>
      <incomingServer type="imap">
         <hostname>mail.exemple.fr</hostname>
         <port>143</port>
         <socketType>STARTTLS</socketType>
         <username>%EMAILADDRESS%</username>
      </incomingServer>
      <incomingServer type="pop3">
         <hostname>pop.exemple.fr</hostname>
         <port>995</port>
         <socketType>SSL</socketType>
      </incomingServer>
    </emailProvider></clientConfig>"#;

    #[test]
    fn liens_du_service_seuls() {
        assert!(lien_reconnu("https://agent.mmedia.fr/mmail/Ab3_dEf-9xYz0123456789ab"));
        assert!(!lien_reconnu("https://agent.mmedia.fr/mmail/"));
        assert!(!lien_reconnu("https://agent.mmedia.fr/mmail/sauvegardes/x/y"), "« / » hors du jeton");
        assert!(!lien_reconnu("https://agent.mmedia.fr.piege.com/mmail/Ab3dEf9xYz012345"));
        assert!(!lien_reconnu("http://agent.mmedia.fr/mmail/Ab3dEf9xYz012345"));
        assert!(!lien_reconnu("https://exemple.com/mmail/Ab3dEf9xYz012345"));
        assert!(lire_lien("https://exemple.com/mmail/Ab3dEf9xYz012345").is_err());
    }

    #[test]
    fn serveur_mailcow() {
        assert_eq!(
            analyser_autoconfig(MAILCOW, "nom@client.fr"),
            Some(ServeurImap { hote: "mail.exemple.fr".into(), port: 993 })
        );
    }

    #[test]
    fn serveurs_inutilisables_ecartes() {
        // STARTTLS seul, ou un autre identifiant que l'adresse : rien.
        let starttls = MAILCOW.replacen("<socketType>SSL</socketType>", "<socketType>STARTTLS</socketType>", 1);
        assert_eq!(analyser_autoconfig(&starttls, "nom@client.fr"), None);
        let identifiant = MAILCOW.replacen("%EMAILADDRESS%", "%EMAILLOCALPART%", 1);
        assert_eq!(analyser_autoconfig(&identifiant, "nom@client.fr"), None);
        let autre_port = MAILCOW.replacen("<port>993</port>", "<port>1993</port>", 1);
        assert_eq!(analyser_autoconfig(&autre_port, "nom@client.fr"), None);
    }

    #[test]
    fn domaine_substitue() {
        let xml = MAILCOW.replacen("mail.exemple.fr</hostname>", "imap.%EMAILDOMAIN%</hostname>", 1);
        assert_eq!(analyser_autoconfig(&xml, "nom@client.fr").unwrap().hote, "imap.client.fr");
    }

    #[test]
    fn documents_refuses() {
        assert_eq!(analyser_autoconfig("pas du xml", "nom@client.fr"), None);
        let dtd = format!("<!DOCTYPE x [<!ENTITY e \"mail.exemple.fr\">]>{}", &MAILCOW[21..]);
        assert_eq!(analyser_autoconfig(&dtd, "nom@client.fr"), None);
        assert_eq!(analyser_autoconfig(MAILCOW, "adresse-sans-domaine"), None);
    }

    /// Contre un vrai domaine : `MMAIL_DECOUVERTE=nom@domaine cargo test -- --ignored decouverte`.
    #[test]
    #[ignore = "exige le réseau et MMAIL_DECOUVERTE"]
    fn decouverte_reelle() {
        let adresse = std::env::var("MMAIL_DECOUVERTE").expect("MMAIL_DECOUVERTE");
        let serveur = decouvrir(&adresse);
        eprintln!("{adresse} -> {serveur:?}");
        assert!(serveur.is_some());
    }

    #[test]
    fn adresses_interrogees() {
        assert_eq!(
            adresses_autoconfig(" Jean.Dupont+tri@client.fr "),
            vec![
                "https://autoconfig.client.fr/mail/config-v1.1.xml?emailaddress=Jean.Dupont%2Btri%40client.fr",
                "https://client.fr/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=Jean.Dupont%2Btri%40client.fr",
            ]
        );
        assert!(adresses_autoconfig("nom@localhost").is_empty());
        assert!(adresses_autoconfig("nom@client.fr/../x").is_empty());
        assert!(adresses_autoconfig("@client.fr").is_empty());
    }
}
