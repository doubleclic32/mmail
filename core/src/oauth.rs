// SPDX-License-Identifier: GPL-3.0-or-later
//! Connexion OAuth2 des comptes sans mot de passe — Microsoft (`microsoft`),
//! Google (`google`) : ce qui leur est commun.
//!
//! Flux « code d'autorisation » avec PKCE (RFC 7636), retour sur une écoute
//! locale (RFC 8252 § 7.3) : la connexion se fait dans le navigateur du
//! système, MMail ne voit jamais le mot de passe. Le jeton de
//! rafraîchissement tient lieu de mot de passe : il va au coffre du système.
//! Les jetons d'accès (une heure) ne vivent qu'en mémoire, partagés par les
//! sessions d'un compte (`Partage`).

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aws_lc_rs::rand::SecureRandom;
use aws_lc_rs::{digest, rand};

use crate::http::{self, Demande};
use crate::imap::Erreur;

/// Délai laissé à la personne pour se connecter dans son navigateur.
pub const DELAI_CONNEXION: Duration = Duration::from_secs(300);

/// Service d'un compte à jetons : il dit comment rafraîchir ses jetons et à
/// quel serveur IMAP se connecter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fournisseur {
    Microsoft,
    Google,
}

impl Fournisseur {
    pub fn nom(self) -> &'static str {
        match self {
            Fournisseur::Microsoft => "Microsoft",
            Fournisseur::Google => "Google",
        }
    }

    /// Serveur IMAP du service.
    pub fn hote_imap(self) -> &'static str {
        match self {
            Fournisseur::Microsoft => crate::microsoft::HOTE_IMAP,
            Fournisseur::Google => crate::google::HOTE_IMAP,
        }
    }

    /// Mode d'authentification du compte dans l'index (`accounts.auth`).
    pub fn auth(self) -> &'static str {
        match self {
            Fournisseur::Microsoft => crate::magasin::AUTH_MICROSOFT,
            Fournisseur::Google => crate::magasin::AUTH_GOOGLE,
        }
    }

    /// Le service d'un mode d'authentification ; `None` pour un mot de passe.
    pub fn de_auth(auth: &str) -> Option<Fournisseur> {
        match auth {
            crate::magasin::AUTH_MICROSOFT => Some(Fournisseur::Microsoft),
            crate::magasin::AUTH_GOOGLE => Some(Fournisseur::Google),
            _ => None,
        }
    }

    /// Portée du jeton d'accès de la lecture (IMAP).
    pub fn portee_imap(self) -> &'static str {
        match self {
            Fournisseur::Microsoft => crate::microsoft::PORTEE_IMAP,
            Fournisseur::Google => crate::google::PORTEE_MAIL,
        }
    }

    /// Nouveau jeton d'accès par le jeton de rafraîchissement.
    fn rafraichir(self, rafraichissement: &str, portee: &str) -> Result<ReponseJeton, ErreurOAuth> {
        match self {
            Fournisseur::Microsoft => {
                let client = crate::microsoft::identifiant_client();
                if client.is_empty() {
                    return Err(ErreurOAuth::refus("client", "MMail n'a pas d'identifiant d'application Microsoft"));
                }
                crate::microsoft::rafraichir(&client, rafraichissement, portee)
            }
            Fournisseur::Google => crate::google::rafraichir_compte(rafraichissement),
        }
    }
}

/// Demande des jetons au point de jeton `url` (formulaire encodé).
pub fn demander_jeton(url: &str, corps: &str) -> Result<ReponseJeton, ErreurOAuth> {
    let entetes = [("Content-Type", "application/x-www-form-urlencoded".to_string())];
    let reponse = http::envoyer(&Demande {
        methode: "POST",
        url,
        accepte: "application/json",
        entetes: &entetes,
        corps: corps.as_bytes(),
        taille_max: 256 * 1024,
    })
    .map_err(|e| ErreurOAuth::Reseau(e.to_string()))?;
    lire_reponse_jeton(reponse.statut, &reponse.corps)
}

// ------------------------------------------------------------------ erreurs

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurOAuth {
    /// Service de connexion injoignable, ou réponse illisible.
    Reseau(String),
    /// Refus du service ou de la personne : code OAuth et description.
    Refus { code: String, description: String },
}

impl ErreurOAuth {
    pub(crate) fn refus(code: &str, description: &str) -> Self {
        ErreurOAuth::Refus { code: code.into(), description: description.into() }
    }

