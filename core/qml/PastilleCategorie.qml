// SPDX-License-Identifier: GPL-3.0-or-later
// Pastille d'une catégorie (0.6.15) : un carré de sa couleur en bout de ligne
// dans la liste, comme la colonne « Catégories » d'Outlook ; une étiquette à
// son nom dans l'en-tête du message, qu'un clic cherche.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Rectangle {
    id: pastille
    property string nom: ""
    property color couleur: "#8A8F98"
    // Vrai : l'étiquette à son nom ; faux : le carré seul.
    property bool avecNom: false
    property real taille: 10
    property real taillePolice: fenetre.tailleMessage * 0.85
    signal clique()
    // Texte sombre sur une couleur claire (jaune, pêche), blanc sinon.
    readonly property bool claire: couleur.r * 0.299 + couleur.g * 0.587 + couleur.b * 0.114 > 0.62
    implicitWidth: avecNom ? texte.implicitWidth + 16 : taille
    implicitHeight: avecNom ? texte.implicitHeight + 4 : taille
    radius: avecNom ? height / 2 : 2
    color: couleur
    border.width: 1
    border.color: Qt.darker(couleur, 1.35)

    Accessible.role: avecNom ? Accessible.Button : Accessible.StaticText
    Accessible.name: qsTr("Catégorie %1").arg(nom)
    Accessible.onPressAction: if (avecNom) pastille.clique()

    Label {
        id: texte
        visible: pastille.avecNom
        anchors.centerIn: parent
        text: pastille.nom
        font.pointSize: pastille.taillePolice
        color: pastille.claire ? "#1a1a1a" : "#ffffff"
    }
    MouseArea {
        id: survol
        anchors.fill: parent
        hoverEnabled: !fenetre.tactile
        // Dans la liste, le carré laisse passer le clic à la ligne.
        enabled: pastille.avecNom || !fenetre.tactile
        acceptedButtons: pastille.avecNom ? Qt.LeftButton : Qt.NoButton
        cursorShape: pastille.avecNom ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: pastille.clique()
    }
    ToolTip.visible: survol.containsMouse
    ToolTip.delay: 500
    ToolTip.text: pastille.avecNom ? qsTr("Chercher les messages de la catégorie « %1 »").arg(pastille.nom)
                                   : pastille.nom
}
