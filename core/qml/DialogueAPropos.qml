// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « À propos de MMail ».
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueAPropos { id: dlgAPropos } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgAPropos
    title: qsTr("À propos de MMail")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(460, fenetre.width - 24)
    standardButtons: Dialog.Close
    ColumnLayout {
        width: dlgAPropos.availableWidth
        spacing: 8
        Label {
            text: "MMail " + Qt.application.version
            font.bold: true
            font.pointSize: fenetre.font.pointSize * 1.4
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Client de messagerie IMAP pour Windows, Linux et Android.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("© M-Media — logiciel libre, distribué sous licence GNU GPL version 3.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.StyledText
            text: qsTr("Sources : %1").arg("<a href=\"https://github.com/mmedia-fr/mmail\">github.com/mmedia-fr/mmail</a>")
            onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
        }
        // Version plus récente : son lien reste ici, même après « Jamais ».
        Label {
            visible: fenetre.versionNouvelle.length > 0
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.StyledText
            text: qsTr("Version %1 disponible : <a href=\"%2\">page de téléchargement</a>.")
                  .arg(fenetre.versionNouvelle).arg(fenetre.pageVersion)
                  + (installeur.programme() && fenetre.fichierMiseAJour.length > 0
                     && fenetre.genreMiseAJour === "setup"
                     ? " " + qsTr("Elle s'installera à la fermeture de MMail.") : "")
            onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            text: fenetre.noyau + " · Qt " + versionQt + (renduLogiciel ? " · " + qsTr("rendu logiciel") : "")
        }
    }
}
