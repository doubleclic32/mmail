// SPDX-License-Identifier: GPL-3.0-or-later
//! Le poste où tourne MMail : son nom et son système, tels que la
//! restauration d'une sauvegarde les propose et que la présentation au
//! serveur IMAP (commande `ID`) les donne.

/// Nom du poste : celui de l'ordinateur.
pub fn nom() -> String {
    if cfg!(target_os = "android") {
        return "Android".to_string();
    }
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "poste".to_string())
}

/// Famille du système : Windows, Linux, Android, macOS.
pub fn famille() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "linux" => "Linux",
        "android" => "Android",
        "macos" => "macOS",
        autre => autre,
    }
}

/// Système et sa version (« Windows 11 Version 24H2 ») : Qt le nomme sur
/// toutes les cibles, Rust non — l'interface le pose dans `MMAIL_SYSTEME` au
/// démarrage. À défaut, la famille seule.
pub fn systeme() -> String {
    std::env::var("MMAIL_SYSTEME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| famille().to_string())
}
