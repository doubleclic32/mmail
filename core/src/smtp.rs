// SPDX-License-Identifier: GPL-3.0-or-later
//! Client SMTP de soumission : remettre au serveur un message déjà fabriqué.
//!
//! Écrit sur la pile TLS du client IMAP, comme `http` : mêmes autorités de
//! certification sur les trois cibles. TLS d'emblée sur le port 465 (RFC 8314),
//! authentification PLAIN ou LOGIN — `XOAUTH2` pour un compte à jeton (Gmail)
//! —, un message par connexion : l'envoi est assez rare pour ne pas garder de
//! session ouverte.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

use crate::imap::{configuration_tls, joindre, Erreur, Resultat, DELAI};

/// Soumission sur TLS d'emblée : c'est ce que publie Mailcow.
pub const PORT_SOUMISSION: u16 = 465;

/// Longueur au-delà de laquelle une ligne de réponse est tenue pour aberrante.
const LIGNE_MAX: usize = 8192;

/// Envoie `octets` (un message RFC 5322 complet, sans en-tête `Bcc`) de
/// `expediteur` à chacun des `destinataires` — `Cci` compris : c'est
/// l'enveloppe qui les porte, pas le message. `accuse` demande un accusé de
/// remise (DSN, RFC 3461) quand le serveur le propose ; sinon rien.
pub fn envoyer(
    hote: &str,
    utilisateur: &str,
    mot_de_passe: &str,
    expediteur: &str,
    destinataires: &[String],
    octets: &[u8],
    accuse: bool,
) -> Resultat<()> {
    remettre(hote, &Authentification::MotDePasse { utilisateur, mot_de_passe }, expediteur, destinataires, octets, accuse)
}

/// Comme `envoyer`, avec un jeton d'accès OAuth2 au lieu d'un mot de passe
/// (`AUTH XOAUTH2`, comptes Google). Un jeton refusé rend une erreur dont le
/// texte commence par « authentification ».
pub fn envoyer_jeton(
    hote: &str,
    utilisateur: &str,
    jeton: &str,
    expediteur: &str,
    destinataires: &[String],
    octets: &[u8],
    accuse: bool,
) -> Resultat<()> {
    remettre(hote, &Authentification::Jeton { utilisateur, jeton }, expediteur, destinataires, octets, accuse)
}

/// Ce qui ouvre la session : un mot de passe, ou un jeton d'accès.
enum Authentification<'a> {
    MotDePasse { utilisateur: &'a str, mot_de_passe: &'a str },
    Jeton { utilisateur: &'a str, jeton: &'a str },
}

fn remettre(
    hote: &str,
    auth: &Authentification,
    expediteur: &str,
    destinataires: &[String],
    octets: &[u8],
    accuse: bool,
) -> Resultat<()> {
    if destinataires.is_empty() {
        return Err(Erreur::Refuse("aucun destinataire".into()));
    }
    let config = configuration_tls()?;
    let nom = rustls::pki_types::ServerName::try_from(hote.to_string())
        .map_err(|_| Erreur::Reseau(format!("nom de serveur invalide : {hote}")))?;
    let connexion = rustls::ClientConnection::new(Arc::new(config), nom)
        .map_err(|e| Erreur::Reseau(e.to_string()))?;
    let tcp = joindre(hote, PORT_SOUMISSION)?;
    tcp.set_read_timeout(Some(DELAI))?;
    tcp.set_write_timeout(Some(DELAI))?;
    let mut s = Session { flux: BufReader::new(rustls::StreamOwned::new(connexion, tcp)) };

    s.attendre("salutation", 220)?;
    let ehlo = s.echanger("EHLO", "EHLO localhost", 250)?;
    if let Some(max) = taille_max(&ehlo) {
        if octets.len() > max {
            return Err(Erreur::Refuse(format!(
                "message trop volumineux pour le serveur : {} Mo, {} Mo au plus",
                octets.len().div_ceil(1_048_576),
                max / 1_048_576
            )));
        }
    }
    authentifier(&mut s, &mecanismes(&ehlo), auth)?;
    let dsn = accuse && ehlo.lines().any(|l| l.trim().eq_ignore_ascii_case("DSN"));
    let (retour, notifier) = if dsn { (" RET=HDRS", " NOTIFY=SUCCESS,FAILURE") } else { ("", "") };
    s.echanger("expéditeur", &format!("MAIL FROM:<{expediteur}>{retour}"), 250)?;
    for destinataire in destinataires {
        // 251 : « pas local, je transmets » — accepté aussi.
        let (code, texte) = s.envoyer_ligne(&format!("RCPT TO:<{destinataire}>{notifier}"))?;
        if code != 250 && code != 251 {
            return Err(Erreur::Refuse(format!("destinataire {destinataire} refusé : {code} {texte}")));
        }
    }
    s.echanger("données", "DATA", 354)?;
    s.flux.get_mut().write_all(&donnees(octets))?;
    s.flux.get_mut().flush()?;
    s.attendre("remise", 250)?;
    // Le message est accepté : un QUIT perdu n'y change rien.
    let _ = s.envoyer_ligne("QUIT");
    Ok(())
}

