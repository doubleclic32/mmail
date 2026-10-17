// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue d’application d’un lien de configuration.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueLien { id: dlgLien } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgLien
    title: qsTr("Ajouter avec un lien de configuration")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(520, fenetre.width - 24)
    standardButtons: Dialog.Ok | Dialog.Cancel
    function ouvrir(lien, note) {
        champLien.text = lien
        noteLien.text = note
        open()
    }
    onOpened: champLien.forceActiveFocus()
    onAccepted: fenetre.lireLien(champLien.text.trim())

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            id: noteLien
            Layout.fillWidth: true
            visible: text.length > 0
            wrapMode: Text.Wrap
            color: "#b00020"
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Collez le lien de configuration que vous avez reçu :")
        }
        TextField {
            id: champLien
            placeholderText: "https://…"
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            Layout.fillWidth: true
            onAccepted: dlgLien.accept()
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.75
            font.pixelSize: 11
            text: qsTr("Le lien ne sert qu'une fois. Il apporte l'adresse, le serveur et le mot de passe de chaque compte ; le mot de passe va directement au coffre du système, sans être affiché.")
        }
    }
}
