// SPDX-License-Identifier: GPL-3.0-or-later
//! Mise à jour assistée (demande de Manu du 10/10/2026) : MMail signale une
//! version plus récente et, si on le lui demande, télécharge le paquet de sa
//! plateforme, en contrôle l'empreinte et le tient prêt. L'installation — un
//! installeur silencieux lancé à la fermeture, ou l'AppImage remplacée — revient
//! au programme (`cpp/installeur.h`) : elle suit la fin du processus.
//!
//! Tout vient de la publication du dépôt sur GitHub : son numéro par l'API, le
//! paquet et le fichier des empreintes (`SHA256SUMS-<version>.txt`) à leur
//! adresse de publication, que MMail compose lui-même. Un paquet dont
//! l'empreinte ne correspond pas à celle publiée n'est jamais gardé.
//!
//! Depuis la 0.5.17, le fichier des empreintes doit être signé (Ed25519,
//! `SHA256SUMS-<version>.txt.sig`) par la clé de MMEDIA dont la partie publique
//! est ici : la clé privée n'est pas sur GitHub, si bien qu'un compte GitHub
//! compromis ne suffit plus à faire installer un paquet aux postes.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type MiseAJour = super::MiseAJourRust;

        /// Demande en arrière-plan la dernière version publiée. Issue :
        /// `disponible`, seulement si elle est plus récente que celle-ci.
        #[qinvokable]
        #[cxx_name = "verifier"]
        fn verifier(self: Pin<&mut MiseAJour>);

        /// Télécharge en arrière-plan le paquet `genre` (« setup », « appimage »)
        /// de `version` vers `destination`, empreinte contrôlée. Rien n'est
        /// téléchargé si le fichier y est déjà, intact. Issues : `progression`,
        /// puis `prete` ou `echec`. Sans effet pendant un autre téléchargement.
        #[qinvokable]
        #[cxx_name = "telecharger"]
        fn telecharger(self: Pin<&mut MiseAJour>, version: &QString, genre: &QString, destination: &QString);

        /// Efface de `dossier` les paquets et les téléchargements interrompus
        /// de MMail, sauf le fichier `garder` (chemin complet, ou vide). Pour
        /// une AppImage, dont le dossier est celui de l'utilisateur, seuls les
        /// téléchargements interrompus — fichiers cachés écrits par MMail.
        #[qinvokable]
        #[cxx_name = "nettoyer"]
        fn nettoyer(self: &MiseAJour, genre: &QString, dossier: &QString, garder: &QString);

        /// Version de ce programme, telle que la compare `verifier`.
        #[qinvokable]
        #[cxx_name = "versionActuelle"]
        fn version_actuelle(self: &MiseAJour) -> QString;
    }

    impl cxx_qt::Threading for MiseAJour {}

    unsafe extern "RustQt" {
        /// Une version plus récente est publiée : son numéro et sa page.
        #[qsignal]
        #[cxx_name = "disponible"]
        fn disponible(self: Pin<&mut MiseAJour>, version: &QString, page: &QString);

        /// Avancement du téléchargement, en pour cent.
        #[qsignal]
        #[cxx_name = "progression"]
        fn progression(self: Pin<&mut MiseAJour>, version: &QString, pourcent: i32);

        /// Le paquet est là, intact.
        #[qsignal]
        #[cxx_name = "prete"]
        fn prete(self: Pin<&mut MiseAJour>, version: &QString, fichier: &QString);

        #[qsignal]
        #[cxx_name = "echec"]
        fn echec(self: Pin<&mut MiseAJour>, version: &QString, message: &QString);
    }
}

use core::pin::Pin;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use aws_lc_rs::digest;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::http;

/// Dernière publication du dépôt : ni préversion ni brouillon.
const PUBLICATIONS: &str = "https://api.github.com/repos/mmedia-fr/mmail/releases/latest";

/// Adresse de publication des fichiers d'une version.
const TELECHARGEMENTS: &str = "https://github.com/mmedia-fr/mmail/releases/download";

/// Taille au-delà de laquelle un paquet est refusé : l'AppImage pèse 50 Mo.
const TAILLE_MAX_PAQUET: u64 = 400 * 1024 * 1024;

