// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Carnet d'adresses » (0.6.1) : les contacts des carnets CardDAV
// des comptes (SOGo pour Mailcow), à chercher et à qui écrire. Depuis la
// 0.6.20, ils s'y créent, s'y modifient et s'y suppriment (DialogueFiche), et
// les fiches vides et les doublons s'y nettoient (DialogueNettoyage). Les
// mêmes contacts viennent en tête des adresses proposées à la saisie.
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
    // Vrai entre une suppression demandée d'ici et la réponse du serveur.
    property bool attente: false
    property bool creationPossible: false

    /// Ouvre le carnet sur une recherche (une adresse, depuis un message).
    function ouvrirSur(texte) {
        open()
        champRecherche.text = texte
    }

    function confirmerSuppression(contact) {
        confirmation.contact = contact
        confirmation.open()
    }

    Connections {
        target: boite
        function onContactsEcrits(ok, message, conflit) {
            if (!dlgCarnet.visible)
                return
            var ouvertAvant = dlgCarnet.ouvert >= 0 ? dlgCarnet.contacts[dlgCarnet.ouvert].id : 0
            dlgCarnet.chercher()
            for (var i = 0; i < dlgCarnet.contacts.length; ++i)
                if (dlgCarnet.contacts[i].id === ouvertAvant)
                    dlgCarnet.ouvert = i
            if (dlgCarnet.attente) {
                dlgCarnet.attente = false
                fenetre.annoncer(ok ? message : qsTr("Suppression refusée : %1").arg(message), !ok)
            }
        }
        function onAgendasSynchronises(change, erreurs) {
            if (dlgCarnet.visible && change)
                dlgCarnet.chercher()
        }
    }

    Dialog {
        id: confirmation
        property var contact: ({})
        title: qsTr("Supprimer ce contact ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: confirmation.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Supprimer « %1 » du carnet « %2 » sur le serveur ? Une copie de la fiche est gardée sur ce poste.")
                      .arg(confirmation.contact.nom || (confirmation.contact.courriels || [""])[0]).arg(confirmation.contact.carnet || "")
            }
        }
        onAccepted: {
            dlgCarnet.attente = true
            boite.supprimerContacts(JSON.stringify([confirmation.contact.id]))
        }
    }

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
        creationPossible = JSON.parse(boite.carnetsModifiables()).length > 0
        champRecherche.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            TextField {
                id: champRecherche
                MenuSaisie {}
                Layout.fillWidth: true
                placeholderText: qsTr("Chercher : nom, organisation ou adresse")
                onTextChanged: dlgCarnet.chercher()
            }
            Button {
                text: qsTr("Nouveau contact…")
                enabled: dlgCarnet.creationPossible
                ToolTip.visible: hovered && !enabled
                ToolTip.delay: 400
                ToolTip.text: qsTr("Aucun carnet modifiable : il faut une boîte connectée dont le serveur a un carnet d'adresses (le webmail, pour les boîtes de MMEDIA).")
                onClicked: dlgFiche.ouvrirNouveau({})
            }
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
                        // Modifier, supprimer ; sinon, pourquoi pas (carnet
                        // en lecture seule, boîte hors ligne).
                        RowLayout {
                            visible: modelData.modifiable
                            spacing: 6
                            Button {
                                text: qsTr("Modifier…")
                                enabled: !boite.contactsOccupe
                                onClicked: dlgFiche.ouvrirFiche(modelData.id)
                            }
                            Button {
                                text: qsTr("Supprimer…")
                                enabled: !boite.contactsOccupe
                                onClicked: dlgCarnet.confirmerSuppression(modelData)
                            }
                        }
                        Label {
                            visible: !modelData.modifiable
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            opacity: 0.6
                            font.pointSize: dlgCarnet.font.pointSize * 0.9
                            text: qsTr("Non modifiable ici : %1.").arg(modelData.motif)
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
        RowLayout {
            Layout.fillWidth: true
            Button {
                text: qsTr("Fiches vides et doublons…")
                enabled: !boite.contactsOccupe
                onClicked: dlgNettoyage.ouvrir()
            }
            Item { Layout.fillWidth: true }
            Label {
                visible: boite.contactsOccupe
                text: qsTr("Écriture en cours…")
                opacity: 0.7
            }
        }
    }
}
