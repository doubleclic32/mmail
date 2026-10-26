// SPDX-License-Identifier: GPL-3.0-or-later
//! Badge animé de la signature (0.6.17) : une option du module signature, pas
//! son comportement de base.
//!
//! Une image ronde ou carrée qui passe en boucle d'un visuel à l'autre — le
//! logo de la société, la photo de la personne, puis d'autres logos — par une
//! bascule en lamelles obliques sous un voile gris. Elle rejoint la signature
//! comme toute autre image (dossier `signatures/` du profil) et part avec
//! chaque message en image liée (`cid:`), réécrite à chaque envoi par
//! `redaction::integrer_images`.
//!
//! Moteur repris du générateur de signature de Guillaume LUDGER (Double Clic,
//! Hub v6 : `signatureAnim.ts` pour le dessin, `signature.rs` pour l'assemblage
//! GIF), dont il a permis la reprise sous GPL v3 le 22/10/2026. Mêmes réglages
//! par défaut, mêmes corrections (dossier de portage, § 9) : image entière en un
//! passage au repos, bascule sous le gris le plus dense, polygones qui
//! débordent, sources sur fond blanc, budget de 150 images. Le dessin passe du
//! canvas du navigateur à tiny-skia ; l'assemblage part d'images en mémoire,
//! sans PNG intermédiaire. Un seul code dessine l'aperçu et le badge inséré.

use serde::{Deserialize, Serialize};
use tiny_skia::{
    Color, ColorU8, FillRule, FilterQuality, IntRect, IntSize, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Rect, Stroke, Transform,
};

/// Côté du badge, en pixels : il s'affiche à `AFFICHAGE` dans la signature,
/// pour rester net sur un écran à forte densité.
pub const COTE: u32 = 208;

/// Largeur d'affichage dans la signature.
pub const AFFICHAGE: u32 = 138;

/// Au-delà, certaines passerelles refusent ou tronquent le GIF : le budget se
/// répartit sur le nombre de visuels, pas par transition.
const BUDGET_IMAGES: u32 = 150;

/// Nombre de visuels au plus : au-delà de 21, même six images de transition
/// par visuel dépassent le budget.
pub const VISUELS_MAX: usize = 21;

/// Gris du front de bascule.
const GRIS: (u8, u8, u8) = (206, 211, 217);

/// Anneau de contour, 3 pixels, `#cbd5e1`.
const ANNEAU: (u8, u8, u8) = (203, 213, 225);

/// Réglages de l'animation. Défauts du Hub de Double Clic, validés en
/// production ; bornés aux plages de son interface.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// Angle des lamelles, en degrés.
    pub angle: f64,
    /// Nombre de lamelles.
    pub lamelles: u32,
    /// Largeur du front de bascule, de 0 à 1.
    pub front: f64,
    /// Intensité du gris au front, de 0 à 1.
    pub gris: f64,
    /// Glissement d'une lamelle, en pixels.
    pub glissement: f64,
    /// Variation du glissement d'une lamelle à l'autre : l'effet déchiré.
    pub decalage: f64,
    /// Fentes permanentes entre lamelles, en pixels.
    pub fentes: f64,
    /// Pause sur chaque visuel, en millisecondes.
    pub pause: u32,
    /// Durée d'une image de transition, en millisecondes.
    pub transition: u32,
    /// Images par transition, avant le budget.
    pub etapes: u32,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            angle: 60.0,
            lamelles: 11,
            front: 0.42,
            gris: 0.85,
            glissement: 46.0,
            decalage: 0.9,
            fentes: 0.0,
            pause: 1100,
            transition: 38,
            etapes: 18,
        }
    }
}

impl Reglages {
    /// Les mêmes, ramenés dans leurs plages : un réglage hors bornes, ou un
    /// nombre illisible, ne fait pas un GIF démesuré.
    pub fn bornes(self) -> Self {
        let b = |v: f64, min: f64, max: f64, defaut: f64| if v.is_finite() { v.clamp(min, max) } else { defaut };
        let d = Reglages::default();
        Reglages {
            angle: b(self.angle, 20.0, 80.0, d.angle),
            lamelles: self.lamelles.clamp(4, 22),
            front: b(self.front, 0.12, 0.9, d.front),
            gris: b(self.gris, 0.0, 1.0, d.gris),
            glissement: b(self.glissement, 0.0, 120.0, d.glissement),
            decalage: b(self.decalage, 0.0, 2.0, d.decalage),
            fentes: b(self.fentes, 0.0, 6.0, d.fentes),
            pause: self.pause.clamp(300, 2500),
            transition: self.transition.clamp(20, 200),
            etapes: self.etapes.clamp(6, 30),
        }
    }
}

/// Forme du badge : le GIF la porte lui-même (fond transparent autour), parce
/// qu'Outlook ignore `border-radius`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Forme {
    #[default]
    Rond,
    Carre,
}

/// Une image décodée, en RGBA non prémultiplié.
#[derive(Clone, Debug)]
pub struct Visuel {
    pub largeur: u32,
    pub hauteur: u32,
    pub rgba: Vec<u8>,
}

/// Cadrage d'un visuel dans le carré du badge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cadrage {
    /// Photo : elle couvre le carré, agrandie de `zoom` et décalée de
    /// (`dx`, `dy`) pixels du badge.
    Photo { zoom: f64, dx: f64, dy: f64 },
    /// Logo : marges rognées, ajusté au plus juste dans la forme.
    Logo,
}

/// Visuel choisi dans la fenêtre de composition.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choix {
    /// Adresse `file:` de l'image.
    pub url: String,
    /// « photo » ou « logo ».
    pub genre: String,
    pub zoom: f64,
    pub dx: f64,
    pub dy: f64,
}

