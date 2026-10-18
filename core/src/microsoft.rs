// SPDX-License-Identifier: GPL-3.0-or-later
//! Comptes Microsoft (Office 365, Outlook.com) : connexion par OAuth2, envoi
//! par Microsoft Graph.
//!
//! Office 365 n'accepte plus de mot de passe en IMAP : il faut un jeton
//! d'accès, que Microsoft délivre une fois la personne connectée dans son
//! navigateur et son consentement donné. MMail est une application publique —
//! sans secret — inscrite une fois pour toutes dans un annuaire Entra, ouverte
//! à toutes les organisations et aux comptes personnels : rien n'est à inscrire
//! chez le client, qui consent seulement — l'utilisateur lui-même, ou son
//! administrateur une fois pour toute l'organisation (`url_consentement`).
//!
//! Flux « code d'autorisation » avec PKCE (RFC 7636), retour sur une écoute
//! locale `http://localhost:<port>` (RFC 8252 § 7.3) : la connexion se fait
//! dans le navigateur du système, MMail ne voit jamais le mot de passe.
//!
//! Le jeton de rafraîchissement tient lieu de mot de passe : il va au coffre du
//! système, sous la même clé. Les jetons d'accès (une heure) ne vivent qu'en
//! mémoire, un par ressource — la lecture (IMAP) et l'envoi (Graph).
//!
//! L'envoi passe par Graph et non par SMTP : le SMTP authentifié est coupé par
//! défaut dans Office 365, et le rétablir est l'affaire de l'administrateur de
//! chaque organisation. Graph range lui-même la copie dans « Éléments
//! envoyés ». Une requête Graph pèse 4 Mo au plus : un message plus lourd
//! passe par un brouillon, auquel les pièces jointes s'ajoutent à part
//! (`Mail.ReadWrite`), avant d'être envoyé.

use std::time::Duration;

use crate::http::{self, Demande};
#[cfg(test)]
use crate::imap::Erreur;
use crate::oauth::{encoder, Fournisseur};
#[cfg(test)]
use crate::oauth::{base64url, defi_de, parametres, verrouiller};
#[cfg(test)]
use std::io::{Read, Write};
#[cfg(test)]
use std::net::TcpStream;

// Le commun aux comptes OAuth2 (cf. `oauth`), sous les noms que le reste de
// MMail emploie pour Microsoft.
pub use crate::oauth::{
    a_conserver, adresse_id_token, etat, jeton_acces, lire_reponse_jeton, lire_requete, oublier, pkce, retenir,
    xoauth2, Ecoute, ErreurOAuth, Jetons, Partage, Pkce, ReponseJeton, DELAI_CONNEXION,
};

/// Écoute locale du retour de Microsoft : `http://localhost:<port>`.
pub fn ecouter() -> std::io::Result<Ecoute> {
    crate::oauth::ecouter("localhost")
}

/// Attend le retour du navigateur (cf. `oauth::attendre_code`).
pub fn attendre_code(ecoute: &Ecoute, etat: &str, delai: Duration) -> Result<String, ErreurOAuth> {
    crate::oauth::attendre_code(ecoute, etat, delai, "Microsoft")
}

/// Jetons d'un compte Microsoft, partagés entre ses sessions.
pub fn partager(rafraichissement: &str) -> Partage {
    crate::oauth::partager(Fournisseur::Microsoft, rafraichissement)
}

/// Point d'entrée de Microsoft pour toutes les organisations et les comptes
/// personnels : le rafraîchissement des jetons, et la connexion sans adresse.
const AUTORITE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";

/// Adresse de retour de la connexion sous Android : la page intégrée à MMail
/// (cf. ConnexionIntegree.qml) l'intercepte sans la charger — rien n'y écoute.
/// La même que sur le bureau, déjà déclarée dans l'application Entra « MMail ».
pub const REDIRECTION_ANDROID: &str = "http://localhost";

/// Domaines des comptes personnels de Microsoft (Outlook.com).
const PARTICULIERS: [&str; 5] = ["outlook", "hotmail", "live", "msn", "passport"];

/// Point d'entrée de la connexion pour une adresse. Retour de Manu du
/// 18/10 : avec le point d'entrée commun, un téléphone déjà connecté à un
/// compte Microsoft personnel partait sur la connexion des particuliers
/// (login.live.com) pour une adresse d'entreprise. Une adresse d'Outlook.com,
/// Hotmail, Live ou MSN va aux comptes personnels, toute autre aux comptes
/// d'organisation ; sans adresse, le point d'entrée commun.
pub fn autorite(adresse: &str) -> String {
    let domaine = adresse.trim().rsplit_once('@').map(|(_, d)| d.to_ascii_lowercase()).unwrap_or_default();
    let quoi = if domaine.is_empty() {
        "common"
    } else if PARTICULIERS.iter().any(|p| domaine.starts_with(&format!("{p}."))) {
        "consumers"
    } else {
        "organizations"
    };
    format!("https://login.microsoftonline.com/{quoi}/oauth2/v2.0")
}

/// Serveur IMAP d'Office 365 et d'Outlook.com.
pub const HOTE_IMAP: &str = "outlook.office365.com";

/// Lecture : IMAP pour le compte de la personne connectée.
pub const PORTEE_IMAP: &str = "offline_access https://outlook.office.com/IMAP.AccessAsUser.All";

/// Envoi : Graph, `Mail.Send`, et `Mail.ReadWrite` pour le brouillon d'un
/// message trop lourd pour une seule requête. IMAP donne déjà accès à toute
/// la boîte : ce droit n'ouvre rien de plus sur les messages.
pub const PORTEE_ENVOI: &str =
    "offline_access https://graph.microsoft.com/Mail.Send https://graph.microsoft.com/Mail.ReadWrite";

