// SPDX-License-Identifier: GPL-3.0-or-later
// Pastille du nombre de non-lus d’un dossier ou d’un compte.
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

// Nombre de non-lus en pastille rouge, chiffre blanc.
Rectangle {
    id: cn
    // Le nombre est déjà dit par la ligne ou le bouton qui porte la pastille.
    Accessible.ignored: true
    property int nombre: 0
    property real taille: fenetre.tailleArborescence
    radius: height / 2
    color: fenetre.couleurAlerte
    implicitHeight: chiffreCompteur.implicitHeight + 2
    implicitWidth: Math.max(implicitHeight, chiffreCompteur.implicitWidth + 10)
    width: implicitWidth
    height: implicitHeight
    Label {
        id: chiffreCompteur
        anchors.centerIn: parent
        text: cn.nombre
        color: "#ffffff"
        font.bold: true
        font.pointSize: cn.taille * 0.88
    }
}
