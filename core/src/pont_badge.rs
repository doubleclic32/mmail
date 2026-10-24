// SPDX-License-Identifier: GPL-3.0-or-later
//! Passerelle QML du badge animé de la signature (`crate::badge`, 0.6.17).
//!
//! Les images choisies sont décodées par Qt (les formats que MMail affiche
//! déjà), le badge est fabriqué dans un fil à part — 50 à 150 images à
//! dessiner, puis à réduire à 256 couleurs chacune — et rendu sous forme d'un
//! GIF provisoire : l'aperçu, ou le badge à insérer, que la fenêtre copie
//! ensuite dans le dossier des signatures (`Boite::imageSignature`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Vrai pendant une fabrication.
        #[qproperty(bool, occupe)]
        type BadgeAnime = super::BadgeAnimeRust;

        /// Fabrique en arrière-plan le badge que décrit `composition` (JSON,
        /// cf. `badge::Composition`). `apercu` : couleurs réduites plus vite.
        /// Issue : `pret` ou `echec`. Demandée pendant une fabrication, elle
        /// attend la fin de celle-ci ; de deux aperçus en attente, seul le
        /// dernier est fait, et un aperçu ne remplace pas un badge à insérer.
        #[qinvokable]
        #[cxx_name = "fabriquer"]
        fn fabriquer(self: Pin<&mut BadgeAnime>, composition: &QString, apercu: bool);

        /// Réglages par défaut de l'animation (JSON).
        #[qinvokable]
        #[cxx_name = "reglagesParDefaut"]
        fn reglages_par_defaut(self: &BadgeAnime) -> QString;

        /// Nombre d'images au plus dans un badge.
        #[qinvokable]
        #[cxx_name = "visuelsMax"]
        fn visuels_max(self: &BadgeAnime) -> i32;

        /// Largeur du badge dans la signature, en pixels.
        #[qinvokable]
        #[cxx_name = "largeurAffichee"]
        fn largeur_affichee(self: &BadgeAnime) -> i32;
    }

    impl cxx_qt::Threading for BadgeAnime {}

    unsafe extern "RustQt" {
        /// Le badge est prêt : adresse `file:` du GIF provisoire, nombre
        /// d'images et poids en octets.
        #[qsignal]
        #[cxx_name = "pret"]
        fn pret(self: Pin<&mut BadgeAnime>, url: &QString, apercu: bool, images: i32, octets: i32);

        #[qsignal]
        #[cxx_name = "echec"]
        fn echec(self: Pin<&mut BadgeAnime>, apercu: bool, message: &QString);
    }
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{AspectRatioMode, QImage, QString, TransformationMode};

use crate::badge::{self, Composition, Visuel, AFFICHAGE, COTE, VISUELS_MAX};

/// Une image plus lourde est refusée avant même d'être décodée.
const POIDS_MAX_SOURCE: u64 = 30 * 1024 * 1024;

/// Numéro des fichiers provisoires : une adresse neuve à chaque badge, que
/// `AnimatedImage` relit (une même adresse resterait en cache).
static NUMERO: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub struct BadgeAnimeRust {
    occupe: bool,
    /// Demande arrivée pendant une fabrication.
    en_attente: Option<(String, bool)>,
    /// GIF provisoires écrits : les plus anciens s'effacent au fil de l'eau,
    /// les derniers avec l'objet.
    provisoires: Vec<PathBuf>,
}

