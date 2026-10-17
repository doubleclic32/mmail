// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue de restauration d’une configuration sauvegardée.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueRestauration { id: dlgRestauration } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgRestauration
    title: qsTr("Configuration sauvegardée")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    property int choix: 0
    readonly property var choisie: fenetre.sauvegardesProposees[choix] || null

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: fenetre.sauvegardesProposees.length > 1
                  ? qsTr("Plusieurs sauvegardes de votre configuration existent. Laquelle restaurer ?")
                  : qsTr("Une sauvegarde de votre configuration existe. La restaurer ?")
        }
        Repeater {
            model: fenetre.sauvegardesProposees
            delegate: RadioButton {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                checked: index === dlgRestauration.choix
                onClicked: dlgRestauration.choix = index
                text: qsTr("%1, du %2").arg(modelData.nom).arg(fenetre.dateLongue(modelData.date * 1000))
                      + (modelData.poste === reglagesSauvegarde.poste ? qsTr(" (ce poste)") : "")
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dlgRestauration.choisie !== null
            text: dlgRestauration.choisie
                  ? qsTr("Comptes : %1.").arg(dlgRestauration.choisie.comptes.join(", ")) : ""
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            text: qsTr("Reviennent les comptes et leurs mots de passe, les noms et signatures, les dossiers favoris et masqués, l'apparence et les réglages. Les comptes déjà présents sur ce poste restent tels quels.")
        }
    }
    footer: DialogButtonBox {
        Button {
            text: qsTr("Restaurer")
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        Button {
            text: qsTr("Ne pas restaurer")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    onAccepted: {
        if (choisie)
            fenetre.restaurerConfiguration(choisie)
    }
}
