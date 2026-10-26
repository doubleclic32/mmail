// SPDX-License-Identifier: GPL-3.0-or-later
// Menu « Comptes » de la barre de menus : la gestion des comptes seule —
// ajouter, modifier, retirer. Le reste (signature, filtres, carnet,
// corbeilles, sauvegarde) est dans « Réglages » (MenuReglages.qml) depuis la
// 0.6.20, à la demande de Manu (26/10).
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuComptes { id: menuComptes } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// ---------------------------------------------------------------- menus
Menu {
    id: menuComptes
    title: qsTr("Comptes")
    onAboutToShow: width = fenetre.largeurMenu(menuComptes)
    MenuItem {
        text: qsTr("Ajouter un compte…")
        onTriggered: fenetre.demanderCompte(null, "")
    }
    MenuItem {
        text: qsTr("Ajouter un compte Microsoft (Office 365, Outlook.com)…")
        onTriggered: fenetre.ajouterMicrosoft("")
    }
    MenuItem {
        text: qsTr("Ajouter un compte Google (Gmail)…")
        visible: boite.googleAjoutOuvert()
        height: visible ? implicitHeight : 0
        onTriggered: fenetre.connecterGoogle("")
    }
    MenuItem {
        text: qsTr("Ajouter avec un lien de configuration…")
        onTriggered: dlgLien.ouvrir("", "")
    }
    // Serveurs, mot de passe, nom et signature d'un compte présent (aussi
    // au clic droit sur le compte) : à chercher ici en premier.
    Menu {
        id: menuModification
        title: qsTr("Modifier un compte")
        onAboutToShow: width = fenetre.largeurMenu(menuModification)
        Instantiator {
            model: fenetre.comptesConnus
            delegate: MenuItem {
                required property var modelData
                text: modelData.adresse + "…"
                onTriggered: fenetre.modifierCompte(modelData.compte)
            }
            onObjectAdded: function(indice, objet) { menuModification.insertItem(indice, objet) }
            onObjectRemoved: function(indice, objet) { menuModification.removeItem(objet) }
        }
        MenuItem {
            text: qsTr("Aucun compte")
            enabled: false
            visible: fenetre.comptesConnus.length === 0
            height: visible ? implicitHeight : 0
        }
    }
    MenuSeparator {}
    // Le retrait est aussi au clic droit sur le compte ; il est repris ici
    // pour qu'on le trouve sans le connaître.
    Menu {
        id: menuRetrait
        title: qsTr("Retirer un compte")
        onAboutToShow: width = fenetre.largeurMenu(menuRetrait)
        Instantiator {
            model: fenetre.comptesConnus
            delegate: MenuItem {
                required property var modelData
                text: modelData.adresse + "…"
                onTriggered: dlgRetrait.ouvrir(modelData.compte, modelData.adresse, modelData.hote)
            }
            onObjectAdded: function(indice, objet) { menuRetrait.insertItem(indice, objet) }
            onObjectRemoved: function(indice, objet) { menuRetrait.removeItem(objet) }
        }
        MenuItem {
            text: qsTr("Aucun compte")
            enabled: false
            visible: fenetre.comptesConnus.length === 0
            height: visible ? implicitHeight : 0
        }
    }
}
