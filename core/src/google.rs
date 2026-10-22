// SPDX-License-Identifier: GPL-3.0-or-later
//! Comptes Google (Gmail) : connexion par OAuth2, lecture IMAP et envoi SMTP
//! en `XOAUTH2`.
//!
//! Gmail ne prend plus de mot de passe ordinaire en IMAP : il faut un jeton
//! d'accès, que Google délivre une fois la personne connectée dans son
//! navigateur et son accord donné. MMail est inscrit chez Google comme
//! « application de bureau » : un identifiant et un secret, livrés avec le
//! programme. Google ne tient pas ce secret pour confidentiel dans ce cas,
//! mais l'exige à l'échange du code et au rafraîchissement, PKCE ou pas.
//!
//! Flux et jetons : ceux de `oauth`, retour sur `http://127.0.0.1:<port>` —
//! Google n'accepte pas « localhost » pour une application de bureau. Une
//! seule portée, `https://mail.google.com/`, pour la lecture (IMAP) et
//! l'envoi (SMTP) ; Gmail range lui-même la copie des messages envoyés.
//!
//! Tant que Google n'a pas vérifié l'application, seuls ses « utilisateurs de
//! test » peuvent s'y connecter, et leur jeton de rafraîchissement expire au
//! bout de sept jours si elle reste « En test » : le compte demande alors à
//! se reconnecter à Google.

use std::time::Duration;

use crate::oauth::{encoder, Ecoute, ErreurOAuth, Fournisseur, Partage, ReponseJeton};

const AUTORISATION: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const POINT_JETON: &str = "https://oauth2.googleapis.com/token";

/// Serveurs de Gmail.
pub const HOTE_IMAP: &str = "imap.gmail.com";
pub const HOTE_SMTP: &str = "smtp.gmail.com";

/// Lecture et envoi : tout le courrier, IMAP comme SMTP.
pub const PORTEE_MAIL: &str = "https://mail.google.com/";

/// Ce que la connexion demande : le courrier, et `openid email` pour
/// l'adresse du compte.
const PORTEE_AUTORISATION: &str = "openid email https://mail.google.com/";

/// Client « application de bureau » de MMail chez Google (projet « MMail »).
/// Son secret n'est pas confidentiel pour une application de bureau — il est
/// livré dans le programme —, mais il n'est pas écrit ici : les détecteurs de
/// secrets le signaleraient. Il est donné à la compilation
/// (`MMAIL_GOOGLE_CLIENT_SECRET`, secret de l'intégration continue) ; sans
/// lui, la connexion Google est indisponible.
const GOOGLE_CLIENT_ID: &str = "40879489364-nsefhmvm7mv1mq3rc9phfgllhqvs2nbs.apps.googleusercontent.com";
const GOOGLE_CLIENT_SECRET: &str = match option_env!("MMAIL_GOOGLE_CLIENT_SECRET") {
    Some(secret) => secret,
    None => "",
};

fn reglage(variable: &str, defaut: &str) -> String {
    std::env::var(variable)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| defaut.to_string())
}

/// Identifiant du client : `MMAIL_GOOGLE_CLIENT_ID` s'il est donné (essais,
/// autre inscription), sinon celui du programme.
pub fn identifiant_client() -> String {
    reglage("MMAIL_GOOGLE_CLIENT_ID", GOOGLE_CLIENT_ID)
}

/// Secret du client : `MMAIL_GOOGLE_CLIENT_SECRET`, sinon celui du programme.
pub fn secret_client() -> String {
    reglage("MMAIL_GOOGLE_CLIENT_SECRET", GOOGLE_CLIENT_SECRET)
}

/// Vrai si MMail a un client Google : sans lui, pas de compte Google.
pub fn disponible() -> bool {
    !identifiant_client().is_empty() && !secret_client().is_empty()
}

