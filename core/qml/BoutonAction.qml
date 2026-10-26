// SPDX-License-Identifier: GPL-3.0-or-later
// Bouton de la barre d’actions du message (Répondre, Transférer…).
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

// Bouton d'action du volet de lecture. Sous une apparence de marque, une
// pastille blanche cernée de bleu pétrole, pleine au survol ; ailleurs,
// le bouton du style.
Button {
    id: ba
    Rectangle {
        visible: fenetre.marque !== null
        z: -1
        anchors.fill: parent
        radius: height / 2
        color: ba.down ? Qt.darker(fenetre.couleurRubrique, 1.2)
             : ba.hovered ? fenetre.couleurRubrique : fenetre.palette.base
        border.width: 1
        border.color: fenetre.couleurRubrique
        Behavior on color {
            enabled: fenetre.anime
            ColorAnimation { duration: fenetre.dureeTeinte }
        }
        // Sur la pastille, jamais sur le bouton : un « scale » sur le contrôle
        // rétrécit sa zone cliquable, Qt testant le contact dans le repère
        // transformé.
        scale: ba.down && fenetre.anime ? 0.97 : 1
        Behavior on scale {
            enabled: fenetre.anime
            NumberAnimation { duration: fenetre.dureeAppui; easing.type: Easing.OutQuad }
        }
    }
    SiMarque { target: ba.background; property: "visible"; value: false }
    SiMarque {
        target: ba.contentItem
        property: "color"
        value: ba.hovered || ba.down ? "#ffffff" : fenetre.palette.buttonText
    }
    SiMarque { target: ba; property: "leftPadding"; value: 14 }
    SiMarque { target: ba; property: "rightPadding"; value: 14 }
}