/// Ouvre la session SMTP selon les mécanismes annoncés.
fn authentifier<F: Read + Write>(s: &mut Session<F>, mecanismes: &[String], auth: &Authentification) -> Resultat<()> {
    match auth {
        Authentification::MotDePasse { utilisateur, mot_de_passe } => {
            if mecanismes.iter().any(|m| m == "PLAIN") {
                let jeton = base64(format!("\0{utilisateur}\0{mot_de_passe}").as_bytes());
                s.echanger("authentification", &format!("AUTH PLAIN {jeton}"), 235)?;
            } else if mecanismes.iter().any(|m| m == "LOGIN") {
                s.echanger("authentification", "AUTH LOGIN", 334)?;
                s.echanger("authentification", &base64(utilisateur.as_bytes()), 334)?;
                s.echanger("authentification", &base64(mot_de_passe.as_bytes()), 235)?;
            } else {
                return Err(Erreur::Refuse("le serveur ne propose ni PLAIN ni LOGIN".into()));
            }
        }
        Authentification::Jeton { utilisateur, jeton } => {
            if !mecanismes.iter().any(|m| m == "XOAUTH2") {
                return Err(Erreur::Refuse("le serveur ne propose pas XOAUTH2".into()));
            }
            let chaine = crate::oauth::xoauth2(utilisateur, jeton);
            let (code, texte) = s.envoyer_ligne(&format!("AUTH XOAUTH2 {chaine}"))?;
            if code == 334 {
                // Refus : le serveur détaille l'erreur (JSON en base64) et
                // attend une ligne vide avant de conclure (RFC 7628 § 3.2.3).
                let (code, texte) = s.envoyer_ligne("")?;
                return verifier("authentification", code, texte, 235).map(|_| ());
            }
            verifier("authentification", code, texte, 235)?;
        }
    }
    Ok(())
}

struct Session<F: Read + Write> {
    flux: BufReader<F>,
}

impl<F: Read + Write> Session<F> {
    fn envoyer_ligne(&mut self, ligne: &str) -> Resultat<(u16, String)> {
        let flux = self.flux.get_mut();
        flux.write_all(ligne.as_bytes())?;
        flux.write_all(b"\r\n")?;
        flux.flush()?;
        lire_reponse(&mut self.flux)
    }

    /// Envoie une commande et exige le code attendu. `etape` nomme l'échange
    /// dans l'erreur : la ligne envoyée n'y figure jamais, elle peut porter
    /// les identifiants.
    fn echanger(&mut self, etape: &str, ligne: &str, attendu: u16) -> Resultat<String> {
        let (code, texte) = self.envoyer_ligne(ligne)?;
        verifier(etape, code, texte, attendu)
    }

    fn attendre(&mut self, etape: &str, attendu: u16) -> Resultat<String> {
        let (code, texte) = lire_reponse(&mut self.flux)?;
        verifier(etape, code, texte, attendu)
    }
}

