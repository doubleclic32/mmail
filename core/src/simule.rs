// SPDX-License-Identifier: GPL-3.0-or-later
//! Faux serveur IMAP pour les tests : il rejoue un dialogue écrit d'avance et
//! consigne les commandes reçues.
//!
//! Les épreuves contre un vrai serveur (`essais_serveur`) exigent un compte ;
//! celles-ci tournent partout, intégration continue comprise. Chaque échange
//! attendu donne le début de la commande (sans son étiquette), les réponses
//! non étiquetées, et la fin (« OK … », « NO … ») que le faux serveur fait
//! précéder de l'étiquette de la commande. Une commande qui ne commence pas
//! comme prévu fait échouer le test.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use crate::imap::{Client, Flux};

/// Une commande attendue et ce que le serveur y répond.
pub struct Echange {
    pub attendu: String,
    pub reponses: String,
    pub fin: String,
}

/// `echange("UID STORE 7", "", "OK fait")`.
pub fn echange(attendu: &str, reponses: &str, fin: &str) -> Echange {
    Echange { attendu: attendu.into(), reponses: reponses.into(), fin: fin.into() }
}

/// Commandes reçues, sans étiquette, littéraux remplacés par « {n octets} ».
pub type Journal = Arc<Mutex<Vec<String>>>;

struct FluxSimule {
    attendus: VecDeque<Echange>,
    journal: Journal,
    /// Ce que le client va lire.
    sortie: VecDeque<u8>,
    /// Ce que le client a écrit et qui n'est pas encore une commande complète.
    entree: Vec<u8>,
    /// Commande en cours de lecture (une ligne coupée par un littéral).
    commande: String,
    /// Octets de littéral restant à lire ; la ligne de la commande reprend
    /// ensuite jusqu'à son CRLF.
    litteral: Option<usize>,
    /// Étiquette de la commande `IDLE` en cours : `DONE`, qui n'en porte pas,
    /// la termine.
    idle: Option<String>,
}

impl FluxSimule {
    fn traiter(&mut self) {
        loop {
            if let Some(taille) = self.litteral {
                if self.entree.len() < taille {
                    return;
                }
                let litteral: Vec<u8> = self.entree.drain(..taille).collect();
                // Le texte du littéral, lisible dans le journal (recherches).
                if let Ok(t) = std::str::from_utf8(&litteral) {
                    if t.len() <= 64 && !t.contains('\n') {
                        self.commande.push_str(t);
                    }
                }
                self.litteral = None;
                continue;
            }
            let Some(fin) = self.entree.windows(2).position(|f| f == b"\r\n") else {
                return;
            };
            let ligne: Vec<u8> = self.entree.drain(..fin + 2).collect();
            let ligne = String::from_utf8_lossy(&ligne[..fin]).into_owned();
            // « … {12} » : littéral synchronisé, le client attend l'invite ;
            // « … {12+} » (LITERAL+) : il envoie la suite sans attendre.
            if let Some((taille, synchronise)) = taille_litteral(&ligne) {
                self.commande.push_str(&ligne[..ligne.rfind('{').unwrap_or(ligne.len())]);
                self.commande.push_str(&format!("{{{taille} octets}}"));
                if synchronise {
                    self.sortie.extend(b"+ pret\r\n");
                }
                self.litteral = Some(taille);
                continue;
            }
            self.commande.push_str(&ligne);
            let complete = std::mem::take(&mut self.commande);
            self.repondre(&complete);
        }
    }

    fn repondre(&mut self, complete: &str) {
        let (etiquette, texte) = match (complete, &self.idle) {
            ("DONE", Some(e)) => (e.clone(), "DONE"),
            _ => {
                let (e, t) = complete.split_once(' ').unwrap_or((complete, ""));
                (e.to_string(), t)
            }
        };
        let etiquette = etiquette.as_str();
        self.journal.lock().unwrap().push(texte.to_string());
        let echange = self
            .attendus
            .pop_front()
            .unwrap_or_else(|| panic!("commande imprévue : {texte}"));
        assert!(
            texte.starts_with(&echange.attendu),
            "commande reçue « {texte} », attendue « {}… »",
            echange.attendu
        );
        if texte == "IDLE" {
            // Invite, puis ce que le serveur signale de lui-même ; la fin ne
            // vient qu'avec DONE.
            self.sortie.extend(b"+ idling\r\n");
            self.sortie.extend(echange.reponses.as_bytes());
            self.idle = Some(etiquette.to_string());
            return;
        }
        if texte == "DONE" {
            self.idle = None;
        }
        self.sortie.extend(echange.reponses.as_bytes());
        self.sortie.extend(format!("{etiquette} {}\r\n", echange.fin).as_bytes());
    }
}

fn taille_litteral(ligne: &str) -> Option<(usize, bool)> {
    let debut = ligne.rfind('{')?;
    let contenu = ligne[debut + 1..].strip_suffix('}')?;
    let synchronise = !contenu.ends_with('+');
    Some((contenu.trim_end_matches('+').parse().ok()?, synchronise))
}

