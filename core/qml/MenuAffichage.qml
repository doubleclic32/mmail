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
        text: qsTr("Notifier le nouveau courrier")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        checkable: true
        checked: reglages.notifications
        onTriggered: reglages.notifications = checked
    }
    // Comme « Volet de lecture » dans Outlook : un message affiché passe
    // « lu » aussitôt, après quelques secondes, ou seulement à la main.
    Menu {
        id: menuMarquageLu
        title: qsTr("Marquer comme lu")
        onAboutToShow: width = fenetre.largeurMenu(menuMarquageLu)
        Repeater {
            model: [
                { libelle: qsTr("À l'affichage"), delai: 0 },
                { libelle: qsTr("Après 3 secondes"), delai: 3 },
                { libelle: qsTr("Après 5 secondes"), delai: 5 },
                { libelle: qsTr("Après 10 secondes"), delai: 10 },
                { libelle: qsTr("Après 30 secondes"), delai: 30 },
                { libelle: qsTr("Jamais, seulement à la main"), delai: -1 }
            ]
            MenuItem {
                required property var modelData
                text: modelData.libelle
                checkable: true
                checked: reglages.delaiLu === modelData.delai
                onTriggered: reglages.delaiLu = modelData.delai
            }
        }
    }
    // Comme dans Outlook : la première lettre d'une phrase en capitale
    // pendant la frappe. Sous Android, c'est le clavier qui s'en charge.
    MenuItem {
        text: qsTr("Majuscule en début de phrase")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        checkable: true
        checked: reglages.majusculeAuto
        onTriggered: reglages.majusculeAuto = checked
    }
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