/// Ce que la connexion demande : les deux ressources d'un coup, pour un seul
/// consentement ; `openid email` donnent l'adresse du compte. Le code, lui,
/// ne s'échange que contre un jeton d'une seule ressource (`PORTEE_CODE`).
const PORTEE_AUTORISATION: &str = "offline_access openid email \
     https://outlook.office.com/IMAP.AccessAsUser.All https://graph.microsoft.com/Mail.Send \
     https://graph.microsoft.com/Mail.ReadWrite";
const PORTEE_CODE: &str = "offline_access openid email https://outlook.office.com/IMAP.AccessAsUser.All";

/// Identifiant de l'application « MMail » dans l'annuaire Entra où elle est
/// inscrite (éditeur `mmedia.fr`). Public par nature : application sans
/// secret, que toute page de connexion affiche.
const MICROSOFT_CLIENT_ID: &str = "7b6c8863-79ca-42b3-8149-313df37a3993";

/// Graph, pour le compte de la personne connectée.
const GRAPH: &str = "https://graph.microsoft.com/v1.0/me";

/// Corps d'une requête Graph, message en base64 compris : 4 Mo annoncés, sans
/// dire s'il s'agit de millions d'octets ou de Mio — d'où la marge.
const ENVOI_MAX: usize = 3_900_000;

/// Morceau d'une pièce jointe déposée par une session de dépôt : un multiple
/// de 320 Kio, sous la limite d'une requête.
const MORCEAU: usize = 10 * 320 * 1024;

/// Pièce jointe la plus lourde que Graph accepte.
const PIECE_MAX: usize = 150 * 1024 * 1024;

/// Réponse attendue de Graph : un accusé, ou la description d'un message —
/// son corps compris, pour un brouillon.
const REPONSE_MAX: usize = 256 * 1024;
const BROUILLON_MAX: usize = 8 * 1024 * 1024;

/// Identifiant de l'application : `MMAIL_MICROSOFT_CLIENT_ID` s'il est donné
/// (essais, autre inscription), sinon celui du programme. Vide : la connexion
/// Microsoft est indisponible.
pub fn identifiant_client() -> String {
    std::env::var("MMAIL_MICROSOFT_CLIENT_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| MICROSOFT_CLIENT_ID.to_string())
}

pub fn disponible() -> bool {
    !identifiant_client().is_empty()
}

/// Lien que l'administrateur d'une organisation ouvre une fois : MMail est
/// alors autorisé pour tous ses utilisateurs, sans qu'aucun n'ait à consentir.
pub fn url_consentement(client: &str) -> String {
    format!("https://login.microsoftonline.com/organizations/adminconsent?client_id={}", encoder(client))
}

/// Adresse de la page de connexion de Microsoft. `indice` : l'adresse du
/// compte quand on le reconnecte, pour ne pas la redemander.
pub fn url_autorisation(client: &str, redirection: &str, defi: &str, etat: &str, indice: &str) -> String {
    let mut url = format!(
        "{}/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query\
         &scope={}&state={}&code_challenge={}&code_challenge_method=S256&prompt=select_account",
        autorite(indice),
        encoder(client),
        encoder(redirection),
        encoder(PORTEE_AUTORISATION),
        encoder(etat),
        encoder(defi),
    );
    if !indice.trim().is_empty() {
        url.push_str(&format!("&login_hint={}", encoder(indice.trim())));
    }
    url
}

/// Échange le code reçu par l'écoute locale contre les premiers jetons.
/// `adresse` : celle de la connexion, pour le même point d'entrée.
pub fn echanger_code(client: &str, code: &str, verificateur: &str, redirection: &str, adresse: &str) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(&format!("{}/token", autorite(adresse)), &format!(
        "client_id={}&grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}&scope={}",
        encoder(client),
        encoder(code),
        encoder(redirection),
        encoder(verificateur),
        encoder(PORTEE_CODE),
    ))
}

/// Jeton d'accès à une ressource (`PORTEE_IMAP`, `PORTEE_ENVOI`), par le jeton
/// de rafraîchissement.
pub fn rafraichir(client: &str, rafraichissement: &str, portee: &str) -> Result<ReponseJeton, ErreurOAuth> {
    demander_jeton(&format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}&scope={}",
        encoder(client),
        encoder(rafraichissement),
        encoder(portee),
    ))
}

fn demander_jeton(corps: &str) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(&format!("{AUTORITE}/token"), corps)
}

// -------------------------------------------------------------------- envoi

/// Ce qui porte les requêtes : `http::envoyer`, ou un faux Graph dans les
/// essais.
type Transport<'a> = dyn FnMut(&Demande) -> crate::imap::Resultat<http::Reponse> + 'a;

/// Envoie un message par Graph : le MIME complet, en base64 (forme documentée
/// de `sendMail`). Graph lit les destinataires dans les en-têtes : ceux de
/// l'enveloppe qui n'y figurent pas — les Cci — y sont ajoutés en `Bcc`, que
/// Microsoft retire à la remise.
///
/// Trop lourd pour une requête, le message passe par un brouillon : créé sans
/// ses plus grosses pièces jointes, complété de celles-ci une à une — par
/// morceaux au-delà de la limite d'une requête —, puis envoyé. Le brouillon
/// d'un envoi qui échoue en route est supprimé.
pub fn envoyer(jeton: &str, octets: &[u8], destinataires: &[String]) -> Result<(), ErreurOAuth> {
    envoyer_par(&mut |d: &Demande| http::envoyer(d), jeton, octets, destinataires)
}