impl Flux for FluxSimule {}

impl Read for FluxSimule {
    fn read(&mut self, tampon: &mut [u8]) -> io::Result<usize> {
        // Pendant IDLE, rien à lire veut dire « rien de neuf » : un délai écoulé.
        if self.sortie.is_empty() && self.idle.is_some() {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        // Rien à lire : le serveur « ferme », ce que le client prend pour une
        // perte de connexion — un dialogue incomplet échoue donc tout de suite.
        let n = tampon.len().min(self.sortie.len());
        for (i, octet) in self.sortie.drain(..n).enumerate() {
            tampon[i] = octet;
        }
        Ok(n)
    }
}

impl Write for FluxSimule {
    fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
        self.entree.extend_from_slice(octets);
        self.traiter();
        Ok(octets.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Session ouverte sur le faux serveur, qui annonce `capacites` après
/// connexion, puis attend les `echanges` dans l'ordre.
pub fn session(capacites: &str, echanges: Vec<Echange>) -> (Client, Journal) {
    let (client, journal) = ouverture(capacites, "OK", echanges);
    journal.lock().unwrap().clear();
    (client, journal)
}

/// Comme `session`, le journal gardant l'ouverture ; `fin_id` : la réponse à
/// la présentation (`ID`), si le serveur l'annonce.
fn ouverture(capacites: &str, fin_id: &str, echanges: Vec<Echange>) -> (Client, Journal) {
    let mut attendus = VecDeque::from(vec![echange(
        "LOGIN",
        "",
        "OK connecté",
    ), echange("CAPABILITY", &format!("* CAPABILITY IMAP4rev1 {capacites}\r\n"), "OK")]);
    if capacites.split_whitespace().any(|c| c == "ID") {
        attendus.push_back(echange("ID (\"name\" \"MMail\"", "* ID (\"name\" \"Dovecot\")\r\n", fin_id));
    }
    if capacites.split_whitespace().any(|c| c == "QRESYNC") {
        attendus.push_back(echange("ENABLE QRESYNC", "* ENABLED QRESYNC\r\n", "OK"));
    }
    attendus.extend(echanges);
    let journal = Journal::default();
    let flux = FluxSimule {
        attendus,
        journal: journal.clone(),
        sortie: VecDeque::from(b"* OK serveur simule\r\n".to_vec()),
        entree: Vec::new(),
        commande: String::new(),
        litteral: None,
        idle: None,
    };
    let mut client = Client::sur(Box::new(flux)).expect("salutation");
    client.ouvrir_session("essai@exemple.fr", "secret").expect("session");
    (client, journal)
}

/// Capacités d'un Dovecot récent, celles que vise MMail.
pub const DOVECOT: &str = "ID QRESYNC CONDSTORE UIDPLUS MOVE SPECIAL-USE LIST-STATUS IDLE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_apres_connexion() {
        let (client, journal) = ouverture(DOVECOT, "OK", vec![]);
        let journal = journal.lock().unwrap();
        assert!(journal[0].starts_with("LOGIN "), "{journal:?}");
        assert_eq!(journal[1], "CAPABILITY");
        let id = &journal[2];
        let attendu = format!("ID (\"name\" \"MMail\" \"version\" \"{}\" \"os\" \"", env!("CARGO_PKG_VERSION"));
        assert!(id.starts_with(&attendu), "{id}");
        assert!(id.contains("\"os-version\" \"") && id.contains("\"x-poste\" \""), "{id}");
        assert!(id.ends_with("\")"), "{id}");
        assert_eq!(journal[3], "ENABLE QRESYNC");
        assert!(client.qresync());
    }

    #[test]
    fn presentation_refusee_sans_effet() {
        let (mut client, journal) = ouverture(DOVECOT, "BAD pas compris", vec![echange("NOOP", "", "OK")]);
        assert!(client.qresync(), "la session continue après le refus");
        client.brute("NOOP").expect("NOOP");
        assert_eq!(journal.lock().unwrap().last().unwrap(), "NOOP");
    }

    #[test]
    fn sans_id_pas_de_presentation() {
        let (_, journal) = ouverture("UIDPLUS MOVE", "OK", vec![]);
        assert!(!journal.lock().unwrap().iter().any(|c| c.starts_with("ID ")));
    }

    #[test]
    fn depot_avec_litteral() {
        let (mut client, journal) = session(
            DOVECOT,
            vec![echange("APPEND \"Sent\" (\\Seen)", "", "OK [APPENDUID 5 42] fait")],
        );
        let uid = client.deposer("Sent", &["\\Seen".into()], "", b"Subject: x\r\n\r\ncorps").unwrap();
        assert_eq!(uid, Some(42));
        assert_eq!(journal.lock().unwrap()[0], "APPEND \"Sent\" (\\Seen) {19 octets}");
    }
}
