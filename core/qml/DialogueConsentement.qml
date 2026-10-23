// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue de consentement à l’ouverture d’un compte Microsoft ou Google.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueConsentement { id: dlgConsentement } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Refus de consentement : l'organisation veut que son administrateur
// autorise MMail, une fois pour tous ses utilisateurs, par un lien.
Dialog {
    id: dlgConsentement
    // Désignés hors de ce fichier (Main.qml, fichiers voisins) : dlgConsentement.<nom>.
    property alias champConsentement: champConsentement
    property string motif: ""
    function ouvrir(m) {
        motif = m
        open()
    }
    title: qsTr("Autorisation de l'administrateur nécessaire")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    standardButtons: Dialog.Close
    ColumnLayout {
        width: dlgConsentement.availableWidth
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Votre organisation ne laisse pas ses utilisateurs autoriser eux-mêmes une application. Transmettez ce lien à l'administrateur de votre Microsoft 365 : en l'ouvrant, il autorise MMail une fois pour toute l'organisation. Vous pourrez ensuite ajouter votre compte.")
        }
        RowLayout {
            Layout.fillWidth: true
            TextField {
                id: champConsentement
                MenuSaisie {}
                Layout.fillWidth: true
                readOnly: true
                selectByMouse: true
                text: boite.consentementMicrosoft()
            }
            Button {
                text: qsTr("Copier le lien")
                onClicked: fenetre.copierTexte(champConsentement.text, "message")
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.75
            font.pixelSize: 11
            text: dlgConsentement.motif
        }
    }
}