/// L'application Google de MMail est-elle publiée ? Passée en production le
/// 19/10/2026 : tout compte Gmail peut s'y connecter, avec l'avertissement
/// « application non vérifiée » tant que Google ne l'a pas vérifiée. L'ajout
/// est donc ouvert, sur le bureau seulement : la connexion par le navigateur
/// et l'écoute locale n'ont pas été éprouvées sous Android.
const APPLICATION_PUBLIEE: bool = cfg!(not(target_os = "android"));

/// Vrai si l'ajout d'un compte Google s'affiche : application publiée, ou
/// `MMAIL_GOOGLE_ESSAI` posée (utilisateurs de test).
pub fn ajout_ouvert() -> bool {
    disponible() && (APPLICATION_PUBLIEE || !reglage("MMAIL_GOOGLE_ESSAI", "").is_empty())
}

/// Écoute locale du retour de Google : `http://127.0.0.1:<port>`.
pub fn ecouter() -> std::io::Result<Ecoute> {
    crate::oauth::ecouter("127.0.0.1")
}

/// Attend le retour du navigateur (cf. `oauth::attendre_code`).
pub fn attendre_code(ecoute: &Ecoute, etat: &str, delai: Duration) -> Result<String, ErreurOAuth> {
    crate::oauth::attendre_code(ecoute, etat, delai, "Google")
}

/// Jetons d'un compte Google, partagés entre ses sessions.
pub fn partager(rafraichissement: &str) -> Partage {
    crate::oauth::partager(Fournisseur::Google, rafraichissement)
}

/// Adresse de la page de connexion de Google. `access_type=offline` et
/// `prompt=consent` : sans eux, Google ne rend de jeton de rafraîchissement
/// qu'à la toute première autorisation, et une reconnexion n'en aurait pas.
/// `indice` : l'adresse du compte quand on le reconnecte.
pub fn url_autorisation(client: &str, redirection: &str, defi: &str, etat: &str, indice: &str) -> String {
    let mut url = format!(
        "{AUTORISATION}?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}\
         &code_challenge={}&code_challenge_method=S256&access_type=offline&prompt={}",
        encoder(client),
        encoder(redirection),
        encoder(PORTEE_AUTORISATION),
        encoder(etat),
        encoder(defi),
        encoder("consent select_account"),
    );
    if !indice.trim().is_empty() {
        url.push_str(&format!("&login_hint={}", encoder(indice.trim())));
    }
    url
}

/// Corps de l'échange du code contre les premiers jetons.
fn corps_echange(client: &str, secret: &str, code: &str, verificateur: &str, redirection: &str) -> String {
    format!(
        "client_id={}&client_secret={}&grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}",
        encoder(client),
        encoder(secret),
        encoder(code),
        encoder(redirection),
        encoder(verificateur),
    )
}

/// Corps d'un rafraîchissement : Google rend un jeton d'accès pour toutes
/// les portées accordées, sans nouveau jeton de rafraîchissement.
fn corps_rafraichissement(client: &str, secret: &str, rafraichissement: &str) -> String {
    format!(
        "client_id={}&client_secret={}&grant_type=refresh_token&refresh_token={}",
        encoder(client),
        encoder(secret),
        encoder(rafraichissement),
    )
}

/// Échange le code reçu par l'écoute locale contre les premiers jetons.
pub fn echanger_code(
    client: &str,
    secret: &str,
    code: &str,
    verificateur: &str,
    redirection: &str,
) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(POINT_JETON, &corps_echange(client, secret, code, verificateur, redirection))
}

/// Refuse un échange où la messagerie n'a pas été accordée : sur sa page
/// d'autorisation, Google présente l'accès à Gmail comme une case à cocher,
/// décochée d'origine. Sans elle, le jeton ne vaut que pour l'adresse et
/// Gmail le refuse en IMAP (« Invalid credentials », constaté le 15/10).
pub fn verifier_portee(r: ReponseJeton) -> Result<ReponseJeton, ErreurOAuth> {
    match r.portees.as_deref() {
        Some(portees) if !portees.split_whitespace().any(|p| p == PORTEE_MAIL) => Err(ErreurOAuth::refus(
            "portee",
            "l'accès à la messagerie n'a pas été accordé",
        )),
        _ => Ok(r),
    }
}

