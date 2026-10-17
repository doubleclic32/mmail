// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue des logiciels associés aux types de pièces jointes.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueLogiciels { id: dlgLogiciels } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Les logiciels retenus, par extension, sur ce poste ; « Retirer » rend le
// type au logiciel du système.
Dialog {
    id: dlgLogiciels
    property var liste: []
    function actualiser() {
        var m = fenetre.logicielsPieces()
        liste = Object.keys(m).sort().map(function(e) { return { extension: e, programme: m[e] } })
    }
    onAboutToShow: actualiser()
    title: qsTr("Logiciels d'ouverture des pièces jointes")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    standardButtons: Dialog.Close
    ColumnLayout {
        width: dlgLogiciels.availableWidth
        spacing: 4
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            text: dlgLogiciels.liste.length === 0
                  ? qsTr("Aucun : les pièces jointes s'ouvrent avec le logiciel du système. Pour en choisir un : clic droit sur une pièce, « Ouvrir avec… ».")
                  : qsTr("Sur ce poste, ces types de fichiers s'ouvrent avec le logiciel choisi ; les autres, avec celui du système.")
        }
        Repeater {
            model: dlgLogiciels.liste
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                Label {
                    text: "." + modelData.extension
                    font.bold: true
                    Layout.preferredWidth: 70
                }
                Label {
                    text: lanceur.nom(modelData.programme)
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                    HoverHandler { id: survolLogiciel }
                    ToolTip.visible: survolLogiciel.hovered
                    ToolTip.text: lanceur.chemin(modelData.programme)
                }
                Button {
                    text: qsTr("Retirer")
                    flat: true
                    onClicked: {
                        fenetre.retenirLogiciel(modelData.extension, "")
                        dlgLogiciels.actualiser()
                    }
                }
            }
        }
    }
}