/// Numéro et page de la dernière version publiée ; une erreur si GitHub ne
/// répond pas, ou pas comme attendu.
///
/// `MMAIL_JETON_GITHUB` (essais de la CI seulement) : la demande part
/// authentifiée. Sans lui, l'API de GitHub n'accepte que 60 demandes par heure
/// et par adresse — partagées, sur les machines de la CI, entre bien des
/// travaux : refusée (403), la vérification restait muette et l'essai
/// « mise à jour assistée » attendait cinq minutes sans rien écrire (échec
/// intermittent sous Windows, 12 au 18/10).
fn derniere_version() -> Result<(String, String), String> {
    let jeton = std::env::var("MMAIL_JETON_GITHUB").unwrap_or_default();
    let entetes: Vec<(&str, String)> =
        if jeton.is_empty() { Vec::new() } else { vec![("Authorization", format!("Bearer {jeton}"))] };
    let demande = http::Demande { entetes: &entetes, ..http::Demande::simple("GET", PUBLICATIONS, "application/vnd.github+json") };
    let reponse = http::envoyer(&demande).map_err(|e| format!("GitHub injoignable : {e}"))?;
    if reponse.statut != 200 {
        return Err(format!("GitHub répond {}", reponse.statut));
    }
    let v: serde_json::Value = serde_json::from_slice(&reponse.corps).map_err(|e| format!("réponse illisible : {e}"))?;
    let version = v["tag_name"].as_str().ok_or("réponse sans tag_name")?.trim_start_matches('v').to_string();
    let page = v["html_url"].as_str().ok_or("réponse sans html_url")?.to_string();
    // Une page ailleurs que sur le dépôt ne s'ouvre pas depuis MMail.
    if !page.starts_with("https://github.com/mmedia-fr/mmail/") {
        return Err(format!("page de publication inattendue : {page}"));
    }
    Ok((version, page))
}

/// Numéro lu chiffre par chiffre : « 0.5.8 » → [0, 5, 8]. Une préversion
/// (« 0.5.8-essai ») ne se lit pas.
fn numeros(version: &str) -> Option<Vec<u32>> {
    let v = version.trim_start_matches('v');
    if v.is_empty() {
        return None;
    }
    v.split('.').map(|n| n.parse().ok()).collect()
}

/// Vrai si `publiee` (« 0.4.7 ») est plus récente que `actuelle`. Un numéro
/// qui ne se lit pas — préversion « 0.4.7-essai » comprise — n'est jamais
/// annoncé.
fn plus_recente(publiee: &str, actuelle: &str) -> bool {
    matches!((numeros(publiee), numeros(actuelle)), (Some(a), Some(b)) if a > b)
}