/// Jeton d'accès par le jeton de rafraîchissement.
pub fn rafraichir(client: &str, secret: &str, rafraichissement: &str) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(POINT_JETON, &corps_rafraichissement(client, secret, rafraichissement))
}

/// Point de révocation de Google (RFC 7009).
const POINT_REVOCATION: &str = "https://oauth2.googleapis.com/revoke";

/// Révoque un jeton de rafraîchissement : le compte retiré de MMail ne garde
/// pas chez Google un accès que plus rien n'utilise (0.6.4). Google retire
/// avec lui l'accord donné à MMail sur ce compte.
pub fn revoquer(rafraichissement: &str) -> Result<(), ErreurOAuth> {
    let corps = format!("token={}", encoder(rafraichissement));
    let entetes = [("Content-Type", "application/x-www-form-urlencoded".to_string())];
    let reponse = crate::http::envoyer(&crate::http::Demande {
        methode: "POST",
        url: POINT_REVOCATION,
        accepte: "application/json",
        entetes: &entetes,
        corps: corps.as_bytes(),
        taille_max: 64 * 1024,
    })
    .map_err(|e| ErreurOAuth::Reseau(e.to_string()))?;
    match reponse.statut {
        200 => Ok(()),
        // Déjà révoqué ou expiré : le but est atteint.
        400 => Ok(()),
        statut => Err(ErreurOAuth::refus(&format!("HTTP {statut}"), "révocation refusée par Google")),
    }
}

/// Rafraîchissement d'un compte, avec le client du programme.
pub(crate) fn rafraichir_compte(rafraichissement: &str) -> Result<ReponseJeton, ErreurOAuth> {
    let (client, secret) = (identifiant_client(), secret_client());
    if client.is_empty() || secret.is_empty() {
        return Err(ErreurOAuth::refus("client", "MMail n'a pas de client Google"));
    }
    rafraichir(&client, &secret, rafraichissement)
}