fn verifier(etape: &str, code: u16, texte: String, attendu: u16) -> Resultat<String> {
    if code == attendu {
        Ok(texte)
    } else {
        Err(Erreur::Refuse(format!("{etape} : {code} {texte}")))
    }
}

/// Réponse du serveur, éventuellement sur plusieurs lignes (`250-…` puis
/// `250 …`) : son code et son texte, lignes jointes par des sauts de ligne.
pub fn lire_reponse<R: BufRead>(flux: &mut R) -> Resultat<(u16, String)> {
    let mut texte = String::new();
    loop {
        let mut ligne = String::new();
        let lus = flux.by_ref().take(LIGNE_MAX as u64).read_line(&mut ligne)?;
        if lus == 0 {
            return Err(Erreur::Reseau("connexion fermée par le serveur".into()));
        }
        let ligne = ligne.trim_end_matches(['\r', '\n']);
        let code: u16 = ligne
            .get(..3)
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| Erreur::Protocole(format!("réponse SMTP illisible : {ligne}")))?;
        if !texte.is_empty() {
            texte.push('\n');
        }
        texte.push_str(ligne.get(4..).unwrap_or("").trim());
        if ligne.as_bytes().get(3) != Some(&b'-') {
            return Ok((code, texte));
        }
    }
}

/// Mécanismes d'authentification annoncés par `EHLO` (`AUTH PLAIN LOGIN`).
pub fn mecanismes(ehlo: &str) -> Vec<String> {
    ehlo.lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix("AUTH ").or_else(|| l.strip_prefix("AUTH="))
        })
        .flat_map(|m| m.split_whitespace())
        .map(|m| m.to_ascii_uppercase())
        .collect()
}

/// Taille maximale annoncée par `EHLO` (`SIZE 52428800`), si elle l'est et
/// n'est pas nulle (zéro : pas de limite annoncée).
pub fn taille_max(ehlo: &str) -> Option<usize> {
    ehlo.lines()
        .find_map(|l| l.trim().strip_prefix("SIZE "))
        .and_then(|n| n.trim().parse().ok())
        .filter(|&n: &usize| n > 0)
}

/// Corps de la commande `DATA` : fins de ligne CRLF, points doublés en début
/// de ligne (RFC 5321 § 4.5.2), puis la ligne finale « . ».
pub fn donnees(octets: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::with_capacity(octets.len() + 64);
    let mut debut_ligne = true;
    for (i, &o) in octets.iter().enumerate() {
        if debut_ligne && o == b'.' {
            sortie.push(b'.');
        }
        if o == b'\n' && (i == 0 || octets[i - 1] != b'\r') {
            sortie.push(b'\r');
        }
        sortie.push(o);
        debut_ligne = o == b'\n';
    }
    if !sortie.ends_with(b"\r\n") {
        sortie.extend_from_slice(b"\r\n");
    }
    sortie.extend_from_slice(b".\r\n");
    sortie
}