fn envoyer_par(t: &mut Transport, jeton: &str, octets: &[u8], destinataires: &[String]) -> Result<(), ErreurOAuth> {
    let message = avec_cci(octets, destinataires);
    if longueur_base64(message.len()) <= ENVOI_MAX {
        let corps = crate::smtp::base64(&message);
        let r = appeler(t, jeton, "POST", &format!("{GRAPH}/sendMail"), "text/plain", corps.as_bytes(), REPONSE_MAX)?;
        return succes(&r);
    }
    let (reduit, pieces) = alleger(&message)?;
    let corps = crate::smtp::base64(&reduit);
    let r = appeler(t, jeton, "POST", &format!("{GRAPH}/messages"), "text/plain", corps.as_bytes(), BROUILLON_MAX)?;
    let brouillon = reponse_json(&r)?;
    let id = brouillon
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ErreurOAuth::Reseau("brouillon créé sans identifiant".into()))?
        .to_string();
    let adresse = format!("{GRAPH}/messages/{}", encoder(&id));
    let issue = completer(t, jeton, &adresse, &brouillon, destinataires, &pieces);
    if issue.is_err() {
        // Un brouillon à moitié fait n'a rien à faire dans la boîte : le
        // message reste à envoyer dans MMail.
        let _ = appeler(t, jeton, "DELETE", &adresse, "", b"", REPONSE_MAX);
    }
    issue
}

/// Complète le brouillon — Cci manquants, pièces jointes — et l'envoie.
fn completer(
    t: &mut Transport,
    jeton: &str,
    adresse: &str,
    brouillon: &serde_json::Value,
    destinataires: &[String],
    pieces: &[Piece],
) -> Result<(), ErreurOAuth> {
    // Que Graph tire les Cci de l'en-tête `Bcc` d'un brouillon n'est pas
    // documenté : ce que porte le brouillon fait foi, et les manquants sont
    // ajoutés.
    let liste = |cle: &str| -> Vec<String> {
        brouillon
            .get(cle)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|d| d.pointer("/emailAddress/address").and_then(|a| a.as_str()))
            .map(|a| a.trim().to_lowercase())
            .collect()
    };
    let presents: Vec<String> = ["toRecipients", "ccRecipients", "bccRecipients"].iter().flat_map(|c| liste(c)).collect();
    let manquants: Vec<String> = destinataires
        .iter()
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !presents.contains(d))
        .collect();
    if !manquants.is_empty() {
        let cci: Vec<serde_json::Value> = liste("bccRecipients")
            .into_iter()
            .chain(manquants)
            .map(|a| serde_json::json!({ "emailAddress": { "address": a } }))
            .collect();
        let corps = serde_json::json!({ "bccRecipients": cci }).to_string();
        succes(&appeler(t, jeton, "PATCH", adresse, "application/json", corps.as_bytes(), BROUILLON_MAX)?)?;
    }
    for p in pieces {
        joindre(t, jeton, adresse, p)?;
    }
    succes(&appeler(t, jeton, "POST", &format!("{adresse}/send"), "", b"", REPONSE_MAX)?)
}

/// Ajoute une pièce jointe au brouillon : en une requête si elle y tient,
/// sinon par une session de dépôt, morceau par morceau.
fn joindre(t: &mut Transport, jeton: &str, brouillon: &str, p: &Piece) -> Result<(), ErreurOAuth> {
    let pieces = format!("{brouillon}/attachments");
    if longueur_base64(p.contenu.len()) + p.nom.len() + 1024 <= ENVOI_MAX {
        let corps = serde_json::json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "name": p.nom,
            "contentType": p.type_mime,
            "contentBytes": crate::smtp::base64(&p.contenu),
        })
        .to_string();
        return succes(&appeler(t, jeton, "POST", &pieces, "application/json", corps.as_bytes(), REPONSE_MAX)?);
    }
    let corps = serde_json::json!({
        "AttachmentItem": {
            "attachmentType": "file",
            "name": p.nom,
            "size": p.contenu.len(),
            "contentType": p.type_mime,
        }
    })
    .to_string();
    let r = appeler(t, jeton, "POST", &format!("{pieces}/createUploadSession"), "application/json", corps.as_bytes(), REPONSE_MAX)?;
    let session = reponse_json(&r)?;
    let depot = session
        .get("uploadUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErreurOAuth::Reseau("session de dépôt sans adresse".into()))?;
    let total = p.contenu.len();
    for (i, morceau) in p.contenu.chunks(MORCEAU).enumerate() {
        let debut = i * MORCEAU;
        let entetes = [
            ("Content-Type", "application/octet-stream".to_string()),
            ("Content-Range", format!("bytes {debut}-{}/{total}", debut + morceau.len() - 1)),
        ];
        // L'adresse de dépôt porte sa propre autorisation : Graph refuse
        // qu'on y joigne le jeton.
        let r = t(&Demande {
            methode: "PUT",
            url: depot,
            accepte: "application/json",
            entetes: &entetes,
            corps: morceau,
            taille_max: REPONSE_MAX,
        })
        .map_err(|e| ErreurOAuth::Reseau(e.to_string()))?;
        succes(&r)?;
    }
    Ok(())
}

/// Une requête à Graph, avec le jeton.
fn appeler(
    t: &mut Transport,
    jeton: &str,
    methode: &str,
    url: &str,
    type_contenu: &str,
    corps: &[u8],
    taille_max: usize,
) -> Result<http::Reponse, ErreurOAuth> {
    let mut entetes = vec![("Authorization", format!("Bearer {jeton}"))];
    if !type_contenu.is_empty() {
        entetes.push(("Content-Type", type_contenu.to_string()));
    }
    t(&Demande { methode, url, accepte: "application/json", entetes: &entetes, corps, taille_max })
        .map_err(|e| ErreurOAuth::Reseau(e.to_string()))
}