/// Ce qu'un refus de Google veut dire, à montrer tel quel.
pub fn message_refus(e: &ErreurOAuth) -> String {
    match e {
        ErreurOAuth::Refus { code, .. } if code == "access_denied" => "Google a refusé l'accès : connexion annulée, \
             ou compte qui n'est pas encore autorisé — tant que Google n'a pas vérifié MMail, seuls ses \
             utilisateurs de test peuvent s'y connecter."
            .into(),
        ErreurOAuth::Refus { code, .. } if code == "portee" => "Google n'a pas accordé à MMail l'accès à la \
             messagerie : sur la page d'autorisation de Google, cochez « Lire, rédiger, envoyer et supprimer \
             définitivement des e-mails dans Gmail », puis recommencez."
            .into(),
        ErreurOAuth::Refus { code, .. } if code == "invalid_grant" => {
            "Google a refusé le jeton du compte, expiré ou révoqué : reconnectez-vous à Google.".into()
        }
        autre => autre.texte("Google"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresse_de_connexion() {
        let url = url_autorisation("123.apps.googleusercontent.com", "http://127.0.0.1:4321", "DEFI", "ETAT", "");
        assert!(url.starts_with(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id=123.apps.googleusercontent.com&response_type=code&"
        ));
        assert!(url.contains("&redirect_uri=http%3A%2F%2F127.0.0.1%3A4321&"));
        assert!(url.contains("&scope=openid%20email%20https%3A%2F%2Fmail.google.com%2F&"));
        assert!(url.contains("&state=ETAT&code_challenge=DEFI&code_challenge_method=S256&"));
        assert!(url.ends_with("&access_type=offline&prompt=consent%20select_account"));
        let url = url_autorisation("c", "http://127.0.0.1:1", "d", "e", " jean@gmail.com ");
        assert!(url.ends_with("&login_hint=jean%40gmail.com"));
    }

    #[test]
    fn corps_des_demandes_de_jeton() {
        assert_eq!(
            corps_echange("ID", "S/E", "4/0A b", "VERIF", "http://127.0.0.1:9"),
            "client_id=ID&client_secret=S%2FE&grant_type=authorization_code&code=4%2F0A%20b\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A9&code_verifier=VERIF"
        );
        assert_eq!(
            corps_rafraichissement("ID", "SEC", "1//0g"),
            "client_id=ID&client_secret=SEC&grant_type=refresh_token&refresh_token=1%2F%2F0g"
        );
    }

    #[test]
    fn refus_expliques() {
        let annule = ErreurOAuth::Refus { code: "access_denied".into(), description: String::new() };
        assert!(message_refus(&annule).contains("utilisateurs de test"));
        let expire = ErreurOAuth::Refus { code: "invalid_grant".into(), description: "Token has been expired or revoked.".into() };
        assert!(message_refus(&expire).contains("reconnectez-vous à Google"));
        let sans_gmail = ErreurOAuth::Refus { code: "portee".into(), description: String::new() };
        assert!(message_refus(&sans_gmail).contains("cochez"));
        let reseau = ErreurOAuth::Reseau("délai dépassé".into());
        assert_eq!(message_refus(&reseau), "Google injoignable : délai dépassé");
    }

    #[test]
    fn messagerie_accordee_ou_non() {
        let reponse = |portees: Option<&str>| ReponseJeton {
            acces: "AT".into(),
            rafraichissement: Some("RT".into()),
            duree: 3599,
            id_token: None,
            portees: portees.map(str::to_string),
        };
        // Réponse réelle de Google, case de Gmail cochée, puis décochée.
        let accordee = reponse(Some("https://mail.google.com/ openid https://www.googleapis.com/auth/userinfo.email"));
        assert!(verifier_portee(accordee).is_ok());
        let refusee = reponse(Some("openid https://www.googleapis.com/auth/userinfo.email"));
        let e = verifier_portee(refusee).unwrap_err();
        assert!(matches!(e, ErreurOAuth::Refus { ref code, .. } if code == "portee"));
        // Sans `scope` dans la réponse, rien à vérifier : IMAP tranchera.
        assert!(verifier_portee(reponse(None)).is_ok());
    }

    #[test]
    fn client_de_l_application() {
        // Variables d'environnement d'abord ; sans client, pas de compte Google.
        std::env::remove_var("MMAIL_GOOGLE_CLIENT_ID");
        std::env::remove_var("MMAIL_GOOGLE_CLIENT_SECRET");
        assert_eq!(identifiant_client(), GOOGLE_CLIENT_ID);
        assert_eq!(disponible(), !GOOGLE_CLIENT_ID.is_empty() && !GOOGLE_CLIENT_SECRET.is_empty());
        std::env::set_var("MMAIL_GOOGLE_CLIENT_ID", " 42.apps.googleusercontent.com ");
        std::env::set_var("MMAIL_GOOGLE_CLIENT_SECRET", "GOCSPX-essai");
        assert_eq!(identifiant_client(), "42.apps.googleusercontent.com");
        assert_eq!(secret_client(), "GOCSPX-essai");
        assert!(disponible());
        // Ajout d'un compte : masqué tant que l'application Google est « En
        // test », sauf MMAIL_GOOGLE_ESSAI.
        std::env::remove_var("MMAIL_GOOGLE_ESSAI");
        assert_eq!(ajout_ouvert(), APPLICATION_PUBLIEE);
        std::env::set_var("MMAIL_GOOGLE_ESSAI", "1");
        assert!(ajout_ouvert());
        std::env::remove_var("MMAIL_GOOGLE_ESSAI");
        std::env::remove_var("MMAIL_GOOGLE_CLIENT_ID");
        std::env::remove_var("MMAIL_GOOGLE_CLIENT_SECRET");
        if GOOGLE_CLIENT_ID.is_empty() {
            let e = rafraichir_compte("RT").unwrap_err();
            assert!(matches!(e, ErreurOAuth::Refus { ref code, .. } if code == "client"), "sans client, aucune demande");
        }
    }
}