/// Base64 standard, avec remplissage.
pub fn base64(octets: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut sortie = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16
            | (*bloc.get(1).unwrap_or(&0) as u32) << 8
            | *bloc.get(2).unwrap_or(&0) as u32;
        for (i, decalage) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= bloc.len() {
                sortie.push(TABLE[(n >> decalage & 63) as usize] as char);
            } else {
                sortie.push('=');
            }
        }
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Faux serveur : ses réponses d'avance, ce que le client lui envoie.
    struct Faux {
        reponses: Cursor<Vec<u8>>,
        recu: Vec<u8>,
    }

    impl Read for Faux {
        fn read(&mut self, tampon: &mut [u8]) -> std::io::Result<usize> {
            self.reponses.read(tampon)
        }
    }

    impl Write for Faux {
        fn write(&mut self, octets: &[u8]) -> std::io::Result<usize> {
            self.recu.extend_from_slice(octets);
            Ok(octets.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn session(reponses: &str) -> Session<Faux> {
        Session { flux: BufReader::new(Faux { reponses: Cursor::new(reponses.as_bytes().to_vec()), recu: Vec::new() }) }
    }

    fn envoye(s: &Session<Faux>) -> String {
        String::from_utf8(s.flux.get_ref().recu.clone()).unwrap()
    }

    #[test]
    fn authentification_par_jeton() {
        let gmail = vec!["LOGIN".to_string(), "PLAIN".into(), "XOAUTH2".into(), "OAUTHBEARER".into()];
        let jeton = Authentification::Jeton { utilisateur: "jean@gmail.com", jeton: "ya29.JETON" };
        let mut s = session("235 2.7.0 Accepted\r\n");
        authentifier(&mut s, &gmail, &jeton).unwrap();
        assert_eq!(envoye(&s), format!("AUTH XOAUTH2 {}\r\n", crate::oauth::xoauth2("jean@gmail.com", "ya29.JETON")));

        // Jeton refusé : le détail en 334, une ligne vide, puis le refus.
        let mut s = session("334 eyJzdGF0dXMiOiI0MDAifQ==\r\n535-5.7.8 Username and Password not accepted.\r\n535 5.7.8 Learn more\r\n");
        let e = authentifier(&mut s, &gmail, &jeton).unwrap_err();
        assert!(matches!(e, Erreur::Refuse(ref m) if m.starts_with("authentification : 535")), "{e:?}");
        assert!(envoye(&s).ends_with("\r\n\r\n"), "ligne vide après le défi");

        let e = authentifier(&mut session(""), &["PLAIN".to_string()], &jeton).unwrap_err();
        assert!(matches!(e, Erreur::Refuse(ref m) if m.contains("XOAUTH2")));
    }

    #[test]
    fn authentification_par_mot_de_passe_inchangee() {
        let mdp = Authentification::MotDePasse { utilisateur: "a@b.fr", mot_de_passe: "secret" };
        let mut s = session("235 ok\r\n");
        authentifier(&mut s, &["PLAIN".to_string(), "LOGIN".into()], &mdp).unwrap();
        assert_eq!(envoye(&s), format!("AUTH PLAIN {}\r\n", base64(b"\0a@b.fr\0secret")));
        let mut s = session("334 VXNlcm5hbWU6\r\n334 UGFzc3dvcmQ6\r\n235 ok\r\n");
        authentifier(&mut s, &["LOGIN".to_string()], &mdp).unwrap();
        assert_eq!(envoye(&s), format!("AUTH LOGIN\r\n{}\r\n{}\r\n", base64(b"a@b.fr"), base64(b"secret")));
    }

    #[test]
    fn reponses_sur_une_et_plusieurs_lignes() {
        let mut flux = Cursor::new(
            b"220 mail.exemple.fr ESMTP\r\n250-mail.exemple.fr\r\n250-SIZE 52428800\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n".to_vec(),
        );
        assert_eq!(lire_reponse(&mut flux).unwrap(), (220, "mail.exemple.fr ESMTP".into()));
        let (code, ehlo) = lire_reponse(&mut flux).unwrap();
        assert_eq!(code, 250);
        assert_eq!(mecanismes(&ehlo), vec!["PLAIN", "LOGIN"]);
        assert_eq!(taille_max(&ehlo), Some(52_428_800));
        assert!(lire_reponse(&mut flux).is_err(), "connexion fermée");
    }

    #[test]
    fn reponse_illisible_refusee() {
        assert!(lire_reponse(&mut Cursor::new(b"bonjour\r\n".to_vec())).is_err());
        assert_eq!(taille_max("SIZE 0"), None);
        assert_eq!(taille_max("8BITMIME"), None);
        assert_eq!(mecanismes("AUTH=LOGIN"), vec!["LOGIN"]);
    }

    #[test]
    fn points_doubles_et_fins_de_ligne() {
        assert_eq!(donnees(b"a\r\n.b\r\n..c\r\n"), b"a\r\n..b\r\n...c\r\n.\r\n");
        assert_eq!(donnees(b".debut\nfin"), b"..debut\r\nfin\r\n.\r\n");
        assert_eq!(donnees(b"x\r\n"), b"x\r\n.\r\n");
    }

    #[test]
    fn base64_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"\0u@x.fr\0m\xc3\xa9"), "AHVAeC5mcgBtw6k=");
    }
}