fn succes(r: &http::Reponse) -> Result<(), ErreurOAuth> {
    if (200..300).contains(&r.statut) {
        Ok(())
    } else {
        Err(erreur_graph(r.statut, &r.corps))
    }
}

fn reponse_json(r: &http::Reponse) -> Result<serde_json::Value, ErreurOAuth> {
    succes(r)?;
    serde_json::from_slice(&r.corps)
        .map_err(|_| ErreurOAuth::Reseau(format!("réponse illisible de Graph (HTTP {})", r.statut)))
}

fn longueur_base64(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Pièce jointe retirée du message, à joindre au brouillon.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    nom: String,
    type_mime: String,
    contenu: Vec<u8>,
}

/// Retire du message ses plus grosses pièces jointes, jusqu'à ce qu'il tienne
/// dans une requête. Seules les pièces jointes en partent : les images du
/// corps y restent, à leur place.
fn alleger(message: &[u8]) -> Result<(Vec<u8>, Vec<Piece>), ErreurOAuth> {
    use mail_parser::MimeHeaders;
    let trop = |texte: String| ErreurOAuth::refus("taille", &texte);
    let analyse = mail_parser::MessageParser::default()
        .parse(message)
        .ok_or_else(|| ErreurOAuth::refus("message", "message illisible"))?;
    let mut candidats: Vec<(std::ops::Range<usize>, &mail_parser::MessagePart)> = analyse
        .parts
        .iter()
        .skip(1)
        .filter(|p| !p.is_multipart() && p.content_disposition().is_some_and(|d| d.is_attachment()))
        .filter_map(|p| Some((plage(message, p)?, p)))
        .collect();
    candidats.sort_by_key(|(r, _)| std::cmp::Reverse(r.len()));
    let mut reste = message.len();
    let mut retires = Vec::new();
    for (r, p) in candidats {
        if longueur_base64(reste) <= ENVOI_MAX {
            break;
        }
        reste -= r.len();
        let piece = piece(message, p).ok_or_else(|| ErreurOAuth::refus("message", "pièce jointe illisible"))?;
        if piece.contenu.len() > PIECE_MAX {
            return Err(trop(format!(
                "pièce jointe « {} » trop volumineuse pour Microsoft : {} Mo, 150 Mo au plus",
                piece.nom,
                piece.contenu.len().div_ceil(1_048_576)
            )));
        }
        retires.push((r, piece));
    }
    if longueur_base64(reste) > ENVOI_MAX {
        return Err(trop(format!(
            "message trop volumineux pour l'envoi par Microsoft : le texte et les images du corps pèsent {} Mo, 3 Mo au plus",
            reste.div_ceil(1_048_576)
        )));
    }
    retires.sort_by_key(|(r, _)| r.start);
    let mut reduit = Vec::with_capacity(reste);
    let mut position = 0;
    for (r, _) in &retires {
        reduit.extend_from_slice(&message[position..r.start]);
        position = r.end;
    }
    reduit.extend_from_slice(&message[position..]);
    Ok((reduit, retires.into_iter().map(|(_, p)| p).collect()))
}

/// Ce qu'occupe une partie dans le message : du saut de ligne qui précède sa
/// ligne de séparation (`--frontière`) jusqu'à celui qui précède la suivante.
/// La retirer laisse un multipart bien formé.
fn plage(message: &[u8], p: &mail_parser::MessagePart) -> Option<std::ops::Range<usize>> {
    let (tete, corps) = (p.offset_header as usize, p.offset_body as usize);
    let avant = message.get(..tete)?.strip_suffix(b"\n")?;
    let avant = avant.strip_suffix(b"\r").unwrap_or(avant);
    let ligne = avant.iter().rposition(|&c| c == b'\n')? + 1;
    let separation = message[ligne..avant.len()].trim_ascii_end();
    if separation.len() < 3 || !separation.starts_with(b"--") {
        return None;
    }
    let debut = if ligne >= 2 && message[ligne - 2] == b'\r' { ligne - 2 } else { ligne - 1 };
    let mut motif = b"\n".to_vec();
    motif.extend_from_slice(separation);
    let mut depuis = corps;
    let suivante = loop {
        let i = depuis + message.get(depuis..)?.windows(motif.len()).position(|f| f == motif.as_slice())?;
        // La frontière entière, pas le début d'une autre plus longue.
        match message.get(i + motif.len()) {
            None | Some(b'\r' | b'\n' | b'-' | b' ' | b'\t') => break i,
            _ => depuis = i + 1,
        }
    };
    let fin = if suivante > 0 && message[suivante - 1] == b'\r' { suivante - 1 } else { suivante };
    (debut < fin).then_some(debut..fin)
}