    /// Vrai si le refus tient au consentement : l'organisation exige que son
    /// administrateur autorise MMail (`url_consentement`).
    pub fn consentement(&self) -> bool {
        match self {
            ErreurOAuth::Refus { code, description } => {
                matches!(code.as_str(), "consent_required" | "admin_consent_required")
                    || ["AADSTS65001", "AADSTS90094", "AADSTS90095", "AADSTS90008"]
                        .iter()
                        .any(|c| description.contains(c))
            }
            ErreurOAuth::Reseau(_) => false,
        }
    }
}

impl ErreurOAuth {
    /// L'erreur, rapportée au service `nom` (« Microsoft », « Google »).
    pub fn texte(&self, nom: &str) -> String {
        match self {
            ErreurOAuth::Reseau(m) => format!("{nom} injoignable : {m}"),
            ErreurOAuth::Refus { code, description } if description.is_empty() => format!("refus de {nom} : {code}"),
            ErreurOAuth::Refus { description, .. } => format!("refus de {nom} : {description}"),
        }
    }

    /// Pour une session IMAP ou SMTP : le service injoignable est une affaire
    /// de réseau, un refus vaut des identifiants refusés.
    pub fn vers_erreur(&self, nom: &str) -> Erreur {
        match self {
            ErreurOAuth::Reseau(_) => Erreur::Reseau(self.texte(nom)),
            ErreurOAuth::Refus { .. } => Erreur::Refuse(self.texte(nom)),
        }
    }
}

/// Formulation des comptes Microsoft, les premiers venus ; un autre service
/// passe par `texte` et `vers_erreur`.
impl std::fmt::Display for ErreurOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.texte("Microsoft"))
    }
}

impl From<ErreurOAuth> for Erreur {
    fn from(e: ErreurOAuth) -> Self {
        e.vers_erreur("Microsoft")
    }
}

// --------------------------------------------------------- autorisation

/// Vérificateur et défi PKCE (RFC 7636, méthode S256).
pub struct Pkce {
    pub verificateur: String,
    pub defi: String,
}

pub fn pkce() -> Pkce {
    let verificateur = base64url(&alea::<32>());
    Pkce { defi: defi_de(&verificateur), verificateur }
}

pub(crate) fn defi_de(verificateur: &str) -> String {
    base64url(digest::digest(&digest::SHA256, verificateur.as_bytes()).as_ref())
}

/// Valeur imprévisible pour `state` : une réponse qui ne la rend pas n'est pas
/// celle de notre demande.
pub fn etat() -> String {
    base64url(&alea::<16>())
}

fn alea<const N: usize>() -> [u8; N] {
    let mut octets = [0u8; N];
    // Sans source d'aléa, il n'y a pas de connexion sûre possible : mieux vaut
    // une valeur nulle, que le service refuse (défi PKCE connu), qu'un arrêt.
    let _ = rand::SystemRandom::new().fill(&mut octets);
    octets
}

/// Écoute locale qui reçoit le retour du navigateur. Liée à 127.0.0.1 sur un
/// port choisi par le système, et, au mieux, au même port sur ::1 : le
/// navigateur peut résoudre « localhost » en IPv6. `hote` : le nom que porte
/// l'adresse de retour — « localhost » chez Microsoft, « 127.0.0.1 » chez
/// Google, qui l'exige pour une application de bureau.
pub struct Ecoute {
    ecoutes: Vec<TcpListener>,
    pub redirection: String,
}

pub fn ecouter(hote: &str) -> std::io::Result<Ecoute> {
    let v4 = TcpListener::bind("127.0.0.1:0")?;
    let port = v4.local_addr()?.port();
    let mut ecoutes = vec![v4];
    if let Ok(v6) = TcpListener::bind(("::1", port)) {
        ecoutes.push(v6);
    }
    for e in &ecoutes {
        e.set_nonblocking(true)?;
    }
    Ok(Ecoute { ecoutes, redirection: format!("http://{hote}:{port}") })
}

