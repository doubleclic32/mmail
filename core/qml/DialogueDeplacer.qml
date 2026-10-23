// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Déplacer vers… » (Ctrl+Maj+V).
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueDeplacer { id: dlgDeplacer } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgDeplacer
    // Fenêtre d'un message (0.6.16) qui ouvre ce dialogue pour son message :
    // elle a `compte`, `chemin` et `deplacerVers(compte, chemin)`. Sans elle,
    // la sélection de la fenêtre principale.
    property var pour: null
    title: pour ? qsTr("Déplacer le message vers…")
                : qsTr("Déplacer %1 vers…").arg(fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages")))
    modal: true
    anchors.centerIn: Overlay.overlay
    // Zoom de la colonne d'où il est ouvert (liste, message), fixé à
    // l'ouverture ; les dimensions suivent, dans la limite de la fenêtre.
    property real zoom: 1
    font.pointSize: fenetre.tailleBase * zoom
    width: Math.min(520 * zoom, (pour ? pour.width : fenetre.width) - 24)
    height: Math.min(560 * zoom, (pour ? pour.height : fenetre.height) - 40)
    standardButtons: Dialog.Ok | Dialog.Cancel
    property var cibles: []
    onOpened: {
        champFiltre.text = ""
        filtrer()
        champFiltre.forceActiveFocus()
    }
    onAccepted: {
        if (vueCibles.currentIndex >= 0 && vueCibles.currentIndex < modeleCibles.count) {
            var c = modeleCibles.get(vueCibles.currentIndex)
            if (pour)
                pour.deplacerVers(c.compte, c.chemin)
            else
                fenetre.deplacerVers(c.compte, c.chemin)
        }
    }
    function filtrer() {
        modeleCibles.clear()
        var filtre = champFiltre.text.toLowerCase()
        for (var i = 0; i < cibles.length; ++i) {
            var c = cibles[i]
            if (pour ? c.compte === pour.compte && c.chemin === pour.chemin
                     : c.compte === boite.compteCourant && c.chemin === boite.dossierCourant)
                continue
            var texte = (c.adresse + " " + c.chemin + " " + fenetre.libelleLigne(c)).toLowerCase()
            if (filtre.length === 0 || texte.indexOf(filtre) >= 0)
                modeleCibles.append(c)
        }
        vueCibles.currentIndex = modeleCibles.count > 0 ? 0 : -1
    }

    ColumnLayout {
        anchors.fill: parent
        TextField {
            id: champFiltre
            MenuSaisie {}
            Layout.fillWidth: true
            placeholderText: qsTr("Filtrer : nom du dossier ou de la boîte")
            onTextChanged: dlgDeplacer.filtrer()
            Keys.onDownPressed: vueCibles.incrementCurrentIndex()
            Keys.onUpPressed: vueCibles.decrementCurrentIndex()
            onAccepted: dlgDeplacer.accept()
        }
        ListView {
            id: vueCibles
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            ScrollBar.vertical: ScrollBar {
                policy: size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            }
            model: ListModel { id: modeleCibles }
            delegate: ItemDelegate {
                width: ListView.view.width
                       - (vueCibles.ScrollBar.vertical.size < 1 ? vueCibles.ScrollBar.vertical.width : 0)
                highlighted: ListView.isCurrentItem
                onClicked: vueCibles.currentIndex = index
                onDoubleClicked: dlgDeplacer.accept()
                contentItem: RowLayout {
                    Label {
                        text: model.adresse
                        opacity: 0.6
                        Layout.preferredWidth: 170 * dlgDeplacer.zoom
                        elide: Text.ElideRight
                    }
                    Item { Layout.preferredWidth: model.profondeur * 12 }
                    Label {
                        text: fenetre.libelleLigne(model)
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