/// Nom, type et contenu décodé d'une pièce jointe. Décodée depuis le message
/// brut, et non lue chez l'analyseur : il convertit en UTF-8 le texte d'un
/// fichier, que Microsoft doit recevoir tel quel.
fn piece(message: &[u8], p: &mail_parser::MessagePart) -> Option<Piece> {
    use mail_parser::decoders::{base64::base64_decode, quoted_printable::quoted_printable_decode};
    use mail_parser::{Encoding, MimeHeaders};
    let brut = message.get(p.offset_body as usize..p.offset_end as usize)?;
    let contenu = match p.encoding {
        Encoding::Base64 => base64_decode(brut)?,
        Encoding::QuotedPrintable => quoted_printable_decode(brut)?,
        Encoding::None => brut.to_vec(),
    };
    let nom = p.attachment_name().filter(|n| !n.trim().is_empty()).unwrap_or("piece-jointe").to_string();
    let type_mime = p
        .content_type()
        .map(|t| match t.subtype() {
            Some(s) => format!("{}/{}", t.ctype(), s),
            None => t.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into());
    Some(Piece { nom, type_mime, contenu })
}

/// Refus de Graph : `{"error": {"code", "message"}}`.
fn erreur_graph(statut: u16, corps: &[u8]) -> ErreurOAuth {
    let json: serde_json::Value = serde_json::from_slice(corps).unwrap_or_default();
    let erreur = json.get("error");
    let texte = |cle: &str| erreur.and_then(|e| e.get(cle)).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let code = texte("code");
    ErreurOAuth::Refus {
        code: if code.is_empty() { format!("HTTP {statut}") } else { code },
        description: texte("message"),
    }
}

/// Le message, avec un en-tête `Bcc` pour les destinataires de l'enveloppe
/// absents de `To` et `Cc`.
pub fn avec_cci(octets: &[u8], destinataires: &[String]) -> Vec<u8> {
    let message = mail_parser::MessageParser::default().parse_headers(octets);
    let mut presents: Vec<String> = Vec::new();
    if let Some(m) = &message {
        for liste in [m.to(), m.cc()].into_iter().flatten() {
            presents.extend(liste.iter().filter_map(|a| a.address()).map(|a| a.to_lowercase()));
        }
    }
    let caches: Vec<&String> =
        destinataires.iter().filter(|d| !presents.contains(&d.trim().to_lowercase())).collect();
    if caches.is_empty() {
        return octets.to_vec();
    }
    let liste: Vec<String> = caches.iter().map(|d| d.trim().to_string()).collect();
    let mut sortie = format!("Bcc: {}\r\n", liste.join(", ")).into_bytes();
    sortie.extend_from_slice(octets);
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defi_pkce_de_la_rfc() {
        // RFC 7636, annexe B.
        assert_eq!(
            defi_de("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let p = pkce();
        assert_eq!(p.verificateur.len(), 43, "32 octets en base64url");
        assert!(p.verificateur.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        assert_eq!(p.defi, defi_de(&p.verificateur));
        assert_ne!(etat(), etat());
    }

    #[test]
    fn adresse_de_connexion() {
        let url = url_autorisation("abc-123", "http://localhost:4321", "DEFI", "ETAT", "");
        assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?client_id=abc-123&"));
        assert!(url.contains("&response_type=code&"));
        assert!(url.contains("&redirect_uri=http%3A%2F%2Flocalhost%3A4321&"));
        assert!(url.contains("&state=ETAT&code_challenge=DEFI&code_challenge_method=S256&prompt=select_account"));
        assert!(url.contains(
            "&scope=offline_access%20openid%20email%20https%3A%2F%2Foutlook.office.com%2FIMAP.AccessAsUser.All%20\
             https%3A%2F%2Fgraph.microsoft.com%2FMail.Send%20https%3A%2F%2Fgraph.microsoft.com%2FMail.ReadWrite&"
        ));
        assert!(!url.contains("login_hint"));
        let url = url_autorisation("abc", "http://localhost:1", "d", "e", " jean@exemple.fr ");
        assert!(url.starts_with("https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize?"));
        assert!(url.ends_with("&login_hint=jean%40exemple.fr"));
        // Points d'entrée : organisation, particuliers, commun.
        assert_eq!(autorite("fabienne.velozo@pollenergie.fr"), "https://login.microsoftonline.com/organizations/oauth2/v2.0");
        assert_eq!(autorite("Paul@Hotmail.fr"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@outlook.com"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@live.fr"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@outlookfr.com"), "https://login.microsoftonline.com/organizations/oauth2/v2.0");
        assert_eq!(autorite(""), "https://login.microsoftonline.com/common/oauth2/v2.0");
        assert_eq!(
            url_consentement("abc-123"),
            "https://login.microsoftonline.com/organizations/adminconsent?client_id=abc-123"
        );
    }

    #[test]
    fn chaine_xoauth2() {
        let attendu = crate::smtp::base64(b"user=jean@exemple.fr\x01auth=Bearer JETON\x01\x01");
        assert_eq!(xoauth2("jean@exemple.fr", "JETON"), attendu);
        assert_eq!(
            crate::rendu::base64(&xoauth2("a@b.fr", "t")).unwrap(),
            b"user=a@b.fr\x01auth=Bearer t\x01\x01"
        );
    }

    #[test]
    fn reponses_du_point_de_jeton() {
        let r = lire_reponse_jeton(
            200,
            br#"{"token_type":"Bearer","scope":"x","expires_in":3599,"access_token":"AT","refresh_token":"RT","id_token":"a.b.c"}"#,
        )
        .unwrap();
        assert_eq!(r.acces, "AT");
        assert_eq!(r.rafraichissement.as_deref(), Some("RT"));
        assert_eq!(r.duree, 3599);
        assert_eq!(r.id_token.as_deref(), Some("a.b.c"));
        let r = lire_reponse_jeton(200, br#"{"access_token":"AT","expires_in":"60"}"#).unwrap();
        assert_eq!((r.rafraichissement, r.duree), (None, 60));

        let e = lire_reponse_jeton(
            400,
            br#"{"error":"invalid_grant","error_description":"AADSTS65001: The user or administrator has not consented to use the application."}"#,
        )
        .unwrap_err();
        assert!(e.consentement());
        assert!(e.to_string().contains("AADSTS65001"));
        let e = lire_reponse_jeton(400, br#"{"error":"invalid_grant","error_description":"AADSTS70008: expired"}"#)
            .unwrap_err();
        assert!(!e.consentement());
        assert!(matches!(Erreur::from(e), Erreur::Refuse(_)));
        assert!(matches!(lire_reponse_jeton(502, b"<html>").unwrap_err(), ErreurOAuth::Reseau(_)));
        assert!(matches!(lire_reponse_jeton(200, b"{}").unwrap_err(), ErreurOAuth::Reseau(_)));
    }

    #[test]
    fn adresse_tiree_de_l_id_token() {
        let jeton = |charge: &str| format!("e30.{}.sig", base64url(charge.as_bytes()));
        assert_eq!(
            adresse_id_token(&jeton(r#"{"email":"Jean@Exemple.FR","preferred_username":"autre@x.fr"}"#)),
            Some("jean@exemple.fr".into())
        );
        assert_eq!(
            adresse_id_token(&jeton(r#"{"preferred_username":"jean@exemple.onmicrosoft.com"}"#)),
            Some("jean@exemple.onmicrosoft.com".into())
        );
        assert_eq!(adresse_id_token(&jeton(r#"{"preferred_username":"+33600000000"}"#)), None);
        assert_eq!(adresse_id_token("illisible"), None);
    }

    #[test]
    fn retour_du_navigateur() {
        let ok = lire_requete("GET /?code=0.AXo-abc%2Fdef&state=E1&session_state=x HTTP/1.1", "E1");
        assert_eq!(ok, Some(Ok("0.AXo-abc/def".into())));
        let etranger = lire_requete("GET /?code=C&state=AUTRE HTTP/1.1", "E1").unwrap().unwrap_err();
        assert!(matches!(etranger, ErreurOAuth::Refus { ref code, .. } if code == "state"));
        let refus = lire_requete(
            "GET /?error=access_denied&error_description=AADSTS65004%3a+User+declined+to+consent.&state=E1 HTTP/1.1",
            "E1",
        )
        .unwrap()
        .unwrap_err();
        assert_eq!(
            refus,
            ErreurOAuth::Refus { code: "access_denied".into(), description: "AADSTS65004: User declined to consent.".into() }
        );
        let admin = lire_requete(
            "GET /?error=consent_required&error_description=AADSTS90094%3A+Admin+approval&state=E1 HTTP/1.1",
            "E1",
        )
        .unwrap()
        .unwrap_err();
        assert!(admin.consentement());
        assert_eq!(lire_requete("GET /favicon.ico HTTP/1.1", "E1"), None);
        assert_eq!(lire_requete("GET / HTTP/1.1", "E1"), None);
        assert_eq!(lire_requete("POST /?code=C&state=E1 HTTP/1.1", "E1"), None);
    }

    #[test]
    fn ecoute_locale_de_bout_en_bout() {
        let ecoute = ecouter().unwrap();
        let port = ecoute.redirection.rsplit(':').next().unwrap().parse::<u16>().unwrap();
        let navigateur = std::thread::spawn(move || {
            let mut pages = Vec::new();
            for requete in ["GET /favicon.ico HTTP/1.1\r\n\r\n", "GET /?code=CODE&state=E1 HTTP/1.1\r\nHost: x\r\n\r\n"] {
                let mut flux = TcpStream::connect(("127.0.0.1", port)).unwrap();
                flux.write_all(requete.as_bytes()).unwrap();
                let mut page = String::new();
                flux.read_to_string(&mut page).unwrap();
                pages.push(page);
            }
            pages
        });
        assert_eq!(attendre_code(&ecoute, "E1", Duration::from_secs(10)), Ok("CODE".into()));
        let pages = navigateur.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"));
        assert!(pages[1].starts_with("HTTP/1.1 200") && pages[1].contains("revenir à MMail"));
    }

    #[test]
    fn jetons_partages() {
        let p = partager("RT");
        retenir(&p, PORTEE_IMAP, "AT", 3600);
        assert_eq!(jeton_acces(&p, PORTEE_IMAP), Ok("AT".into()), "encore bon : pas de demande");
        assert_eq!(a_conserver(&p), None);
        oublier(&p, PORTEE_IMAP);
        verrouiller(&p).renouvele = true;
        assert_eq!(a_conserver(&p), Some("RT".into()));
        assert_eq!(a_conserver(&p), None, "une fois");
    }

    #[test]
    fn cci_ajoutes_pour_graph() {
        let message = b"From: a@x.fr\r\nTo: B <b@x.fr>\r\nCc: c@x.fr\r\nSubject: s\r\n\r\ncorps\r\n";
        let tous = ["b@x.fr".to_string(), "C@x.fr".to_string()];
        assert_eq!(avec_cci(message, &tous), message.to_vec(), "rien de caché : inchangé");
        let avec = ["b@x.fr".to_string(), "c@x.fr".to_string(), "d@x.fr".to_string(), "e@x.fr".to_string()];
        let sortie = String::from_utf8(avec_cci(message, &avec)).unwrap();
        assert!(sortie.starts_with("Bcc: d@x.fr, e@x.fr\r\nFrom: a@x.fr\r\n"));
    }

    /// Contenu sans motif, que la compression ni le hasard n'arrangent.
    fn octets(n: usize, graine: u32) -> Vec<u8> {
        let mut x = graine;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect()
    }

    fn message_avec_pieces(pieces: &[(&str, &str, Vec<u8>)]) -> Vec<u8> {
        let mut m = mail_builder::MessageBuilder::new()
            .from(("Jean", "jean@exemple.fr"))
            .to(vec![("Paul", "paul@exemple.fr")])
            .subject("Essai")
            .text_body("Bonjour")
            .html_body("<p>Bonjour</p>");
        for (nom, type_mime, contenu) in pieces {
            m = m.attachment(type_mime.to_string(), nom.to_string(), contenu.clone());
        }
        m.write_to_vec().unwrap()
    }

    fn noms_des_pieces(message: &[u8]) -> Vec<String> {
        use mail_parser::MimeHeaders;
        let m = mail_parser::MessageParser::default().parse(message).expect("message lisible");
        m.attachments().map(|p| p.attachment_name().unwrap_or_default().to_string()).collect()
    }

    #[test]
    fn pieces_jointes_retirees_du_message() {
        // Encodées en base64 dans le message, puis encodées de nouveau pour
        // Graph : 2,5 et 1,5 Mo de fichiers n'y tiennent pas ensemble.
        let a = octets(2_500_000, 1);
        let b = octets(1_500_000, 2);
        let message = message_avec_pieces(&[
            ("a.bin", "application/octet-stream", a.clone()),
            ("b.pdf", "application/pdf", b.clone()),
            ("c.txt", "text/plain", b"petit".to_vec()),
        ]);
        let (reduit, pieces) = alleger(&message).unwrap();
        // La plus grosse part, et elle seule : le reste tient.
        assert_eq!(pieces, vec![Piece { nom: "a.bin".into(), type_mime: "application/octet-stream".into(), contenu: a }]);
        assert!(longueur_base64(reduit.len()) <= ENVOI_MAX);
        assert_eq!(noms_des_pieces(&reduit), ["b.pdf", "c.txt"]);
        let lu = mail_parser::MessageParser::default().parse(&reduit).unwrap();
        assert_eq!(lu.body_text(0).as_deref(), Some("Bonjour"));
        assert_eq!(lu.attachment(0).unwrap().contents(), b.as_slice());

        // Les images du corps ne partent pas : trop lourd sans pièce jointe,
        // le message est refusé.
        let image = octets(4_000_000, 3);
        let lourd = mail_builder::MessageBuilder::new()
            .from("jean@exemple.fr")
            .to("paul@exemple.fr")
            .html_body("<img src=\"cid:i1\">")
            .inline("image/png", "i1", image)
            .write_to_vec()
            .unwrap();
        let e = alleger(&lourd).unwrap_err();
        assert!(matches!(e, ErreurOAuth::Refus { ref code, .. } if code == "taille"), "{e}");
    }

    #[test]
    fn piece_jointe_rendue_telle_quelle() {
        // Du Latin-1 en quoted-printable, du texte brut : les octets du
        // fichier, sans conversion en UTF-8.
        let message = b"From: a@x.fr\r\nTo: b@x.fr\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
            Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n\
            --B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\ncorps\r\n\
            --B\r\nContent-Type: text/plain; charset=iso-8859-1\r\n\
            Content-Disposition: attachment; filename=\"a.txt\"\r\n\
            Content-Transfer-Encoding: quoted-printable\r\n\r\nd=E9j=E0\r\n\
            --B\r\nContent-Type: text/csv\r\nContent-Disposition: attachment; filename=\"b.csv\"\r\n\r\nx;y\r\n1;2\r\n\
            --B--\r\n";
        let lu = mail_parser::MessageParser::default().parse(message).unwrap();
        let a = piece(message, lu.attachment(0).unwrap()).unwrap();
        assert_eq!((a.nom.as_str(), a.type_mime.as_str(), a.contenu.as_slice()), ("a.txt", "text/plain", b"d\xe9j\xe0".as_slice()));
        let b = piece(message, lu.attachment(1).unwrap()).unwrap();
        assert_eq!((b.nom.as_str(), b.contenu.as_slice()), ("b.csv", b"x;y\r\n1;2".as_slice()));

        // Retirer les deux laisse un multipart bien formé.
        let (r1, r2) = (plage(message, lu.attachment(0).unwrap()).unwrap(), plage(message, lu.attachment(1).unwrap()).unwrap());
        let mut reste = message[..r1.start].to_vec();
        reste.extend_from_slice(&message[r1.end..r2.start]);
        reste.extend_from_slice(&message[r2.end..]);
        assert!(String::from_utf8_lossy(&reste).ends_with("\r\n\r\n--B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\ncorps\r\n--B--\r\n"));
        assert!(noms_des_pieces(&reste).is_empty());
    }

    struct Requete {
        methode: String,
        url: String,
        entetes: Vec<(String, String)>,
        corps: Vec<u8>,
    }

    impl Requete {
        fn entete(&self, nom: &str) -> Option<&str> {
            self.entetes.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str())
        }
    }

    fn reponse(statut: u16, corps: &str) -> http::Reponse {
        http::Reponse {
            statut,
            type_contenu: "application/json".into(),
            redirection: None,
            corps: corps.as_bytes().to_vec(),
            adresse: String::new(),
            entetes: Vec::new(),
        }
    }

    /// Un faux Graph : il note les requêtes et répond comme le vrai — sauf
    /// au dépôt des morceaux, s'il doit échouer.
    fn graph(journal: &mut Vec<Requete>, depot_refuse: bool) -> impl FnMut(&Demande) -> crate::imap::Resultat<http::Reponse> + '_ {
        move |d: &Demande| {
            journal.push(Requete {
                methode: d.methode.into(),
                url: d.url.into(),
                entetes: d.entetes.iter().map(|(n, v)| (n.to_string(), v.clone())).collect(),
                corps: d.corps.to_vec(),
            });
            Ok(match d.methode {
                "POST" if d.url == format!("{GRAPH}/sendMail") => reponse(202, ""),
                "POST" if d.url == format!("{GRAPH}/messages") => reponse(
                    201,
                    r#"{"id":"AAMk/x=","toRecipients":[{"emailAddress":{"name":"Paul","address":"Paul@exemple.fr"}}],"bccRecipients":[]}"#,
                ),
                "POST" if d.url.ends_with("/createUploadSession") => {
                    reponse(201, r#"{"uploadUrl":"https://outlook.office.com/api/v2.0/Users('u')/Messages('m')/AttachmentSessions('s')?authtoken=abc"}"#)
                }
                "PUT" if depot_refuse => reponse(500, r#"{"error":{"code":"ErrorInternalServerError","message":"panne"}}"#),
                "PUT" => reponse(200, r#"{"nextExpectedRanges":[]}"#),
                "POST" if d.url.ends_with("/send") => reponse(202, ""),
                "PATCH" | "DELETE" => reponse(200, "{}"),
                _ => reponse(400, r#"{"error":{"code":"Inattendu","message":"requête inattendue"}}"#),
            })
        }
    }

    #[test]
    fn petit_message_en_une_requete() {
        let message = message_avec_pieces(&[("c.txt", "text/plain", b"petit".to_vec())]);
        let mut journal = Vec::new();
        envoyer_par(&mut graph(&mut journal, false), "JETON", &message, &["paul@exemple.fr".into()]).unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!((journal[0].methode.as_str(), journal[0].url.as_str()), ("POST", "https://graph.microsoft.com/v1.0/me/sendMail"));
        assert_eq!(journal[0].entete("Authorization"), Some("Bearer JETON"));
        assert_eq!(crate::rendu::base64(std::str::from_utf8(&journal[0].corps).unwrap()).unwrap(), message);
    }

    #[test]
    fn gros_message_par_un_brouillon() {
        let gros = octets(5_000_000, 7);
        let message = message_avec_pieces(&[
            ("photo.jpg", "image/jpeg", gros.clone()),
            ("devis.pdf", "application/pdf", b"%PDF-1.4".to_vec()),
        ]);
        let destinataires = ["paul@exemple.fr".to_string(), "cache@exemple.fr".to_string()];
        let mut journal = Vec::new();
        envoyer_par(&mut graph(&mut journal, false), "JETON", &message, &destinataires).unwrap();

        let brouillon = format!("{GRAPH}/messages/AAMk%2Fx%3D");
        let etapes: Vec<(&str, &str)> = journal.iter().map(|r| (r.methode.as_str(), r.url.as_str())).collect();
        let depot = "https://outlook.office.com/api/v2.0/Users('u')/Messages('m')/AttachmentSessions('s')?authtoken=abc";
        assert_eq!(
            etapes,
            [
                ("POST", format!("{GRAPH}/messages").as_str()),
                ("PATCH", brouillon.as_str()),
                ("POST", format!("{brouillon}/attachments/createUploadSession").as_str()),
                ("PUT", depot),
                ("PUT", depot),
                ("POST", format!("{brouillon}/send").as_str()),
            ]
        );
        // Le brouillon : le message sans la photo, avec la Cci en en-tête.
        let cree = crate::rendu::base64(std::str::from_utf8(&journal[0].corps).unwrap()).unwrap();
        assert_eq!(noms_des_pieces(&cree), ["devis.pdf"]);
        assert!(String::from_utf8_lossy(&cree).starts_with("Bcc: cache@exemple.fr\r\n"));
        // Graph ne l'a pas reprise : elle est ajoutée.
        let cci: serde_json::Value = serde_json::from_slice(&journal[1].corps).unwrap();
        assert_eq!(cci, serde_json::json!({ "bccRecipients": [{ "emailAddress": { "address": "cache@exemple.fr" } }] }));
        let session: serde_json::Value = serde_json::from_slice(&journal[2].corps).unwrap();
        assert_eq!(
            session,
            serde_json::json!({ "AttachmentItem": { "attachmentType": "file", "name": "photo.jpg", "size": 5_000_000, "contentType": "image/jpeg" } })
        );
        // Les morceaux : sans le jeton, bornés, la photo entière.
        assert_eq!(journal[3].entete("Content-Range"), Some("bytes 0-3276799/5000000"));
        assert_eq!(journal[4].entete("Content-Range"), Some("bytes 3276800-4999999/5000000"));
        assert!(journal[3..5].iter().all(|r| r.entete("Authorization").is_none()));
        assert_eq!([journal[3].corps.as_slice(), journal[4].corps.as_slice()].concat(), gros);
        assert!(journal.iter().filter(|r| r.methode != "PUT").all(|r| r.entete("Authorization") == Some("Bearer JETON")));
    }

    #[test]
    fn brouillon_supprime_quand_l_envoi_echoue() {
        let message = message_avec_pieces(&[("photo.jpg", "image/jpeg", octets(5_000_000, 9))]);
        let mut journal = Vec::new();
        let e = envoyer_par(&mut graph(&mut journal, true), "JETON", &message, &["paul@exemple.fr".into()]).unwrap_err();
        assert_eq!(e, ErreurOAuth::Refus { code: "ErrorInternalServerError".into(), description: "panne".into() });
        let derniere = journal.last().unwrap();
        assert_eq!((derniere.methode.as_str(), derniere.url.as_str()), ("DELETE", format!("{GRAPH}/messages/AAMk%2Fx%3D").as_str()));
        assert!(!journal.iter().any(|r| r.url.ends_with("/send")));
    }

    #[test]
    fn encodages() {
        assert_eq!(encoder("a b/c?d=é"), "a%20b%2Fc%3Fd%3D%C3%A9");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(
            parametres("a=1+2&b=%C3%A9&c"),
            vec![("a".into(), "1 2".into()), ("b".into(), "é".into()), ("c".into(), "".into())]
        );
    }

    #[test]
    fn identifiant_de_l_application() {
        // La variable d'environnement prime ; vide, c'est la constante.
        std::env::remove_var("MMAIL_MICROSOFT_CLIENT_ID");
        assert_eq!(identifiant_client(), MICROSOFT_CLIENT_ID);
        std::env::set_var("MMAIL_MICROSOFT_CLIENT_ID", " 11111111-2222-3333-4444-555555555555 ");
        assert_eq!(identifiant_client(), "11111111-2222-3333-4444-555555555555");
        assert!(disponible());
        std::env::remove_var("MMAIL_MICROSOFT_CLIENT_ID");
    }
}