/// Attend le retour du navigateur et rend le code d'autorisation. Toute autre
/// requête (l'icône que demande le navigateur) reçoit un 404 et l'attente
/// continue. `nom` : le service, pour le message de délai dépassé.
pub fn attendre_code(ecoute: &Ecoute, etat: &str, delai: Duration, nom: &str) -> Result<String, ErreurOAuth> {
    let fin = Instant::now() + delai;
    while Instant::now() < fin {
        for e in &ecoute.ecoutes {
            match e.accept() {
                Ok((flux, _)) => {
                    if let Some(issue) = servir(flux, etat) {
                        return issue;
                    }
                }
                Err(x) if x.kind() == ErrorKind::WouldBlock => {}
                Err(x) => return Err(ErreurOAuth::Reseau(format!("écoute locale : {x}"))),
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(ErreurOAuth::refus("delai", &format!("connexion à {nom} non terminée dans le délai")))
}

/// Lit une requête et y répond ; `None` si ce n'est pas le retour attendu.
fn servir(mut flux: TcpStream, etat: &str) -> Option<Result<String, ErreurOAuth>> {
    let _ = flux.set_nonblocking(false);
    let _ = flux.set_read_timeout(Some(Duration::from_secs(5)));
    let mut tampon = Vec::new();
    let mut morceau = [0u8; 2048];
    while !tampon.windows(4).any(|f| f == b"\r\n\r\n") && tampon.len() < 16 * 1024 {
        match flux.read(&mut morceau) {
            Ok(0) | Err(_) => break,
            Ok(n) => tampon.extend_from_slice(&morceau[..n]),
        }
    }
    let texte = String::from_utf8_lossy(&tampon);
    let ligne = texte.lines().next().unwrap_or_default();
    let issue = lire_requete(ligne, etat);
    let (statut, page) = match &issue {
        None => ("404 Not Found", page("MMail", "Rien ici.")),
        Some(Ok(_)) => ("200 OK", page("Connexion réussie", "Vous pouvez fermer cette fenêtre et revenir à MMail.")),
        Some(Err(e)) => (
            "200 OK",
            page(
                "Connexion non aboutie",
                &format!("{} Vous pouvez fermer cette fenêtre et revenir à MMail.", echapper(&e.to_string())),
            ),
        ),
    };
    let reponse = format!(
        "HTTP/1.1 {statut}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = flux.write_all(reponse.as_bytes());
    let _ = flux.flush();
    issue
}

fn page(titre: &str, texte: &str) -> String {
    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><title>{titre}</title></head>\
         <body style=\"font-family:sans-serif;margin:3em\"><h1>{titre}</h1><p>{texte}</p></body></html>"
    )
}

fn echapper(texte: &str) -> String {
    texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Ligne de requête du navigateur (« GET /?code=…&state=… HTTP/1.1 ») : le
/// code, le refus que le service y rapporte, ou `None` si ce n'est pas un
/// retour (autre chemin, ni code ni erreur).
pub fn lire_requete(ligne: &str, etat: &str) -> Option<Result<String, ErreurOAuth>> {
    let mut morceaux = ligne.split_whitespace();
    if morceaux.next() != Some("GET") {
        return None;
    }
    let cible = morceaux.next()?;
    let (chemin, requete) = cible.split_once('?').unwrap_or((cible, ""));
    if chemin != "/" {
        return None;
    }
    let parametres = parametres(requete);
    let valeur = |nom: &str| parametres.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str());
    if valeur("code").is_none() && valeur("error").is_none() {
        return None;
    }
    if valeur("state") != Some(etat) {
        return Some(Err(ErreurOAuth::refus("state", "réponse étrangère à cette connexion (state)")));
    }
    if let Some(code) = valeur("error") {
        return Some(Err(ErreurOAuth::refus(code, valeur("error_description").unwrap_or_default())));
    }
    valeur("code").map(|c| Ok(c.to_string()))
}

/// Le code d'une adresse de retour (`fr.mmedia.mmail://oauth?code=…&state=…`),
/// contrôlée comme une requête reçue par l'écoute locale.
pub fn lire_retour(url: &str, etat: &str) -> Option<Result<String, ErreurOAuth>> {
    let requete = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    lire_requete(&format!("GET /?{requete} HTTP/1.1"), etat)
}

/// Paramètres d'une requête `a=1&b=2`, décodés (`+` vaut une espace).
pub(crate) fn parametres(requete: &str) -> Vec<(String, String)> {
    requete
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (n, v) = p.split_once('=').unwrap_or((p, ""));
            let decoder = |t: &str| crate::rendu::decoder_pourcent(&t.replace('+', " ")).unwrap_or_default();
            (decoder(n), decoder(v))
        })
        .collect()
}

// ------------------------------------------------------------------ jetons

/// Réponse du point de jeton.
#[derive(Debug, Clone, PartialEq)]
pub struct ReponseJeton {
    pub acces: String,
    /// Nouveau jeton de rafraîchissement, s'il en rend un : il remplace
    /// l'ancien.
    pub rafraichissement: Option<String>,
    /// Durée de validité du jeton d'accès, en secondes.
    pub duree: u64,
    pub id_token: Option<String>,
    /// Portées accordées (`scope`), quand le service les rend : la personne a
    /// pu en décocher une sur la page d'autorisation (Google).
    pub portees: Option<String>,
}

pub fn lire_reponse_jeton(statut: u16, corps: &[u8]) -> Result<ReponseJeton, ErreurOAuth> {
    let json: serde_json::Value = serde_json::from_slice(corps)
        .map_err(|_| ErreurOAuth::Reseau(format!("réponse illisible du point de jeton (HTTP {statut})")))?;
    let texte = |cle: &str| json.get(cle).and_then(|v| v.as_str()).map(str::to_string);
    if let Some(code) = texte("error") {
        return Err(ErreurOAuth::Refus { code, description: texte("error_description").unwrap_or_default() });
    }
    let acces = texte("access_token")
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ErreurOAuth::Reseau(format!("aucun jeton d'accès (HTTP {statut})")))?;
    let duree = json
        .get("expires_in")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(3600);
    Ok(ReponseJeton {
        acces,
        rafraichissement: texte("refresh_token"),
        duree,
        id_token: texte("id_token"),
        portees: texte("scope"),
    })
}

