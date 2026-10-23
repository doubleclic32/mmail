// SPDX-License-Identifier: GPL-3.0-or-later
// Menu du clic droit d'un champ de saisie : Annuler, Couper, Copier, Coller,
// Tout sélectionner, comme dans les champs de Windows (0.6.14, demande de
// Manu du 22/10 : coller à la souris dans la recherche). Qt 6.8 n'en donne
// aucun d'office.
//
// Se déclare dans le champ lui-même — « MenuSaisie {} » — : le champ est
// l'élément qui le porte, TextField ou TextArea. Bureau seulement : au doigt,
// la barre d'Android s'en charge.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

TapHandler {
    id: clicDroit
    readonly property Item champ: parent
    // Collage propre au champ (le corps d'une rédaction colle aussi les
    // images) ; nul : celui du champ, permis s'il y a du texte à coller.
    property var coller: null
    // Mot de passe : ni couper ni copier, comme le champ lui-même.
    readonly property bool masque: champ.echoMode !== undefined && champ.echoMode !== TextInput.Normal

    // Clic droit de la souris seulement (cf. VoletLecture) : un TapHandler ne
    // trie les boutons que pour la souris.
    acceptedButtons: Qt.RightButton
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
    enabled: !fenetre.tactile

    // Le choix du texte au moment du clic, rendu au champ avant d'agir : le
    // menu prend le focus, et le champ peut perdre sa sélection.
    property int debut: 0
    property int fin: 0
    property bool agi: false

    onTapped: {
        debut = champ.selectionStart
        fin = champ.selectionEnd
        agi = false
        menu.popup()
    }

    function agir(action) {
        agi = true
        champ.forceActiveFocus()
        champ.select(debut, fin)
        action()
    }

    property Menu menu: Menu {
        id: menuChamp
        parent: clicDroit.champ
        onAboutToShow: width = fenetre.largeurMenu(menuChamp)
        // Le focus rendu au champ, pour continuer à la frappe (Entrée lance
        // la recherche collée).
        onClosed: if (clicDroit.agi) clicDroit.champ.forceActiveFocus()
        EntreeMenu {
            text: qsTr("Annuler")
            raccourci: qsTr("Ctrl+Z")
            enabled: !clicDroit.champ.readOnly && clicDroit.champ.canUndo
            onTriggered: clicDroit.agir(function() { clicDroit.champ.undo() })
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Couper")
            raccourci: qsTr("Ctrl+X")
            enabled: !clicDroit.champ.readOnly && !clicDroit.masque && clicDroit.fin > clicDroit.debut
            onTriggered: clicDroit.agir(function() { clicDroit.champ.cut() })
        }
        EntreeMenu {
            text: qsTr("Copier")
            raccourci: qsTr("Ctrl+C")
            enabled: !clicDroit.masque && clicDroit.fin > clicDroit.debut
            onTriggered: clicDroit.agir(function() { clicDroit.champ.copy() })
        }
        EntreeMenu {
            text: qsTr("Coller")
            raccourci: qsTr("Ctrl+V")
            enabled: !clicDroit.champ.readOnly && (clicDroit.coller !== null || clicDroit.champ.canPaste)
            onTriggered: clicDroit.agir(function() {
                if (clicDroit.coller !== null)
                    clicDroit.coller()
                else
                    clicDroit.champ.paste()
            })
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Tout sélectionner")
            raccourci: qsTr("Ctrl+A")
            enabled: clicDroit.champ.length > 0
            onTriggered: clicDroit.agir(function() { clicDroit.champ.selectAll() })
        }
    }
}
