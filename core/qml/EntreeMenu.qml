// SPDX-License-Identifier: GPL-3.0-or-later
// Entrée de menu et son raccourci clavier, aligné à droite en gris, comme
// dans les menus de Windows (0.6.14). Au doigt, pas de raccourci affiché.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

MenuItem {
    id: entree
    // « Ctrl+R », « Suppr »… : le libellé du raccourci, vide s'il n'y en a pas.
    property string raccourci: ""
    readonly property bool avecRaccourci: raccourci.length > 0 && !fenetre.tactile
    // Masquée, elle ne laisse pas de vide dans le menu.
    height: visible ? implicitHeight : 0
    // La place du raccourci, prise sur la marge de droite : le libellé du
    // style reste en place, et la largeur du menu (largeurMenu) en tient
    // compte.
    rightPadding: padding + (avecRaccourci ? etiquetteRaccourci.implicitWidth + 28 : 0)

    Label {
        id: etiquetteRaccourci
        visible: entree.avecRaccourci
        text: entree.raccourci
        font: entree.font
        anchors.right: parent.right
        anchors.rightMargin: entree.padding
        anchors.verticalCenter: parent.verticalCenter
        color: entree.highlighted ? entree.palette.highlightedText : entree.palette.text
        opacity: entree.highlighted ? 0.85 : 0.55
    }
}