/// Adresse du compte, tirée de l'`id_token` : `email`, à défaut
/// `preferred_username`. Lu sans en vérifier la signature : il arrive
/// directement du point de jeton du service, en TLS, en réponse à notre
/// propre demande — il n'a transité par personne d'autre.
pub fn adresse_id_token(id_token: &str) -> Option<String> {
    let charge = id_token.split('.').nth(1)?;
    let octets = crate::rendu::base64(charge)?;
    let json: serde_json::Value = serde_json::from_slice(&octets).ok()?;
    ["email", "preferred_username"]
        .iter()
        .filter_map(|cle| json.get(*cle).and_then(|v| v.as_str()))
        .map(|a| a.trim().to_lowercase())
        .find(|a| a.contains('@'))
}

/// Ce que demande `AUTHENTICATE XOAUTH2` : la chaîne SASL, en base64.
pub fn xoauth2(utilisateur: &str, jeton: &str) -> String {
    crate::smtp::base64(format!("user={utilisateur}\x01auth=Bearer {jeton}\x01\x01").as_bytes())
}

/// Jetons d'un compte, partagés entre ses sessions : le fil du compte, ceux
/// des envois, les déplacements vers lui.
pub struct Jetons {
    pub(crate) fournisseur: Fournisseur,
    pub(crate) rafraichissement: String,
    pub(crate) acces: Vec<(&'static str, String, Instant)>,
    /// Le service a rendu un nouveau jeton de rafraîchissement, pas encore
    /// confié au coffre.
    pub(crate) renouvele: bool,
}

pub type Partage = Arc<Mutex<Jetons>>;

pub fn partager(fournisseur: Fournisseur, rafraichissement: &str) -> Partage {
    Arc::new(Mutex::new(Jetons {
        fournisseur,
        rafraichissement: rafraichissement.to_string(),
        acces: Vec::new(),
        renouvele: false,
    }))
}

/// Le service d'un compte à jetons.
pub fn fournisseur(p: &Partage) -> Fournisseur {
    verrouiller(p).fournisseur
}

pub(crate) fn verrouiller(p: &Partage) -> std::sync::MutexGuard<'_, Jetons> {
    p.lock().unwrap_or_else(|e| e.into_inner())
}

/// Garde un jeton d'accès déjà obtenu (celui de la connexion).
pub fn retenir(p: &Partage, portee: &'static str, jeton: &str, duree: u64) {
    let mut j = verrouiller(p);
    j.acces.retain(|(q, ..)| *q != portee);
    j.acces.push((portee, jeton.to_string(), echeance(duree)));
}

/// Un jeton d'accès encore bon pour `portee`, sinon un neuf. Le verrou est
/// gardé pendant la demande : deux sessions du même compte ne rafraîchissent
/// pas en même temps.
pub fn jeton_acces(p: &Partage, portee: &'static str) -> Result<String, ErreurOAuth> {
    let mut j = verrouiller(p);
    let maintenant = Instant::now();
    if let Some((_, jeton, _)) = j.acces.iter().find(|(q, _, fin)| *q == portee && maintenant < *fin) {
        return Ok(jeton.clone());
    }
    let r = j.fournisseur.rafraichir(&j.rafraichissement, portee)?;
    if let Some(neuf) = r.rafraichissement.filter(|n| !n.is_empty() && *n != j.rafraichissement) {
        j.rafraichissement = neuf;
        j.renouvele = true;
    }
    j.acces.retain(|(q, ..)| *q != portee);
    j.acces.push((portee, r.acces.clone(), echeance(r.duree)));
    Ok(r.acces)
}

