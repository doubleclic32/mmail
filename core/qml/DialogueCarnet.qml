// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Carnet d'adresses » (0.6.1) : les contacts des carnets CardDAV
// des comptes (SOGo pour Mailcow), à chercher et à qui écrire. Lecture seule :
// un contact se crée ou se modifie dans le webmail. Les mêmes contacts
// viennent en tête des adresses proposées à la saisie.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml, où « DialogueCarnet { id: dlgCarnet } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: dlgCarnet
    title: qsTr("Carnet d'adresses")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(640, fenetre.width - 16)
    height: Math.min(640, fenetre.height - 24)
    padding: 10
    standardButtons: Dialog.Close

    property var contacts: []
    property int ouvert: -1

    function chercher() {
        try {
            contacts = JSON.parse(boite.carnet(champRecherche.text))
        } catch (e) {
            contacts = []
        }
        ouvert = contacts.length === 1 ? 0 : -1
    }
    onOpened: {
        champRecherche.text = ""
        chercher()
        champRecherche.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        TextField {
            id: champRecherche
            Layout.fillWidth: true
            placeholderText: qsTr("Chercher : nom, organisation ou adresse")
            onTextChanged: dlgCarnet.chercher()
        }
        ListView {
            id: vueContacts
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: dlgCarnet.contacts
            ScrollBar.vertical: ScrollBar {}
            delegate: ItemDelegate {
                required property var modelData
                required property int index
                width: ListView.view.width
                highlighted: index === dlgCarnet.ouvert
                onClicked: dlgCarnet.ouvert = dlgCarnet.ouvert === index ? -1 : index
                Accessible.name: modelData.nom + (modelData.organisation ? ", " + modelData.organisation : "")
                contentItem: ColumnLayout {
                    spacing: 2
                    Label {
                        text: modelData.nom || modelData.courriels[0] || qsTr("(sans nom)")
                        font.bold: true
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: text.length > 0
                        text: [modelData.organisation, modelData.courriels.length > 0 ? modelData.courriels[0] : ""]
                              .filter(function(t) { return t }).join(" · ")
                        elide: Text.ElideRight
                        opacity: 0.75
                        Layout.fillWidth: true
                    }
                    // Le contact ouvert : toutes ses adresses, ses numéros.
                    ColumnLayout {
                        visible: index === dlgCarnet.ouvert
                        Layout.fillWidth: true
                        spacing: 2
                        Repeater {
                            model: modelData.courriels
                            delegate: RowLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                Label {
                                    text: modelData
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Button {
                                    text: qsTr("Écrire")
                                    onClicked: {
                                        dlgCarnet.close()
                                        fenetre.ouvrirLien("mailto:" + modelData)
                                    }
                                }
                            }
                        }
                        Repeater {
                            model: modelData.telephones
                            delegate: Label {
                                required property var modelData
                                text: qsTr("Tél. %1").arg(modelData)
                            }
                        }
                        Label {
                            text: qsTr("Carnet « %1 » de %2").arg(modelData.carnet).arg(modelData.compte)
                            opacity: 0.6
                            font.pointSize: dlgCarnet.font.pointSize * 0.9
                        }
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                width: parent.width - 24
                visible: dlgCarnet.contacts.length === 0
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                opacity: 0.6
                text: champRecherche.text.length > 0
                      ? qsTr("Aucun contact ne répond à cette recherche.")
                      : qsTr("Aucun contact. Les carnets d'adresses du serveur (le webmail, pour les boîtes de MMEDIA) se relisent avec les agendas, tous les quarts d'heure.")
            }
        }
    }
}
