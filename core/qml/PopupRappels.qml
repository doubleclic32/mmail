// SPDX-License-Identifier: GPL-3.0-or-later
// Fenêtre des rappels de l’agenda.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « PopupRappels { id: popupRappels } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Popup {
    id: popupRappels
    property var rappels: []
    anchors.centerIn: Overlay.overlay
    width: Math.min(fenetre.width - 32, 480)
    height: Math.min(fenetre.height - 48, implicitHeight)
    modal: false
    focus: true
    closePolicy: Popup.NoAutoClose
    padding: 16
    contentItem: ColumnLayout {
        spacing: 10
        Label {
            text: popupRappels.rappels.length > 1 ? qsTr("%1 rappels").arg(popupRappels.rappels.length) : qsTr("Rappel")
            font.bold: true
            font.pixelSize: 17
        }
        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            implicitHeight: Math.min(contentHeight, 320)
            clip: true
            spacing: 8
            model: popupRappels.rappels
            delegate: RowLayout {
                id: ligneRappel
                required property var modelData
                width: ListView.view.width
                spacing: 8
                Rectangle {
                    Layout.preferredWidth: 10
                    Layout.preferredHeight: 10
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 5
                    radius: 3
                    color: ligneRappel.modelData.couleur || fenetre.accent
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        text: ligneRappel.modelData.titre
                        font.bold: true
                        wrapMode: Text.Wrap
                    }
                    Label {
                        Layout.fillWidth: true
                        text: ligneRappel.modelData.quand
                              + (ligneRappel.modelData.lieu.length > 0 ? " — " + ligneRappel.modelData.lieu : "")
                        wrapMode: Text.Wrap
                        opacity: 0.8
                    }
                }
                Button {
                    text: qsTr("Ouvrir")
                    flat: true
                    onClicked: {
                        fenetre.module = "agenda"
                        vueAgenda.allerAuJour(ligneRappel.modelData.date)
                    }
                }
                Button {
                    text: qsTr("Ignorer")
                    onClicked: {
                        boite.rappelVu(ligneRappel.modelData.cle)
                        fenetre.verifierRappels()
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Répéter dans") }
            ComboBox {
                id: delaiRappel
                model: [qsTr("5 minutes"), qsTr("10 minutes"), qsTr("15 minutes"), qsTr("30 minutes"), qsTr("1 heure")]
                readonly property var minutes: [5, 10, 15, 30, 60]
            }
            Button {
                text: qsTr("Répéter")
                onClicked: {
                    for (var i = 0; i < popupRappels.rappels.length; ++i)
                        boite.repousserRappel(popupRappels.rappels[i].cle, delaiRappel.minutes[delaiRappel.currentIndex])
                    fenetre.verifierRappels()
                }
            }
            Item { Layout.fillWidth: true }
            Button {
                visible: popupRappels.rappels.length > 1
                text: qsTr("Tout ignorer")
                onClicked: {
                    for (var i = 0; i < popupRappels.rappels.length; ++i)
                        boite.rappelVu(popupRappels.rappels[i].cle)
                    fenetre.verifierRappels()
                }
            }
        }
    }
}