/// Oublie le jeton d'accès d'une ressource : le serveur l'a refusé.
pub fn oublier(p: &Partage, portee: &str) {
    verrouiller(p).acces.retain(|(q, ..)| *q != portee);
}

/// Le jeton de rafraîchissement renouvelé, à confier au coffre — une fois.
pub fn a_conserver(p: &Partage) -> Option<String> {
    let mut j = verrouiller(p);
    std::mem::take(&mut j.renouvele).then(|| j.rafraichissement.clone())
}

/// Échéance d'un jeton : cinq minutes avant la fin annoncée, une au moins.
fn echeance(duree: u64) -> Instant {
    Instant::now() + Duration::from_secs(duree.saturating_sub(300).max(60))
}

// --------------------------------------------------------------- encodages

/// Base64 « URL » sans remplissage (RFC 4648 § 5).
pub(crate) fn base64url(octets: &[u8]) -> String {
    crate::smtp::base64(octets).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

/// Encodage d'un composant d'adresse : seuls les caractères non réservés
/// restent tels quels (RFC 3986 § 2.3).
pub(crate) fn encoder(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for octet in texte.bytes() {
        match octet {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => sortie.push(octet as char),
            _ => sortie.push_str(&format!("%{octet:02X}")),
        }
    }
    sortie
}

#[cfg(test)]
mod tests {

    #[test]
    fn adresse_de_retour() {
        let ok = lire_retour("fr.mmedia.mmail://oauth?code=0.AbC%2Bd&state=ETAT&session_state=x", "ETAT");
        assert_eq!(ok.unwrap().unwrap(), "0.AbC+d");
        assert!(lire_retour("fr.mmedia.mmail://oauth?code=c&state=AUTRE", "ETAT").unwrap().is_err());
        let refus = lire_retour("fr.mmedia.mmail://oauth?error=access_denied&error_description=non&state=ETAT", "ETAT");
        assert!(refus.unwrap().is_err());
        assert!(lire_retour("fr.mmedia.mmail://oauth", "ETAT").is_none());
    }

    use super::*;

    #[test]
    fn erreur_rapportee_au_service() {
        let e = ErreurOAuth::refus("invalid_grant", "Token has been expired or revoked.");
        assert_eq!(e.texte("Google"), "refus de Google : Token has been expired or revoked.");
        assert_eq!(e.to_string(), "refus de Microsoft : Token has been expired or revoked.");
        assert!(matches!(e.vers_erreur("Google"), Erreur::Refuse(ref m) if m.contains("Google")));
        let r = ErreurOAuth::Reseau("délai".into());
        assert!(matches!(r.vers_erreur("Google"), Erreur::Reseau(ref m) if m == "Google injoignable : délai"));
        assert_eq!(ErreurOAuth::refus("access_denied", "").texte("Google"), "refus de Google : access_denied");
    }

    #[test]
    fn ecoute_sur_127_0_0_1() {
        let ecoute = ecouter("127.0.0.1").unwrap();
        assert!(ecoute.redirection.starts_with("http://127.0.0.1:"), "{}", ecoute.redirection);
        let delai = attendre_code(&ecoute, "E", Duration::from_millis(60), "Google").unwrap_err();
        assert_eq!(delai.texte("Google"), "refus de Google : connexion à Google non terminée dans le délai");
    }

    #[test]
    fn services() {
        for f in [Fournisseur::Microsoft, Fournisseur::Google] {
            assert_eq!(Fournisseur::de_auth(f.auth()), Some(f));
        }
        assert_eq!(Fournisseur::de_auth(""), None);
        assert_eq!(Fournisseur::Google.hote_imap(), "imap.gmail.com");
        assert_eq!(Fournisseur::Microsoft.hote_imap(), "outlook.office365.com");
    }

    #[test]
    fn jetons_d_un_compte_google() {
        let p = partager(Fournisseur::Google, "RT");
        assert_eq!(fournisseur(&p), Fournisseur::Google);
        assert_eq!(Fournisseur::Google.portee_imap(), "https://mail.google.com/");
        retenir(&p, Fournisseur::Google.portee_imap(), "AT", 3600);
        assert_eq!(jeton_acces(&p, Fournisseur::Google.portee_imap()), Ok("AT".into()));
    }
}