impl Choix {
    pub fn cadrage(&self) -> Cadrage {
        if self.genre == "photo" {
            let zoom = if self.zoom.is_finite() { self.zoom.clamp(1.0, 4.0) } else { 1.0 };
            let decale = |v: f64| if v.is_finite() { v.clamp(-f64::from(COTE), f64::from(COTE)) } else { 0.0 };
            Cadrage::Photo { zoom, dx: decale(self.dx), dy: decale(self.dy) }
        } else {
            Cadrage::Logo
        }
    }
}

/// Ce que la fenêtre de composition demande : les visuels dans l'ordre de la
/// boucle, la forme et les réglages.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Composition {
    pub visuels: Vec<Choix>,
    pub forme: Forme,
    pub reglages: Reglages,
}

/// Un badge fabriqué : le GIF et son nombre d'images.
#[derive(Clone, Debug)]
pub struct Badge {
    pub gif: Vec<u8>,
    pub images: usize,
}

fn lisse(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Facteur pseudo-aléatoire stable par lamelle, dans [-1, 1] : la même suite
/// que celle du Hub (mêmes calculs en `f64`).
fn alea(i: u32) -> f64 {
    let x = (f64::from(i + 1) * 127.1).sin() * 43758.5453;
    2.0 * (x - x.floor()) - 1.0
}

/// Contenu réel d'un logo, hors fond transparent ou quasi blanc : sa boîte,
/// son centre, et `rayon`, la distance du pixel visible le plus éloigné de ce
/// centre. Ce rayon fait tenir n'importe quelle forme dans un badge rond : un
/// logo rond le remplit, un carré s'y inscrit sans coin coupé, un badge aux
/// coins vides grandit d'autant.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Contenu {
    x: u32,
    y: u32,
    largeur: u32,
    hauteur: u32,
    cx: f64,
    cy: f64,
    rayon: f64,
}