/// Version de ce programme. `MMAIL_VERSION_ESSAI` la remplace pour éprouver la
/// mise à jour sur une version déjà publiée : l'essai se croit plus ancien, et
/// installe la dernière publication — rien d'autre ne peut l'être.
fn version_actuelle() -> String {
    std::env::var("MMAIL_VERSION_ESSAI")
        .ok()
        .filter(|v| numeros(v).is_some())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Nom du paquet `genre` d'une version, tel que le publie l'intégration
/// continue ; `None` pour un genre inconnu ou un numéro illisible.
fn nom_du_paquet(version: &str, genre: &str) -> Option<String> {
    numeros(version)?;
    match genre {
        "setup" => Some(format!("MMail-{version}-setup.exe")),
        "appimage" => Some(format!("MMail-{version}-x86_64.AppImage")),
        _ => None,
    }
}

/// Clé publique Ed25519 de MMEDIA qui signe le fichier des empreintes de chaque
/// publication (fiche KeePass « MMail — clé de signature des empreintes de mise
/// à jour », créée le 17/10/2026). En changer impose une version installée à la
/// main sur chaque poste.
const CLE_EMPREINTES: [u8; 32] = [
    0xd7, 0x94, 0xc2, 0x9e, 0x42, 0x4b, 0xb0, 0xdb, 0xe9, 0x8d, 0x6a, 0x36, 0xc3, 0xb1, 0x7a, 0x24,
    0x08, 0x18, 0xdc, 0x75, 0xf2, 0x1d, 0xaa, 0x26, 0x01, 0x80, 0xbc, 0xd3, 0x41, 0x61, 0xa1, 0x23,
];

/// Vrai si `signature` est la signature Ed25519 de `contenu` par la clé `cle`.
fn signature_valide(contenu: &[u8], signature: &[u8], cle: &[u8]) -> bool {
    use aws_lc_rs::signature::{UnparsedPublicKey, ED25519};
    UnparsedPublicKey::new(&ED25519, cle).verify(contenu, signature).is_ok()
}

/// Empreinte publiée pour `nom` dans un fichier au format de `sha256sum` :
/// « <64 chiffres hexadécimaux>  <nom> », en minuscules.
fn empreinte_publiee(sommes: &str, nom: &str) -> Option<String> {
    sommes.lines().find_map(|ligne| {
        let (empreinte, fichier) = ligne.trim().split_once(char::is_whitespace)?;
        let fichier = fichier.trim_start().trim_start_matches('*');
        let valide = empreinte.len() == 64 && empreinte.chars().all(|c| c.is_ascii_hexdigit());
        (valide && fichier == nom).then(|| empreinte.to_ascii_lowercase())
    })
}

fn hexadecimal(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Empreinte SHA-256 d'un fichier ; `None` s'il ne se lit pas.
fn empreinte_du_fichier(chemin: &Path) -> Option<String> {
    let mut fichier = fs::File::open(chemin).ok()?;
    let mut contexte = digest::Context::new(&digest::SHA256);
    let mut tampon = vec![0u8; 256 * 1024];
    loop {
        match fichier.read(&mut tampon).ok()? {
            0 => break,
            n => contexte.update(&tampon[..n]),
        }
    }
    Some(hexadecimal(contexte.finish().as_ref()))
}

/// Téléchargement interrompu : un fichier caché à côté de la destination, pour
/// que le remplacement final soit un simple renommage, d'un seul coup.
fn chemin_partiel(destination: &Path, nom: &str) -> PathBuf {
    destination.with_file_name(format!(".{nom}.part"))
}

/// Télécharge le paquet `genre` de `version` vers `destination` et en contrôle
/// l'empreinte. Rend un message lisible en cas d'échec.
fn telecharger_paquet(
    version: &str,
    genre: &str,
    destination: &Path,
    progression: &mut dyn FnMut(i32),
) -> Result<(), String> {
    let nom = nom_du_paquet(version, genre).ok_or_else(|| format!("paquet « {genre} » inconnu"))?;
    let sommes = http::requete_bornee(
        "GET",
        &format!("{TELECHARGEMENTS}/v{version}/SHA256SUMS-{version}.txt"),
        "text/plain",
        64 * 1024,
    )
    .map_err(|e| format!("empreintes de la publication illisibles — {e}"))?;
    if sommes.statut != 200 {
        return Err(format!("empreintes de la publication introuvables (statut {})", sommes.statut));
    }
    let signature = http::requete_bornee(
        "GET",
        &format!("{TELECHARGEMENTS}/v{version}/SHA256SUMS-{version}.txt.sig"),
        "application/octet-stream",
        1024,
    )
    .map_err(|e| format!("signature des empreintes illisible — {e}"))?;
    if signature.statut != 200 {
        return Err(format!("signature des empreintes introuvable (statut {})", signature.statut));
    }
    if !signature_valide(&sommes.corps, &signature.corps, &CLE_EMPREINTES) {
        return Err("les empreintes de la publication ne portent pas la signature de MMEDIA".into());
    }
    let attendue = empreinte_publiee(&String::from_utf8_lossy(&sommes.corps), &nom)
        .ok_or_else(|| format!("la publication ne donne pas l'empreinte de {nom}"))?;
    if empreinte_du_fichier(destination).as_deref() == Some(attendue.as_str()) {
        progression(100);
        return Ok(());
    }
    if let Some(dossier) = destination.parent() {
        fs::create_dir_all(dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
    }
    let partiel = chemin_partiel(destination, &nom);
    let resultat = (|| {
        let mut fichier = fs::File::create(&partiel).map_err(|e| format!("{} : {e}", partiel.display()))?;
        let mut contexte = digest::Context::new(&digest::SHA256);
        let mut dernier = -1;
        http::telecharger(
            &format!("{TELECHARGEMENTS}/v{version}/{nom}"),
            TAILLE_MAX_PAQUET,
            &mut |morceau| {
                contexte.update(morceau);
                fichier.write_all(morceau)?;
                Ok(())
            },
            &mut |recus, total| {
                let pourcent = (recus * 100 / total.max(1)) as i32;
                if pourcent != dernier {
                    dernier = pourcent;
                    progression(pourcent);
                }
            },
        )
        .map_err(|e| e.to_string())?;
        fichier.sync_all().map_err(|e| e.to_string())?;
        if hexadecimal(contexte.finish().as_ref()) != attendue {
            return Err(format!("{nom} reçu ne correspond pas à l'empreinte publiée"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&partiel, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        fs::rename(&partiel, destination).map_err(|e| format!("{} : {e}", destination.display()))
    })();
    if resultat.is_err() {
        let _ = fs::remove_file(&partiel);
    }
    resultat
}

/// Version d'un installeur d'après son nom : « …/MMail-0.5.8-setup.exe » →
/// « 0.5.8 ».
fn version_du_paquet(chemin: &Path) -> Option<String> {
    let nom = chemin.file_name()?.to_str()?;
    let version = nom.strip_prefix("MMail-")?.strip_suffix("-setup.exe")?;
    numeros(version).map(|_| version.to_string())
}

/// Fichiers de `dossier` que `nettoyer` efface.
fn a_effacer(genre: &str, nom: &str) -> bool {
    let partiel = nom.starts_with(".MMail-") && nom.ends_with(".part");
    let paquet = genre == "setup" && nom.starts_with("MMail-") && nom.ends_with("-setup.exe");
    partiel || paquet
}

#[derive(Default)]
pub struct MiseAJourRust {
    en_cours: Arc<AtomicBool>,
}

impl qobject::MiseAJour {
    pub fn verifier(self: Pin<&mut Self>) {
        let fil = self.qt_thread();
        let _ = thread::Builder::new().name("mmail-version".into()).spawn(move || {
            let (version, page) = match derniere_version() {
                Ok(v) => v,
                Err(e) => {
                    // Muet pour la personne (la vérification se refera) ; dit
                    // dans le journal d'un essai.
                    if std::env::var_os("MMAIL_SCENARIO").is_some() {
                        eprintln!("mise à jour : vérification impossible — {e}");
                    }
                    return;
                }
            };
            if plus_recente(&version, &version_actuelle()) {
                let _ = fil.queue(move |objet: Pin<&mut qobject::MiseAJour>| {
                    objet.disponible(&QString::from(&version), &QString::from(&page));
                });
            }
        });
    }

    pub fn telecharger(self: Pin<&mut Self>, version: &QString, genre: &QString, destination: &QString) {
        let en_cours = self.rust().en_cours.clone();
        if en_cours.swap(true, Ordering::SeqCst) {
            return;
        }
        let (version, genre) = (version.to_string(), genre.to_string());
        let destination = PathBuf::from(destination.to_string());
        let fil = self.qt_thread();
        let lance = thread::Builder::new().name("mmail-mise-a-jour".into()).spawn(move || {
            let progres = fil.clone();
            let v = version.clone();
            let resultat = telecharger_paquet(&version, &genre, &destination, &mut |pourcent| {
                let v = v.clone();
                let _ = progres.queue(move |objet: Pin<&mut qobject::MiseAJour>| {
                    objet.progression(&QString::from(&v), pourcent);
                });
            });
            en_cours.store(false, Ordering::SeqCst);
            let fichier = destination.to_string_lossy().into_owned();
            let _ = fil.queue(move |objet: Pin<&mut qobject::MiseAJour>| match resultat {
                Ok(()) => objet.prete(&QString::from(&version), &QString::from(&fichier)),
                Err(message) => objet.echec(&QString::from(&version), &QString::from(&message)),
            });
        });
        if lance.is_err() {
            self.rust().en_cours.store(false, Ordering::SeqCst);
        }
    }

    pub fn nettoyer(&self, genre: &QString, dossier: &QString, garder: &QString) {
        let genre = genre.to_string();
        // Un paquet n'est gardé que s'il reste à installer.
        let garder = PathBuf::from(garder.to_string());
        let garder = version_du_paquet(&garder)
            .filter(|v| plus_recente(v, &version_actuelle()))
            .map(|_| garder)
            .unwrap_or_default();
        let Ok(entrees) = fs::read_dir(dossier.to_string()) else { return };
        for entree in entrees.flatten() {
            let chemin = entree.path();
            let nom = entree.file_name().to_string_lossy().into_owned();
            if chemin != garder && a_effacer(&genre, &nom) && chemin.is_file() {
                let _ = fs::remove_file(&chemin);
            }
        }
    }

    pub fn version_actuelle(&self) -> QString {
        QString::from(&version_actuelle())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La clé compilée est bien celle de MMEDIA : signature témoin faite le
    /// 17/10/2026 avec la clé privée, sur « mmail-empreintes\n ».
    #[test]
    fn cle_des_empreintes_de_mmedia() {
        let temoin: Vec<u8> = (0..64)
            .map(|i| u8::from_str_radix(&SIGNATURE_TEMOIN[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        assert!(signature_valide(b"mmail-empreintes\n", &temoin, &CLE_EMPREINTES));
        assert!(!signature_valide(b"mmail-empreintes", &temoin, &CLE_EMPREINTES));
    }
    const SIGNATURE_TEMOIN: &str = "abf624eac67a66b53920cb476babfad5a9a5dc63c5af061a83dcc8a3c657f317\
                                    7f8235c9f6f986b15d0d6e22a15b75f712ac7594a9d30abbe117132318c4db03";

    #[test]
    fn signature_des_empreintes() {
        use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&aws_lc_rs::rand::SystemRandom::new()).unwrap();
        let paire = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let sommes = b"0123  MMail-0.5.17-setup.exe\n";
        let signature = paire.sign(sommes);
        let cle = paire.public_key().as_ref();
        assert!(signature_valide(sommes, signature.as_ref(), cle));
        // Une empreinte changée, une autre clé, une signature tronquée : refus.
        assert!(!signature_valide(b"0124  MMail-0.5.17-setup.exe\n", signature.as_ref(), cle));
        assert!(!signature_valide(sommes, signature.as_ref(), &CLE_EMPREINTES));
        assert!(!signature_valide(sommes, &signature.as_ref()[..63], cle));
        assert!(!signature_valide(sommes, b"", cle));
    }

    #[test]
    #[ignore = "interroge l'API de GitHub"]
    fn derniere_version_publiee() {
        let (version, page) = derniere_version().expect("réponse de GitHub");
        println!("dernière version publiée : {version} ({page})");
        assert!(numeros(&version).is_some(), "{version}");
        assert!(page.starts_with("https://github.com/mmedia-fr/mmail/releases/tag/"));
    }

    #[test]
    fn comparaison_des_versions() {
        assert!(plus_recente("0.4.7", "0.4.6"));
        assert!(plus_recente("v0.4.10", "0.4.9"), "nombre, pas texte");
        assert!(plus_recente("0.5.0", "0.4.12"));
        assert!(!plus_recente("0.4.6", "0.4.6"));
        assert!(!plus_recente("0.4.5", "0.4.6"));
        assert!(!plus_recente("0.4.7-essai", "0.4.6"), "préversion jamais annoncée");
        assert!(!plus_recente("", "0.4.6"));
    }

    #[test]
    fn noms_des_paquets() {
        assert_eq!(nom_du_paquet("0.5.8", "setup").as_deref(), Some("MMail-0.5.8-setup.exe"));
        assert_eq!(nom_du_paquet("0.5.8", "appimage").as_deref(), Some("MMail-0.5.8-x86_64.AppImage"));
        assert_eq!(nom_du_paquet("0.5.8", "apk"), None);
        // Le numéro entre dans une adresse et un nom de fichier : rien d'autre
        // que des chiffres et des points.
        assert_eq!(nom_du_paquet("0.5.8/../x", "setup"), None);
        assert_eq!(nom_du_paquet("", "setup"), None);
    }

    /// Le fichier publié avec la 0.5.7.
    const SOMMES: &str = "8da62e93d0ef176b0013d6742a71fde5c6ef687e58a037259707d7bab6c44534  MMail-0.5.7-android-arm64.apk
43b157b96a4a34adba6948c723648bd169a8cba6728d782642c35cbf2251bec9  MMail-0.5.7-setup.exe
06adfd5e9a0cdf9e8b111abb0c9b40e1acf1b405097f0fc1cf1d81fe76b1c5dc  MMail-0.5.7-x86_64.AppImage
";

    #[test]
    fn empreintes_publiees() {
        assert_eq!(
            empreinte_publiee(SOMMES, "MMail-0.5.7-setup.exe").as_deref(),
            Some("43b157b96a4a34adba6948c723648bd169a8cba6728d782642c35cbf2251bec9")
        );
        assert_eq!(
            empreinte_publiee(SOMMES, "MMail-0.5.7-x86_64.AppImage").as_deref(),
            Some("06adfd5e9a0cdf9e8b111abb0c9b40e1acf1b405097f0fc1cf1d81fe76b1c5dc")
        );
        assert_eq!(empreinte_publiee(SOMMES, "MMail-0.5.8-setup.exe"), None);
        assert_eq!(empreinte_publiee(SOMMES, "setup.exe"), None, "nom entier");
        // Mode binaire de sha256sum (« *nom ») et majuscules.
        let binaire = "ABCDEF0123456789abcdef0123456789ABCDEF0123456789abcdef0123456789 *MMail-1.0.0-setup.exe";
        assert_eq!(
            empreinte_publiee(binaire, "MMail-1.0.0-setup.exe").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(empreinte_publiee("abc  MMail-1.0.0-setup.exe", "MMail-1.0.0-setup.exe"), None);
    }

    #[test]
    fn empreinte_d_un_fichier() {
        let dossier = std::env::temp_dir().join(format!("mmail-maj-{}", std::process::id()));
        fs::create_dir_all(&dossier).unwrap();
        let chemin = dossier.join("abc");
        fs::write(&chemin, b"abc").unwrap();
        assert_eq!(
            empreinte_du_fichier(&chemin).as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(empreinte_du_fichier(&dossier.join("absent")), None);
        fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn fichiers_effaces_au_nettoyage() {
        assert!(a_effacer("setup", "MMail-0.5.8-setup.exe"));
        assert!(a_effacer("setup", ".MMail-0.5.8-setup.exe.part"));
        assert!(a_effacer("appimage", ".MMail-0.5.8-x86_64.AppImage.part"));
        // Le dossier d'une AppImage est celui de l'utilisateur : ses fichiers à
        // lui n'y sont jamais touchés, installeurs Windows compris.
        assert!(!a_effacer("appimage", "MMail-0.5.8-setup.exe"));
        assert!(!a_effacer("appimage", "MMail-0.5.7-x86_64.AppImage"));
        assert!(!a_effacer("setup", "notes.txt"));
        assert_eq!(version_du_paquet(Path::new("/x/MMail-0.5.8-setup.exe")).as_deref(), Some("0.5.8"));
        assert_eq!(version_du_paquet(Path::new("/x/MMail-0.5.8-x86_64.AppImage")), None);
        assert_eq!(version_du_paquet(Path::new("")), None);
    }

    #[test]
    #[ignore = "télécharge le fichier des empreintes et le paquet Windows de la 0.5.16 sur GitHub"]
    fn paquet_publie_telecharge_et_controle() {
        let dossier = std::env::temp_dir().join(format!("mmail-maj-reel-{}", std::process::id()));
        let destination = dossier.join("MMail-0.5.16-setup.exe");
        let mut etapes = Vec::new();
        telecharger_paquet("0.5.16", "setup", &destination, &mut |p| etapes.push(p)).unwrap();
        assert_eq!(etapes.last(), Some(&100));
        assert_eq!(
            empreinte_du_fichier(&destination).as_deref(),
            Some("6a3ac8c555f47df57de2c52f0b62b171b812d761933b36e7e7d9b19d2dad9499")
        );
        assert!(!chemin_partiel(&destination, "MMail-0.5.16-setup.exe").exists());
        // Déjà là, intact : rien n'est téléchargé de nouveau.
        let mut encore = Vec::new();
        telecharger_paquet("0.5.16", "setup", &destination, &mut |p| encore.push(p)).unwrap();
        assert_eq!(encore, vec![100]);
        // Altéré : remplacé par le paquet publié.
        fs::write(&destination, b"altere").unwrap();
        telecharger_paquet("0.5.16", "setup", &destination, &mut |_| {}).unwrap();
        assert_eq!(
            empreinte_du_fichier(&destination).as_deref(),
            Some("6a3ac8c555f47df57de2c52f0b62b171b812d761933b36e7e7d9b19d2dad9499")
        );
        // Publication sans signature des empreintes (0.5.7, d'avant la 0.5.17) :
        // refusée avant tout téléchargement.
        let ancienne = dossier.join("MMail-0.5.7-setup.exe");
        let refus = telecharger_paquet("0.5.7", "setup", &ancienne, &mut |_| {}).unwrap_err();
        println!("publication non signée : {refus}");
        assert!(refus.contains("signature"), "{refus}");
        assert!(!ancienne.exists());
        // Version sans publication : échec lisible, rien d'écrit.
        let absente = dossier.join("MMail-9.9.9-setup.exe");
        let erreur = telecharger_paquet("9.9.9", "setup", &absente, &mut |_| {}).unwrap_err();
        println!("version absente : {erreur}");
        assert!(!absente.exists());
        fs::remove_dir_all(&dossier).unwrap();
    }
}
