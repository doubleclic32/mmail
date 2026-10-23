// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Nouveau dossier ».
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueNouveauDossier { id: dlgNouveauDossier } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Sous-dossier créé sur le serveur, puis montré dans l'arborescence, son
// parent déplié (signal `dossierCree`). Un refus immédiat — nom vide ou
// interdit, dossier existant, compte hors ligne — reste dans le dialogue.
Dialog {
    id: dlgNouveauDossier
    // Désignés hors de ce fichier (Main.qml, fichiers voisins) : dlgNouveauDossier.<nom>.
    property alias champNouveauDossier: champNouveauDossier
    property int compte: 0
    property string cheminParent: ""
    property string nomParent: ""
    property string motif: ""
    function ouvrir(c, ch, n) {
        compte = c; cheminParent = ch; nomParent = n; motif = ""
        champNouveauDossier.text = ""
        open()
        champNouveauDossier.forceActiveFocus()
    }
    function creer() {
        motif = boite.creerSousDossier(compte, cheminParent, champNouveauDossier.text)
        if (motif.length > 0)
            return
        messageEtat.texte = qsTr("Création de « %1 » en cours…").arg(champNouveauDossier.text.trim())
        close()
    }
    title: qsTr("Nouveau sous-dossier")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(420, fenetre.width - 24)
    closePolicy: Popup.CloseOnEscape
    // « Créer » ne ferme pas d'office : un refus laisse corriger le nom.
    footer: DialogButtonBox {
        Button {
            text: qsTr("Créer")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgNouveauDossier.creer()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    ColumnLayout {
        width: dlgNouveauDossier.availableWidth
        spacing: 6
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Dans « %1 » :").arg(dlgNouveauDossier.nomParent)
        }
        TextField {
            id: champNouveauDossier
            MenuSaisie {}
            Layout.fillWidth: true
            placeholderText: qsTr("Nom du dossier")
            onTextEdited: dlgNouveauDossier.motif = ""
            Keys.onReturnPressed: dlgNouveauDossier.creer()
            Keys.onEnterPressed: dlgNouveauDossier.creer()
        }
        Label {
            Layout.fillWidth: true
            visible: dlgNouveauDossier.motif.length > 0
            wrapMode: Text.Wrap
            color: "#b00020"
            text: dlgNouveauDossier.motif
        }
    }
}