fn contenu_du_logo(v: &Visuel) -> Contenu {
    let (l, h) = (v.largeur, v.hauteur);
    let entier = Contenu {
        x: 0,
        y: 0,
        largeur: l,
        hauteur: h,
        cx: f64::from(l) / 2.0,
        cy: f64::from(h) / 2.0,
        rayon: f64::from(l).hypot(f64::from(h)) / 2.0,
    };
    let visible = |x: u32, y: u32| {
        let i = ((y * l + x) * 4) as usize;
        let p = &v.rgba[i..i + 4];
        !(p[3] <= 12 || (p[0] > 244 && p[1] > 244 && p[2] > 244))
    };
    let (mut x0, mut y0, mut x1, mut y1) = (l, h, 0, 0);
    let mut vu = false;
    for y in 0..h {
        for x in 0..l {
            if visible(x, y) {
                vu = true;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if !vu {
        return entier;
    }
    let cx = f64::from(x0 + x1 + 1) / 2.0;
    let cy = f64::from(y0 + y1 + 1) / 2.0;
    let mut r2: f64 = 1.0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if visible(x, y) {
                let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
                r2 = r2.max(dx * dx + dy * dy);
            }
        }
    }
    Contenu { x: x0, y: y0, largeur: x1 - x0 + 1, hauteur: y1 - y0 + 1, cx, cy, rayon: r2.sqrt() }
}

fn en_pixmap(v: &Visuel) -> Option<Pixmap> {
    let taille = IntSize::from_wh(v.largeur, v.hauteur)?;
    if v.rgba.len() != (v.largeur * v.hauteur * 4) as usize {
        return None;
    }
    let mut donnees = Vec::with_capacity(v.rgba.len());
    for p in v.rgba.chunks_exact(4) {
        let c = ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
        donnees.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Pixmap::from_vec(donnees, taille)
}

/// Compose un visuel dans le carré du badge, sur fond blanc : un logo blanc
/// sur transparent reste visible, et la transparence n'est posée qu'au
/// découpage final de la forme.
pub fn composer(v: &Visuel, cadrage: Cadrage, forme: Forme) -> Option<Pixmap> {
    let mut sortie = Pixmap::new(COTE, COTE)?;
    sortie.fill(Color::WHITE);
    let source = en_pixmap(v)?;
    let peinture = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
    let m = f64::from(COTE) / 2.0;
    let (l, h) = (f64::from(v.largeur), f64::from(v.hauteur));
    let cote = f64::from(COTE);
    match (cadrage, forme) {
        (Cadrage::Photo { zoom, dx, dy }, _) => {
            let s = (cote / l).max(cote / h) * zoom;
            let (x, y) = (m + dx - l * s / 2.0, m + dy - h * s / 2.0);
            sortie.draw_pixmap(0, 0, source.as_ref(), &peinture, Transform::from_row(s as f32, 0.0, 0.0, s as f32, x as f32, y as f32), None);
        }
        (Cadrage::Logo, Forme::Rond) => {
            // Le pixel visible le plus loin atterrit à 98 % du rayon.
            let c = contenu_du_logo(v);
            let s = cote * 0.49 * 0.98 / c.rayon;
            let (x, y) = (m - c.cx * s, m - c.cy * s);
            sortie.draw_pixmap(0, 0, source.as_ref(), &peinture, Transform::from_row(s as f32, 0.0, 0.0, s as f32, x as f32, y as f32), None);
        }
        (Cadrage::Logo, Forme::Carre) => {
            let c = contenu_du_logo(v);
            let rogne = source.clone_rect(IntRect::from_xywh(c.x as i32, c.y as i32, c.largeur, c.hauteur)?)?;
            let (cl, ch) = (f64::from(c.largeur), f64::from(c.hauteur));
            let s = (cote * 0.92 / cl).min(cote * 0.92 / ch);
            let (x, y) = (m - cl * s / 2.0, m - ch * s / 2.0);
            sortie.draw_pixmap(0, 0, rogne.as_ref(), &peinture, Transform::from_row(s as f32, 0.0, 0.0, s as f32, x as f32, y as f32), None);
        }
    }
    Some(sortie)
}

fn chemin_forme(forme: Forme) -> Option<Path> {
    let c = COTE as f32;
    match forme {
        Forme::Rond => PathBuilder::from_circle(c / 2.0, c / 2.0, c * 0.49),
        Forme::Carre => Some(PathBuilder::from_rect(Rect::from_xywh(2.0, 2.0, c - 4.0, c - 4.0)?)),
    }
}

fn masque_forme(forme: Forme) -> Option<Mask> {
    let mut masque = Mask::new(COTE, COTE)?;
    masque.fill_path(&chemin_forme(forme)?, FillRule::Winding, true, Transform::identity());
    Some(masque)
}

/// L'anneau de contour, découpé par la forme comme le reste.
fn anneau(sortie: &mut Pixmap, forme: Forme, masque: &Mask) {
    let c = COTE as f32;
    let chemin = match forme {
        Forme::Rond => PathBuilder::from_circle(c / 2.0, c / 2.0, c * 0.49),
        Forme::Carre => Rect::from_xywh(1.5, 1.5, c - 3.0, c - 3.0).map(PathBuilder::from_rect),
    };
    let Some(chemin) = chemin else { return };
    let mut peinture = Paint::default();
    peinture.set_color_rgba8(ANNEAU.0, ANNEAU.1, ANNEAU.2, 255);
    let trait_ = Stroke { width: 3.0, ..Stroke::default() };
    sortie.stroke_path(&chemin, &peinture, &trait_, Transform::identity(), Some(masque));
}

/// Une image de l'animation : la bascule du visuel `a` vers `b` à la
/// progression `p` (de 0 à 1).
pub fn image(a: &Pixmap, b: &Pixmap, p: f64, forme: Forme, r: &Reglages) -> Option<Pixmap> {
    let mut sortie = Pixmap::new(COTE, COTE)?;
    let masque = masque_forme(forme)?;
    // Au repos, l'image entière en un seul passage : des lamelles jointives
    // laisseraient des coutures fines (bords anticrénelés), visibles sur une
    // photo même sans fente — et ce sont les images que l'on regarde le plus
    // longtemps. Correction visuelle, pas optimisation (dossier de portage, § 9.1).
    if p <= 0.0 || p >= 1.0 {
        let visuel = if p >= 1.0 { b } else { a };
        sortie.draw_pixmap(0, 0, visuel.as_ref(), &PixmapPaint::default(), Transform::identity(), Some(&masque));
        anneau(&mut sortie, forme, &masque);
        return Some(sortie);
    }
    let cote = f64::from(COTE);
    let th = r.angle.to_radians();
    let (ax, ay) = (th.cos(), -th.sin()); // le long des lamelles
    let (px, py) = (th.sin(), th.cos()); // en travers
    let (mut vmin, mut vmax) = (f64::MAX, f64::MIN);
    for (x, y) in [(0.0, 0.0), (cote, 0.0), (0.0, cote), (cote, cote)] {
        let v = (x - cote / 2.0) * px + (y - cote / 2.0) * py;
        vmin = vmin.min(v);
        vmax = vmax.max(v);
    }
    let etendue = vmax - vmin;
    // Les polygones débordent largement du carré : ajustés à lui, les
    // lamelles obliques laisseraient des coins nus (§ 9.3).
    let longueur = cote * 1.6;
    let nb = r.lamelles.max(1);
    let glisse = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    let mut voile = Paint::default();
    for i in 0..nb {
        let part = (f64::from(i) + 0.5) / f64::from(nb);
        let local = ((p * (1.0 + r.front) - part) / r.front).clamp(0.0, 1.0);
        let e = lisse(local);
        let gris = (std::f64::consts::PI * e).sin() * r.gris;
        let d = r.glissement * (std::f64::consts::PI * e).sin() * (1.0 + r.decalage * alea(i));
        // Le contenu change au milieu, sous le gris le plus dense (§ 9.2).
        let contenu = if e < 0.5 { a } else { b };
        let vc = vmin + etendue * part;
        let demi = etendue / f64::from(nb) / 2.0 - r.fentes / 2.0;
        if demi <= 0.0 {
            continue;
        }
        let (mx, my) = (cote / 2.0 + vc * px, cote / 2.0 + vc * py);
        let mut pb = PathBuilder::new();
        let point = |sa: f64, sp: f64| ((mx + sa * longueur * ax + sp * demi * px) as f32, (my + sa * longueur * ay + sp * demi * py) as f32);
        let (x, y) = point(-1.0, -1.0);
        pb.move_to(x, y);
        for (sa, sp) in [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let (x, y) = point(sa, sp);
            pb.line_to(x, y);
        }
        pb.close();
        let Some(lamelle) = pb.finish() else { continue };
        let mut decoupe = masque.clone();
        decoupe.intersect_path(&lamelle, FillRule::Winding, true, Transform::identity());
        sortie.draw_pixmap(0, 0, contenu.as_ref(), &glisse, Transform::from_translate((d * ax) as f32, (d * ay) as f32), Some(&decoupe));
        if gris > 0.0 {
            voile.set_color_rgba8(GRIS.0, GRIS.1, GRIS.2, (gris.clamp(0.0, 1.0) * 255.0).round() as u8);
            if let Some(tout) = Rect::from_xywh(0.0, 0.0, cote as f32, cote as f32) {
                sortie.fill_rect(tout, &voile, Transform::identity(), Some(&decoupe));
            }
        }
    }
    anneau(&mut sortie, forme, &masque);
    Some(sortie)
}

/// Nombre d'images de transition entre deux visuels, borné par le budget.
fn etapes(visuels: usize, r: &Reglages) -> u32 {
    if visuels <= 1 {
        0
    } else {
        (BUDGET_IMAGES / visuels as u32).saturating_sub(1).min(r.etapes).max(6)
    }
}

/// Toutes les images de la boucle, avec leur délai en centisecondes : pour
/// chaque visuel, un palier puis la bascule vers le suivant.
pub fn images(sources: &[Pixmap], forme: Forme, r: &Reglages) -> Vec<(Pixmap, u16)> {
    let n = sources.len();
    // En dessous de 2 centisecondes, les clients de messagerie imposent leur
    // propre minimum et l'animation perd sa cadence.
    let centi = |ms: u32| ((f64::from(ms) / 10.0).round() as u16).max(2);
    let (pause, transition) = (centi(r.pause), centi(r.transition));
    let tf = etapes(n, r);
    let mut sortie = Vec::new();
    for i in 0..n {
        let (a, b) = (&sources[i], &sources[(i + 1) % n]);
        if let Some(palier) = image(a, a, 0.0, forme, r) {
            sortie.push((palier, pause));
        }
        if n == 1 {
            break;
        }
        for k in 1..=tf {
            if let Some(im) = image(a, b, f64::from(k) / f64::from(tf + 1), forme, r) {
                sortie.push((im, transition));
            }
        }
    }
    sortie
}

/// Pixels d'une image, non prémultipliés ; transparence tout ou rien, comme
/// le GIF : un pixel à peine couvert (bord anticrénelé) compte pour opaque.
fn pixels_gif(image: &Pixmap) -> Vec<[u8; 4]> {
    image
        .pixels()
        .iter()
        .map(|p| {
            let c = p.demultiply();
            if c.alpha() == 0 { [0, 0, 0, 0] } else { [c.red(), c.green(), c.blue(), 255] }
        })
        .collect()
}

/// Rectangle (gauche, haut, largeur, hauteur) qui contient les pixels marqués.
fn rectangle(marques: &[bool], cote: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (cote, cote, 0, 0);
    for (i, _) in marques.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i % cote, i / cote);
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    (x0 <= x1).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// Assemble les images en GIF animé, en boucle. Chaque image ne porte que les
/// pixels qui changent depuis la précédente, dans le plus petit rectangle qui
/// les contient, le reste transparent — l'image d'avant reste en place (0.6.18 :
/// le badge de deux photos passait de 940 Ko à moins de la moitié). Un pixel
/// qui doit redevenir transparent — le bord d'une lamelle qui glisse — fait
/// effacer la zone de l'image d'avant, qui est alors redessinée. La palette de
/// chaque image est calculée sur ses seuls pixels dessinés. `vitesse` :
/// quantification des couleurs, de 1 (meilleure palette) à 30 (plus rapide).
pub fn assembler(images: &[(Pixmap, u16)], vitesse: i32) -> Result<Vec<u8>, String> {
    let cote = COTE as usize;
    let cibles: Vec<Vec<[u8; 4]>> = images.iter().map(|(im, _)| pixels_gif(im)).collect();
    let n = cibles.len();
    // À effacer avant l'image k : opaque dans l'image d'avant, transparent dans
    // celle-ci. L'image 0 suit la dernière, à chaque tour de la boucle.
    let a_effacer = |k: usize| -> Vec<bool> {
        let (avant, ici) = (&cibles[(k + n - 1) % n], &cibles[k]);
        avant.iter().zip(ici).map(|(a, b)| a[3] != 0 && b[3] == 0).collect()
    };
    let mut sortie = Vec::new();
    {
        let mut gif = gif::Encoder::new(&mut sortie, COTE as u16, COTE as u16, &[]).map_err(|e| format!("GIF : {e}"))?;
        gif.set_repeat(gif::Repeat::Infinite).map_err(|e| format!("GIF : {e}"))?;
        // Zone effacée après l'image précédente : tout, avant la première.
        let mut efface = vec![true; cote * cote];
        for (k, cible) in cibles.iter().enumerate() {
            let avant = &cibles[(k + n - 1) % n];
            let dessines: Vec<bool> = (0..cote * cote)
                .map(|i| cible[i][3] != 0 && (efface[i] || (k > 0 && cible[i] != avant[i])))
                .collect();
            let suivant = a_effacer((k + 1) % n);
            let efface_apres = suivant.iter().any(|e| *e);
            let zone: Vec<bool> = dessines.iter().zip(&suivant).map(|(d, s)| *d || *s).collect();
            let (x, y, l, h) = rectangle(&zone, cote).unwrap_or((0, 0, 1, 1));
            // Palette des seuls pixels dessinés : 255 couleurs, la 256e est la
            // transparence.
            let couleurs: Vec<u8> = (0..cote * cote).filter(|i| dessines[*i]).flat_map(|i| cible[i]).collect();
            let distinctes: std::collections::BTreeSet<[u8; 4]> = couleurs.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
            let (palette, indice): (Vec<u8>, Box<dyn Fn(&[u8; 4]) -> u8>) = if distinctes.len() <= 255 {
                let liste: Vec<[u8; 4]> = distinctes.into_iter().collect();
                let palette = liste.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
                (palette, Box::new(move |c: &[u8; 4]| liste.binary_search(c).map(|i| i as u8).unwrap_or(0)))
            } else {
                let nq = color_quant::NeuQuant::new(vitesse.clamp(1, 30), 255, &couleurs);
                (nq.color_map_rgb(), Box::new(move |c: &[u8; 4]| nq.index_of(c) as u8))
            };
            let transparent = (palette.len() / 3) as u8;
            let mut tampon = Vec::with_capacity(l * h);
            for yy in y..y + h {
                for xx in x..x + l {
                    let i = yy * cote + xx;
                    tampon.push(if dessines[i] { indice(&cible[i]) } else { transparent });
                }
            }
            let mut palette = palette;
            palette.extend_from_slice(&[0, 0, 0]);
            let trame = gif::Frame {
                left: x as u16,
                top: y as u16,
                width: l as u16,
                height: h as u16,
                buffer: std::borrow::Cow::Owned(tampon),
                palette: Some(palette),
                transparent: Some(transparent),
                delay: images[k].1,
                // Effacer derrière soi seulement quand l'image suivante a des
                // pixels à rendre transparents ; sinon, garder.
                dispose: if efface_apres { gif::DisposalMethod::Background } else { gif::DisposalMethod::Keep },
                ..gif::Frame::default()
            };
            gif.write_frame(&trame).map_err(|e| format!("GIF : {e}"))?;
            efface = vec![false; cote * cote];
            if efface_apres {
                for yy in y..y + h {
                    for xx in x..x + l {
                        efface[yy * cote + xx] = true;
                    }
                }
            }
        }
    }
    Ok(sortie)
}

/// Fabrique le badge d'une composition. `lire` décode l'image d'une adresse
/// `file:` ; `vitesse`, cf. `assembler`.
pub fn fabriquer(composition: &Composition, lire: impl Fn(&str) -> Result<Visuel, String>, vitesse: i32) -> Result<Badge, String> {
    if composition.visuels.is_empty() {
        return Err("aucune image dans le badge".into());
    }
    if composition.visuels.len() > VISUELS_MAX {
        return Err(format!("{VISUELS_MAX} images au plus dans un badge"));
    }
    let r = composition.reglages.bornes();
    let mut sources = Vec::with_capacity(composition.visuels.len());
    for choix in &composition.visuels {
        let visuel = lire(&choix.url)?;
        sources.push(composer(&visuel, choix.cadrage(), composition.forme).ok_or("image illisible")?);
    }
    let images = images(&sources, composition.forme, &r);
    let gif = assembler(&images, vitesse)?;
    Ok(Badge { gif, images: images.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Visuel uni.
    fn uni(l: u32, h: u32, rgba: [u8; 4]) -> Visuel {
        Visuel { largeur: l, hauteur: h, rgba: rgba.repeat((l * h) as usize) }
    }

    /// Disque de couleur `rgba` sur fond transparent, de rayon `r`, dans un
    /// carré de côté `c`.
    fn disque(c: u32, r: f64, rgba: [u8; 4]) -> Visuel {
        let mut v = uni(c, c, [0, 0, 0, 0]);
        let m = f64::from(c) / 2.0;
        for y in 0..c {
            for x in 0..c {
                if (f64::from(x) + 0.5 - m).hypot(f64::from(y) + 0.5 - m) <= r {
                    let i = ((y * c + x) * 4) as usize;
                    v.rgba[i..i + 4].copy_from_slice(&rgba);
                }
            }
        }
        v
    }

    fn pixel(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = p.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn suite_pseudo_aleatoire_du_hub() {
        // Valeurs de `jitter` calculées par node (même calcul en f64) : les
        // lamelles glissent comme dans le Hub.
        for (i, js) in [(0, -0.34877714588947129), (1, 0.6180793925159378), (2, 0.45235397027863655), (10, 0.47423375733160356), (21, -0.34728005858232791)] {
            assert!((alea(i) - js).abs() < 1e-9, "{i} : {} au lieu de {js}", alea(i));
        }
        for i in 0..22 {
            assert!((-1.0..=1.0).contains(&alea(i)), "{i}");
        }
    }

    #[test]
    fn contenu_d_un_logo() {
        // Disque de rayon 40 au centre d'un carré de 200 aux marges vides.
        let c = contenu_du_logo(&disque(200, 40.0, [200, 0, 0, 255]));
        assert!((c.cx - 100.0).abs() < 1.0 && (c.cy - 100.0).abs() < 1.0, "{c:?}");
        assert!((c.rayon - 40.0).abs() < 1.5, "{c:?}");
        assert!(c.largeur <= 81 && c.largeur >= 79, "{c:?}");
        // Un carré plein : la demi-diagonale.
        let carre = contenu_du_logo(&uni(50, 50, [0, 0, 200, 255]));
        assert!((carre.rayon - 50.0 * std::f64::consts::SQRT_2 / 2.0).abs() < 1.5, "{carre:?}");
        // Rien de visible (blanc) : l'image entière.
        let blanc = contenu_du_logo(&uni(30, 20, [255, 255, 255, 255]));
        assert_eq!((blanc.largeur, blanc.hauteur), (30, 20));
    }

    #[test]
    fn logo_rond_remplit_le_badge() {
        // Un petit disque perdu dans ses marges remplit le cercle du badge.
        let s = composer(&disque(400, 30.0, [0, 120, 0, 255]), Cadrage::Logo, Forme::Rond).unwrap();
        let m = COTE / 2;
        assert_eq!(pixel(&s, m, m), [0, 120, 0, 255]);
        // À 95 % du rayon du badge : encore le logo.
        let bord = (f64::from(COTE) * 0.49 * 0.95) as u32;
        let p = pixel(&s, m + bord, m);
        assert!(p[0] < 20 && (110..=135).contains(&p[1]) && p[2] < 20, "{p:?}");
        // Dans le coin : le fond blanc.
        assert_eq!(pixel(&s, 2, 2), [255, 255, 255, 255]);
    }

    #[test]
    fn photo_couvre_le_carre() {
        let s = composer(&uni(300, 100, [10, 20, 30, 255]), Cadrage::Photo { zoom: 1.0, dx: 0.0, dy: 0.0 }, Forme::Carre).unwrap();
        for (x, y) in [(0, 0), (COTE - 1, 0), (0, COTE - 1), (COTE - 1, COTE - 1), (COTE / 2, COTE / 2)] {
            assert_eq!(pixel(&s, x, y), [10, 20, 30, 255], "({x}, {y})");
        }
    }

    #[test]
    fn repos_et_transition() {
        let a = composer(&uni(10, 10, [255, 0, 0, 255]), Cadrage::Photo { zoom: 1.0, dx: 0.0, dy: 0.0 }, Forme::Rond).unwrap();
        let b = composer(&uni(10, 10, [0, 0, 255, 255]), Cadrage::Photo { zoom: 1.0, dx: 0.0, dy: 0.0 }, Forme::Rond).unwrap();
        let r = Reglages::default();
        let m = COTE / 2;
        let debut = image(&a, &b, 0.0, Forme::Rond, &r).unwrap();
        let fin = image(&a, &b, 1.0, Forme::Rond, &r).unwrap();
        assert_eq!(pixel(&debut, m, m), [255, 0, 0, 255]);
        assert_eq!(pixel(&fin, m, m), [0, 0, 255, 255]);
        // Hors du cercle : transparent ; sur l'anneau : son gris.
        assert_eq!(pixel(&debut, 1, 1)[3], 0);
        // Sur l'anneau (rayon 101,9, trait de 3 dont la moitié intérieure
        // seule reste) : son gris bleuté.
        let anneau = pixel(&debut, m + 101, m);
        assert!(anneau[0] > 170 && anneau[1] > 170 && anneau[2] > 190, "{anneau:?}");
        // Au milieu de la bascule : du gris, et les deux couleurs.
        let milieu = image(&a, &b, 0.5, Forme::Rond, &r).unwrap();
        let (mut rouge, mut bleu, mut gris) = (false, false, false);
        for y in (20..COTE - 20).step_by(4) {
            for x in (20..COTE - 20).step_by(4) {
                let p = pixel(&milieu, x, y);
                rouge |= p[0] > 200 && p[2] < 60;
                bleu |= p[2] > 200 && p[0] < 60;
                gris |= p[0] > 120 && p[2] > 120 && p[1] > 120;
            }
        }
        assert!(rouge && bleu && gris, "rouge {rouge}, bleu {bleu}, gris {gris}");
        // Forme carrée : seuls les deux pixels du bord sont transparents.
        let carre = image(&a, &a, 0.0, Forme::Carre, &r).unwrap();
        assert_eq!(pixel(&carre, 0, 0)[3], 0);
        assert_eq!(pixel(&carre, 10, 10), [255, 0, 0, 255]);
    }

    #[test]
    fn budget_d_images() {
        let r = Reglages::default();
        assert_eq!(etapes(1, &r), 0);
        assert_eq!(etapes(2, &r), 18);
        assert_eq!(etapes(12, &r), 11);
        assert_eq!(etapes(21, &r), 6);
        // 21 visuels : 21 × 7 = 147 images, sous le budget.
        assert!(21 * (1 + etapes(21, &r)) <= BUDGET_IMAGES);
        let source = composer(&uni(4, 4, [0, 0, 0, 255]), Cadrage::Logo, Forme::Rond).unwrap();
        let une = images(std::slice::from_ref(&source), Forme::Rond, &r);
        assert_eq!(une.len(), 1);
        assert_eq!(une[0].1, 110);
        let trois = images(&[source.clone(), source.clone(), source], Forme::Rond, &r);
        assert_eq!(trois.len(), 3 * 19);
        assert_eq!(trois[1].1, 4);
    }

    #[test]
    fn reglages_bornes() {
        let fou = Reglages { angle: f64::NAN, lamelles: 500, front: 0.0, pause: 0, etapes: 999, ..Reglages::default() }.bornes();
        assert_eq!(fou.angle, 60.0);
        assert_eq!(fou.lamelles, 22);
        assert_eq!(fou.front, 0.12);
        assert_eq!(fou.pause, 300);
        assert_eq!(fou.etapes, 30);
        let lu: Composition = serde_json::from_str(r#"{"forme":"carre","visuels":[{"url":"file:///x.png","genre":"photo","zoom":2}]}"#).unwrap();
        assert_eq!(lu.forme, Forme::Carre);
        assert_eq!(lu.reglages, Reglages::default());
        assert_eq!(lu.visuels[0].cadrage(), Cadrage::Photo { zoom: 2.0, dx: 0.0, dy: 0.0 });
    }

    /// Décode un GIF : nombre d'images, délais, côté, et répétition.
    fn relire(gif: &[u8]) -> (usize, Vec<u16>, (u16, u16), bool) {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut d = options.read_info(gif).unwrap();
        let cote = (d.width(), d.height());
        let mut delais = Vec::new();
        while let Some(trame) = d.read_next_frame().unwrap() {
            delais.push(trame.delay);
        }
        let boucle = d.repeat() == gif::Repeat::Infinite;
        (delais.len(), delais, cote, boucle)
    }

    #[test]
    fn badge_complet() {
        let photo = {
            // Dégradé, comme une photo : plus de 256 couleurs.
            let mut v = uni(320, 240, [0, 0, 0, 255]);
            for y in 0..240u32 {
                for x in 0..320u32 {
                    let i = ((y * 320 + x) * 4) as usize;
                    v.rgba[i] = (x * 255 / 319) as u8;
                    v.rgba[i + 1] = (y * 255 / 239) as u8;
                    v.rgba[i + 2] = ((x + y) % 256) as u8;
                }
            }
            v
        };
        let lire = |url: &str| match url {
            "file:///logo.png" => Ok(disque(300, 120.0, [33, 171, 227, 255])),
            "file:///photo.jpg" => Ok(photo.clone()),
            "file:///carre.png" => Ok(uni(64, 64, [230, 30, 41, 255])),
            _ => Err(format!("{url} introuvable")),
        };
        let composition = Composition {
            visuels: vec![
                Choix { url: "file:///logo.png".into(), genre: "logo".into(), ..Choix::default() },
                Choix { url: "file:///photo.jpg".into(), genre: "photo".into(), zoom: 1.3, dx: 10.0, ..Choix::default() },
                Choix { url: "file:///carre.png".into(), genre: "logo".into(), ..Choix::default() },
            ],
            forme: Forme::Rond,
            reglages: Reglages::default(),
        };
        let badge = fabriquer(&composition, lire, 10).unwrap();
        assert_eq!(&badge.gif[..6], b"GIF89a");
        assert_eq!(badge.images, 57);
        let (n, delais, cote, boucle) = relire(&badge.gif);
        assert_eq!(n, 57);
        assert_eq!(cote, (COTE as u16, COTE as u16));
        assert!(boucle);
        assert_eq!(delais[0], 110);
        assert_eq!(delais[1], 4);
        assert_eq!(delais[19], 110);
        // Un poids de signature : quelques centaines de kilo-octets au plus.
        eprintln!("badge de 3 visuels : {} images, {} octets", badge.images, badge.gif.len());
        assert!(badge.gif.len() < 900 * 1024, "{} octets", badge.gif.len());

        // Erreurs : rien, trop, ou une image introuvable.
        assert!(fabriquer(&Composition::default(), lire, 10).is_err());
        let trop = Composition { visuels: vec![composition.visuels[0].clone(); VISUELS_MAX + 1], ..composition.clone() };
        assert!(fabriquer(&trop, lire, 10).unwrap_err().contains("21"));
        let absente = Composition { visuels: vec![Choix { url: "file:///nulle.png".into(), ..Choix::default() }], ..composition };
        assert!(fabriquer(&absente, lire, 10).unwrap_err().contains("introuvable"));
    }

    /// L'assemblage d'avant la 0.6.18 : chaque image entière, palette propre,
    /// effacée d'une image à l'autre. Référence de la mesure du poids.
    fn assembler_plein(images: &[(Pixmap, u16)], vitesse: i32) -> Vec<u8> {
        let cote = COTE as u16;
        let mut sortie = Vec::new();
        {
            let mut gif = gif::Encoder::new(&mut sortie, cote, cote, &[]).unwrap();
            gif.set_repeat(gif::Repeat::Infinite).unwrap();
            for (image, delai) in images {
                let mut rgba: Vec<u8> = image.pixels().iter().flat_map(|p| {
                    let c = p.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                }).collect();
                let mut trame = gif::Frame::from_rgba_speed(cote, cote, &mut rgba, vitesse);
                trame.delay = *delai;
                trame.dispose = gif::DisposalMethod::Background;
                gif.write_frame(&trame).unwrap();
            }
        }
        sortie
    }

    /// Rejoue un GIF comme un navigateur — rectangle de chaque image, index
    /// transparent, effacement — et rend chaque image affichée, en RGBA, sur
    /// `tours` tours de boucle sans repartir d'un écran vide (le cas le moins
    /// favorable : un lecteur qui garde l'écran d'un tour à l'autre).
    fn rejouer(gif: &[u8], tours: usize) -> Vec<Vec<[u8; 4]>> {
        let mut ecran = Vec::new();
        let mut vues = Vec::new();
        let mut a_effacer: Option<(usize, usize, usize, usize)> = None;
        for _ in 0..tours {
            let mut options = gif::DecodeOptions::new();
            options.set_color_output(gif::ColorOutput::Indexed);
            let mut d = options.read_info(gif).unwrap();
            let l = d.width() as usize;
            ecran.resize(l * d.height() as usize, [0u8; 4]);
            let globale = d.global_palette().map(|p| p.to_vec());
            while let Some(t) = d.read_next_frame().unwrap() {
                if let Some((x, y, w, h)) = a_effacer.take() {
                    for yy in y..y + h {
                        for xx in x..x + w {
                            ecran[yy * l + xx] = [0, 0, 0, 0];
                        }
                    }
                }
                let palette = t.palette.clone().or_else(|| globale.clone()).unwrap();
                let (x, y, w, h) = (t.left as usize, t.top as usize, t.width as usize, t.height as usize);
                for yy in 0..h {
                    for xx in 0..w {
                        let i = t.buffer[yy * w + xx];
                        if Some(i) != t.transparent {
                            let c = &palette[i as usize * 3..i as usize * 3 + 3];
                            ecran[(y + yy) * l + x + xx] = [c[0], c[1], c[2], 255];
                        }
                    }
                }
                vues.push(ecran.clone());
                if t.dispose == gif::DisposalMethod::Background {
                    a_effacer = Some((x, y, w, h));
                }
            }
        }
        vues
    }

    #[test]
    fn assemblage_differentiel_fidele() {
        // Deux photos et un logo : chaque image rejouée a la transparence exacte
        // de la sienne et des couleurs proches, et le GIF est bien plus léger
        // que l'assemblage d'avant.
        let mut photo = uni(300, 220, [0, 0, 0, 255]);
        for (i, p) in photo.rgba.chunks_exact_mut(4).enumerate() {
            let (x, y) = ((i % 300) as u32, (i / 300) as u32);
            let bruit = (alea(x * 7919 + y * 104729) * 60.0) as u32;
            p[0] = ((x * 255 / 299 + bruit) % 256) as u8;
            p[1] = ((y * 255 / 219 + bruit / 2) % 256) as u8;
            p[2] = (((x + 2 * y) / 3 + bruit) % 256) as u8;
        }
        let sources = [
            composer(&photo, Cadrage::Photo { zoom: 1.0, dx: 0.0, dy: 0.0 }, Forme::Rond).unwrap(),
            composer(&disque(300, 120.0, [33, 171, 227, 255]), Cadrage::Logo, Forme::Rond).unwrap(),
            composer(&photo, Cadrage::Photo { zoom: 1.6, dx: -30.0, dy: 12.0 }, Forme::Rond).unwrap(),
        ];
        for r in [Reglages::default(), Reglages { fentes: 4.0, ..Reglages::default() }] {
            let liste = images(&sources, Forme::Rond, &r);
            let gif = assembler(&liste, 10).unwrap();
            let plein = assembler_plein(&liste, 10);
            let vues = rejouer(&gif, 2);
            assert_eq!(vues.len(), 2 * liste.len());
            for (k, (vue, (im, _))) in vues.iter().zip(liste.iter().chain(&liste)).enumerate() {
                let cible = pixels_gif(im);
                let mut ecart = 0u64;
                for (a, b) in vue.iter().zip(&cible) {
                    assert_eq!(a[3], b[3], "image {k} : transparence");
                    ecart += (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
                }
                let moyen = ecart as f64 / (cible.len() * 3) as f64;
                assert!(moyen < 4.0, "image {k} : écart moyen {moyen:.2}");
            }
            eprintln!("fentes {} : {} images, {} octets (avant : {})", r.fentes, liste.len(), gif.len(), plein.len());
            assert!(gif.len() * 10 < plein.len() * 8, "{} contre {}", gif.len(), plein.len());
        }
    }

    /// Mesure du poids, à la demande (`cargo test -- --ignored mesure`) :
    /// `MMAIL_BADGE_MESURE` = `fichier.rgba:largeur:hauteur:genre`, séparés
    /// par des virgules, RGBA brut.
    #[test]
    #[ignore]
    fn mesure_du_poids() {
        let Ok(liste) = std::env::var("MMAIL_BADGE_MESURE") else { return };
        let mut sources = Vec::new();
        for e in liste.split(',') {
            let p: Vec<&str> = e.split(':').collect();
            let v = Visuel { largeur: p[1].parse().unwrap(), hauteur: p[2].parse().unwrap(), rgba: std::fs::read(p[0]).unwrap() };
            let cadrage = if p[3] == "photo" { Cadrage::Photo { zoom: 1.0, dx: 0.0, dy: 0.0 } } else { Cadrage::Logo };
            sources.push(composer(&v, cadrage, Forme::Rond).unwrap());
        }
        for etapes in [18, 14, 12, 10] {
            let r = Reglages { etapes, ..Reglages::default() };
            let im = images(&sources, Forme::Rond, &r);
            let t0 = std::time::Instant::now();
            let nouveau = assembler(&im, 10).unwrap();
            let duree = t0.elapsed();
            let plein = assembler_plein(&im, 10);
            eprintln!("étapes {etapes:2} : {} images, avant {} o, après {} o ({:.0} %), {:?}",
                im.len(), plein.len(), nouveau.len(), 100.0 * nouveau.len() as f64 / plein.len() as f64, duree);
        }
    }

    #[test]
    fn badge_integre_au_message() {
        // Le GIF rejoint le dossier des signatures, et part dans le message
        // comme image liée de type image/gif, octets intacts.
        let lire = |_: &str| Ok(uni(40, 40, [11, 22, 33, 255]));
        let composition = Composition {
            visuels: vec![Choix { url: "file:///a.png".into(), genre: "logo".into(), ..Choix::default() }; 2],
            ..Composition::default()
        };
        let badge = fabriquer(&composition, lire, 30).unwrap();
        let dossier = std::env::temp_dir().join(format!("mmail-badge-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let fichier = dossier.join("1790000000-1.gif");
        std::fs::write(&fichier, &badge.gif).unwrap();
        let url = crate::rendu::url_fichier(&fichier);
        let html = format!("<p>Cordialement</p><img src=\"{url}\" width=\"{AFFICHAGE}\" height=\"{AFFICHAGE}\" />");
        let (html, liees) = crate::redaction::integrer_images(&html, |adresse| {
            let chemin = crate::rendu::chemin_de_url(adresse)?;
            Some((crate::rendu::type_image(&chemin.to_string_lossy())?.to_string(), std::fs::read(chemin).ok()?))
        });
        assert_eq!(liees.len(), 1);
        assert_eq!(liees[0].type_mime, "image/gif");
        assert_eq!(liees[0].contenu, badge.gif);
        assert!(html.contains(&format!("src=\"cid:{}\"", liees[0].cid)), "{html}");
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
