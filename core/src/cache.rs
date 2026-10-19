// SPDX-License-Identifier: GPL-3.0-or-later
//! Messages gardés sur le poste (décision 17) : de quoi lire, hors connexion,
//! ce qui est arrivé récemment.
//!
//! Un message de moins d'un mois — arbitrage de Manu du 2026-10-01 ; durée et
//! taille réglables pour la machine (`jours`, `taille_max`) — est gardé entier,
//! tel que le serveur le conserve, dans un fichier `.eml` nommé par son
//! identifiant dans l'index (décision 14) : l'affichage, la source et les
//! pièces jointes s'y lisent sans réseau. Au-delà, il est relu sur le serveur. Le serveur reste la référence : ce
//! cache se reconstruit, rien ne s'y perd.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Jours pendant lesquels un message reçu reste sur le poste : 31, sauf
/// réglage de la machine (serveur RDS : `MMAIL_JOURS_CACHE`, que le lanceur
/// pose aussi depuis la base de registre). 0 : rien n'est gardé.
pub fn jours() -> i64 {
    static JOURS: OnceLock<i64> = OnceLock::new();
    *JOURS.get_or_init(|| {
        std::env::var("MMAIL_JOURS_CACHE")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|j| (0..=3650).contains(j))
            .unwrap_or(31)
    })
}

/// Taille au-delà de laquelle un message n'est pas gardé sur le poste, en
/// octets : 10 Mio, sauf réglage de la machine (`MMAIL_TAILLE_CACHE`, en Mio ;
/// 0 : sans plafond). Quelques grosses pièces jointes font l'essentiel d'un
/// cache — 2 Go pour 5 206 messages sur un serveur RDS (15/10) — et un
/// message plus gros reste lu sur le serveur à l'ouverture (0.6.6).
pub fn taille_max() -> i64 {
    static TAILLE: OnceLock<i64> = OnceLock::new();
    *TAILLE.get_or_init(|| {
        let mio: i64 = std::env::var("MMAIL_TAILLE_CACHE")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|m| (0..=100 * 1024).contains(m))
            .unwrap_or(10);
        if mio == 0 {
            i64::MAX
        } else {
            mio * 1024 * 1024
        }
    })
}

/// Date de réception la plus ancienne gardée sur le poste.
pub fn limite(maintenant: i64) -> i64 {
    match jours() {
        0 => i64::MAX,
        j => maintenant - j * 24 * 3600,
    }
}

pub struct Cache {
    racine: PathBuf,
}

impl Cache {
    /// Cache d'un profil : le dossier `messages` à côté de l'index.
    pub fn du_profil(profil: &str) -> Cache {
        let racine = Path::new(profil)
            .parent()
            .map(|p| p.join("messages"))
            .unwrap_or_else(|| PathBuf::from("messages"));
        Cache { racine }
    }

    /// Fichier d'un message, réparti en 256 sous-dossiers : un dossier de
    /// dizaines de milliers de fichiers se parcourt mal.
    fn chemin(&self, id: i64) -> PathBuf {
        self.racine.join(format!("{:02x}", id & 0xff)).join(format!("{id}.eml"))
    }

    pub fn lire(&self, id: i64) -> Option<Vec<u8>> {
        std::fs::read(self.chemin(id)).ok().filter(|o| !o.is_empty())
    }

    /// Écrit un message. Le fichier n'apparaît qu'une fois complet : un autre
    /// fil peut le lire au même moment.
    pub fn ecrire(&self, id: i64, octets: &[u8]) -> std::io::Result<()> {
        let fichier = self.chemin(id);
        if let Some(dossier) = fichier.parent() {
            std::fs::create_dir_all(dossier)?;
        }
        let provisoire = fichier.with_extension("partiel");
        std::fs::write(&provisoire, octets)?;
        std::fs::rename(&provisoire, &fichier)
    }

    pub fn retirer(&self, id: i64) {
        let _ = std::fs::remove_file(self.chemin(id));
    }

    /// Retire tout ce que l'index ne désigne plus comme gardé — messages
    /// effacés, déplacés, dossier relu en entier — et les écritures
    /// interrompues. Rend le nombre de fichiers retirés. Un fichier de moins de
    /// dix minutes est épargné : le rangement tourne pendant que les fils des
    /// comptes préchargent, et un message qui vient d'être écrit n'est pas
    /// encore noté gardé dans l'index.
    pub fn ranger(&self, gardes: &HashSet<i64>) -> usize {
        let recent = |chemin: &std::path::Path| {
            std::fs::metadata(chemin)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age < std::time::Duration::from_secs(600))
        };
        let mut retires = 0;
        let Ok(sous) = std::fs::read_dir(&self.racine) else {
            return 0;
        };
        for dossier in sous.flatten() {
            let Ok(fichiers) = std::fs::read_dir(dossier.path()) else { continue };
            for fichier in fichiers.flatten() {
                let chemin = fichier.path();
                let garde = chemin.extension().is_some_and(|e| e == "eml")
                    && chemin
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| s.parse::<i64>().ok())
                        .is_some_and(|id| gardes.contains(&id));
                if !garde && !recent(&chemin) && std::fs::remove_file(&chemin).is_ok() {
                    retires += 1;
                }
            }
        }
        retires
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecriture_lecture_rangement() {
        let racine = std::env::temp_dir().join(format!("mmail-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        let cache = Cache::du_profil(&racine.join("index.sqlite").to_string_lossy());
        cache.ecrire(7, b"From: a\r\n\r\nx").unwrap();
        cache.ecrire(263, b"From: b\r\n\r\ny").unwrap();
        assert_eq!(cache.lire(7).unwrap(), b"From: a\r\n\r\nx");
        assert!(cache.lire(8).is_none());
        // 7 et 263 tombent dans le même sous-dossier (07).
        assert!(racine.join("messages/07/263.eml").exists());
        std::fs::write(racine.join("messages/07/9.partiel"), b"coupe").unwrap();
        // Tout juste écrits, 7 et 9.partiel sont épargnés ; vieillis, ils partent.
        assert_eq!(cache.ranger(&HashSet::from([263])), 0);
        for nom in ["7.eml", "9.partiel"] {
            std::fs::File::options()
                .write(true)
                .open(racine.join("messages/07").join(nom))
                .unwrap()
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
                .unwrap();
        }
        assert_eq!(cache.ranger(&HashSet::from([263])), 2);
        assert!(cache.lire(7).is_none());
        assert!(cache.lire(263).is_some());
        cache.retirer(263);
        assert!(cache.lire(263).is_none());
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn fenetre_d_un_mois() {
        // Sans réglage de la machine.
        assert_eq!(limite(10_000_000), 10_000_000 - 31 * 86_400);
    }
}
