// SPDX-License-Identifier: GPL-3.0-or-later
// Pastille « RE », « RE TOUS » ou « TR » devant l’objet d’un message.
//
// Composant en ligne de Main.qml jusqu'à la 0.5.16 : sorti pour servir aussi
// aux fichiers découpés, un composant en ligne n'étant visible que du sien.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Pictogramme d'avertissement (triangle ambre « ! »), dessiné pour ne
// dépendre d'aucune police d'émojis. Réutilisé par les dialogues de vidage.
/// Pastille de la liste des messages : « RE », « RE TOUS », « TR ».
Rectangle {
    property alias texte: libelleMarque.text
    property string infobulle: ""
    implicitWidth: libelleMarque.implicitWidth + 10
    implicitHeight: libelleMarque.implicitHeight + 2
    radius: 3
    border.color: "white"
    Label {
        id: libelleMarque
        anchors.centerIn: parent
        font.bold: true
        font.pointSize: fenetre.tailleListe * 0.75
        color: "white"
    }
    HoverHandler { id: survolMarque }
    ToolTip.visible: survolMarque.hovered && infobulle.length > 0
    ToolTip.text: infobulle
    ToolTip.delay: 500
}