impl Drop for BadgeAnimeRust {
    fn drop(&mut self) {
        for f in &self.provisoires {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// Décode l'image d'une adresse `file:` : Qt reconnaît le format à son
/// contenu. Ramenée d'abord à trois fois le côté du badge — de quoi recadrer
/// et agrandir une photo sans perte visible, sans lire pixel à pixel une
/// image de 24 millions de points.
fn lire_visuel(url: &str) -> Result<Visuel, String> {
    let chemin = crate::rendu::chemin_de_url(url).ok_or_else(|| format!("adresse d'image illisible : {url}"))?;
    let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let poids = std::fs::metadata(&chemin).map_err(|e| format!("« {nom} » : {e}"))?.len();
    if poids > POIDS_MAX_SOURCE {
        return Err(format!("« {nom} » dépasse 30 Mo"));
    }
    let octets = std::fs::read(&chemin).map_err(|e| format!("« {nom} » : {e}"))?;
    let image = QImage::from_data(&octets, None).ok_or_else(|| format!("« {nom} » n'est pas une image lisible"))?;
    let plus = 3 * COTE as i32;
    let image = if image.width() > plus || image.height() > plus {
        image.scaled(plus, plus, AspectRatioMode::KeepAspectRatio, TransformationMode::SmoothTransformation)
    } else {
        image
    };
    let (l, h) = (image.width(), image.height());
    if l <= 0 || h <= 0 {
        return Err(format!("« {nom} » est vide"));
    }
    let mut rgba = Vec::with_capacity((l * h * 4) as usize);
    for y in 0..h {
        for x in 0..l {
            let c = image.pixel_color(x, y);
            rgba.extend_from_slice(&[c.red() as u8, c.green() as u8, c.blue() as u8, c.alpha() as u8]);
        }
    }
    Ok(Visuel { largeur: l as u32, hauteur: h as u32, rgba })
}

impl qobject::BadgeAnime {
    pub fn fabriquer(mut self: Pin<&mut Self>, composition: &QString, apercu: bool) {
        let demande = (composition.to_string(), apercu);
        if *self.occupe() {
            let mut r = self.as_mut().rust_mut();
            let garder = apercu && r.en_attente.as_ref().is_some_and(|(_, a)| !a);
            if !garder {
                r.en_attente = Some(demande);
            }
            return;
        }
        self.lancer(demande);
    }

    fn lancer(mut self: Pin<&mut Self>, (texte, apercu): (String, bool)) {
        self.as_mut().set_occupe(true);
        let cible = std::env::temp_dir().join(format!(
            "mmail-badge-{}-{}.gif",
            std::process::id(),
            NUMERO.fetch_add(1, Ordering::Relaxed)
        ));
        let fil = self.qt_thread();
        let fichier = cible.clone();
        let lance = thread::Builder::new().name("mmail-badge".into()).spawn(move || {
            let resultat = serde_json::from_str::<Composition>(&texte)
                .map_err(|e| format!("composition illisible : {e}"))
                // Aperçu : couleurs vite réduites ; badge inséré : à la
                // vitesse que le Hub a retenue en production.
                .and_then(|c| badge::fabriquer(&c, lire_visuel, if apercu { 30 } else { 10 }))
                .and_then(|b| {
                    std::fs::write(&fichier, &b.gif).map_err(|e| format!("écriture du badge : {e}"))?;
                    Ok((b.images, b.gif.len()))
                });
            let _ = fil.queue(move |objet: Pin<&mut qobject::BadgeAnime>| objet.termine(fichier, apercu, resultat));
        });
        if lance.is_err() {
            self.as_mut().set_occupe(false);
            self.echec(apercu, &QString::from("fil de fabrication impossible à lancer"));
        }
    }

    fn termine(mut self: Pin<&mut Self>, fichier: PathBuf, apercu: bool, resultat: Result<(usize, usize), String>) {
        self.as_mut().set_occupe(false);
        match resultat {
            Ok((images, octets)) => {
                {
                    // Trois derniers gardés : celui affiché, celui qui le
                    // remplace, et un badge à insérer pas encore copié.
                    let mut r = self.as_mut().rust_mut();
                    r.provisoires.push(fichier.clone());
                    while r.provisoires.len() > 3 {
                        let _ = std::fs::remove_file(r.provisoires.remove(0));
                    }
                }
                let url = QString::from(&crate::rendu::url_fichier(&fichier));
                self.as_mut().pret(&url, apercu, images as i32, octets as i32);
            }
            Err(message) => {
                let _ = std::fs::remove_file(&fichier);
                self.as_mut().echec(apercu, &QString::from(&message));
            }
        }
        let suivante = self.as_mut().rust_mut().en_attente.take();
        if let Some(demande) = suivante {
            self.lancer(demande);
        }
    }

    pub fn reglages_par_defaut(&self) -> QString {
        QString::from(&serde_json::to_string(&badge::Reglages::default()).unwrap_or_default())
    }

    pub fn visuels_max(&self) -> i32 {
        VISUELS_MAX as i32
    }

    pub fn largeur_affichee(&self) -> i32 {
        AFFICHAGE as i32
    }
}
