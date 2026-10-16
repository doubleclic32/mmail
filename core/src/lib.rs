// SPDX-License-Identifier: GPL-3.0-or-later
//! Noyau de MMail : protocole IMAP, index local, et les objets exposés à QML.
pub mod agenda;
pub mod boite;
pub mod cache;
pub mod caldav;
pub mod configuration;
pub mod deplacement;
pub mod garde;
pub mod google;
pub mod http;
pub mod imap;
pub mod index;
pub mod invitation;
pub mod magasin;
pub mod mise_a_jour;
pub mod microsoft;
pub mod oauth;
pub mod mise_en_page;
pub mod pont_presse_papier;
pub mod poste;
pub mod presse_papier;
pub mod redaction;
pub mod rendu;
pub mod saisie;
pub mod sauvegarde;
pub mod signature;
pub mod smtp;
pub mod socle;
pub mod protocole;
pub mod synchro;

// Épreuves contre un vrai serveur : ignorées par défaut, elles exigent un
// compte fourni par l'environnement (cf. le module).
#[cfg(test)]
mod essais_serveur;

// Faux serveur IMAP : les mêmes enchaînements, sans compte ni réseau.
#[cfg(test)]
mod simule;
