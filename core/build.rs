// SPDX-License-Identifier: GPL-3.0-or-later
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    // Le rcc de l'hôte compresse en zstd, absent du Qt officiel pour Android :
    // sans cette option, l'édition de liens de l'APK échoue sur
    // « undefined symbol: qt_resourceFeatureZstd ». Leçon du portage de MMdedit.
    std::env::set_var("CXX_QT_AUTORCC_OPTIONS", "--no-zstd");

    let mut qml = vec!["qml/Main.qml", "qml/Agenda.qml", "qml/Redaction.qml", "qml/LigneArborescence.qml", "qml/LigneMessage.qml", "qml/DialogueIdentite.qml", "qml/DialogueAide.qml", "qml/BandeauVersion.qml", "qml/PopupRappels.qml", "qml/MenuComptes.qml", "qml/DialogueDeplacer.qml", "qml/DialogueRestauration.qml", "qml/DialogueAPropos.qml", "qml/CompteurNonLus.qml", "qml/MarqueSuite.qml", "qml/Scenarios.qml", "qml/ReactionsNoyau.qml", "qml/VoletArborescence.qml", "qml/VoletListe.qml", "qml/VoletLecture.qml", "qml/BoutonAction.qml", "qml/EnTeteSelectionnable.qml", "qml/SiMarque.qml", "qml/DialogueCompte.qml", "qml/DialogueNouveauDossier.qml", "qml/DialogueConsentement.qml", "qml/MenuAffichage.qml", "qml/MenuPiece.qml", "qml/DialogueOuvrirAvec.qml", "qml/DialogueLogiciels.qml", "qml/MenuMessage.qml", "qml/MenuDossier.qml", "qml/MenuCompte.qml", "qml/DialogueReprise.qml", "qml/DialogueLien.qml", "qml/DialogueViderDossier.qml", "qml/Avertissement.qml", "qml/DialogueFiltres.qml", "qml/DialogueCarnet.qml", "qml/EntreeMenu.qml", "qml/MenuSaisie.qml", "qml/PastilleCategorie.qml", "qml/MenuCategories.qml", "qml/DialogueCategorie.qml", "qml/DialogueCategories.qml", "qml/FenetreMessage.qml"];
    // Sous Android seulement : la page de connexion Microsoft intégrée, qui
    // demande QtWebView (lié dans CMakeLists.txt pour Android).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        qml.push("qml/ConnexionIntegree.qml");
    }
    CxxQtBuilder::new_qml_module(QmlModule::new("fr.mmedia.mmail").qml_files(qml))
        .qt_module("Qml")
        .files(["src/socle.rs", "src/boite.rs", "src/pont_presse_papier.rs", "src/configuration.rs", "src/mise_a_jour.rs"])
        .build();
}
