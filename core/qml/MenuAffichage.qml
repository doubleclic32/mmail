// SPDX-License-Identifier: GPL-3.0-or-later
// Menu « Affichage » de la barre de menus.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuAffichage { id: menuAffichage } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuAffichage
    title: qsTr("Affichage")
    onAboutToShow: width = fenetre.largeurMenu(menuAffichage)
    Repeater {
        model: fenetre.nomsApparence
        MenuItem {
            required property string modelData
            text: fenetre.jeuxApparence[modelData].libelle
            checkable: true
            checked: fenetre.apparence === modelData
            onTriggered: fenetre.choisirApparence(modelData)
        }
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Logiciels d'ouverture des pièces jointes…")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        onTriggered: dlgLogiciels.open()
    }
    MenuSeparator {}
    // Survol et raccourcis : rien de cela au doigt.
    MenuItem {
        visible: !fenetre.tactile
        height: visible ? implicitHeight : 0
        text: qsTr("Agrandir la colonne survolée (Ctrl +)")
        onTriggered: fenetre.zoomer(fenetre.colonneActive, 1.1)
    }
    MenuItem {
        visible: !fenetre.tactile
        height: visible ? implicitHeight : 0
        text: qsTr("Réduire la colonne survolée (Ctrl −)")
        onTriggered: fenetre.zoomer(fenetre.colonneActive, 1 / 1.1)
    }
    MenuItem {
        text: qsTr("Taille normale partout")
        onTriggered: {
            fenetre.poserZoom("arborescence", 1)
            fenetre.poserZoom("liste", 1)
            fenetre.poserZoom("message", 1)
        }
    }
}
