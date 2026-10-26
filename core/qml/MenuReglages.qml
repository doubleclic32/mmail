// SPDX-License-Identifier: GPL-3.0-or-later
// Menu « Réglages » de la barre de menus (0.6.20) : ce qui était dans
// « Comptes » sans être la gestion des comptes — nom et signature, filtres,
// carnet d'adresses, vidage des corbeilles, sauvegarde et restauration de la
// configuration. Demande de Manu du 26/10 : « Comptes » ne garde qu'ajouter,
// modifier, retirer.
//
// Les noms employés sans être déclarés ici (fenetre, boite, reglages…) sont
// ceux de Main.qml, où « MenuReglages { id: menuReglages } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuReglages
    title: qsTr("Réglages")
    onAboutToShow: width = fenetre.largeurMenu(menuReglages)
    MenuItem {
        text: qsTr("Nom et signature…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgIdentite.ouvrir(boite.compteCourant)
    }
    MenuItem {
        text: qsTr("Filtres du courrier…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgFiltres.ouvrir(null)
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Carnet d'adresses…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgCarnet.open()
    }
    MenuItem {
        text: qsTr("Nouveau contact…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgFiche.ouvrirNouveau({})
    }
    MenuItem {
        text: qsTr("Fiches vides et doublons…")
        enabled: fenetre.comptesConnus.length > 0 && !boite.contactsOccupe
        onTriggered: dlgNettoyage.ouvrir()
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Vider les corbeilles et indésirables…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgViderCorbeilles.open()
    }
    MenuSeparator {}
    MenuItem {
        text: reglagesSauvegarde.date > 0
              ? qsTr("Sauvegarder la configuration (dernière : %1)").arg(fenetre.dateLongue(reglagesSauvegarde.date))
              : qsTr("Sauvegarder la configuration")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: fenetre.sauvegarderConfiguration(true)
    }
    MenuItem {
        text: qsTr("Restaurer une configuration sauvegardée…")
        enabled: fenetre.comptesConnus.length > 0 && !fenetre.rechercheSauvegarde
        onTriggered: fenetre.chercherSauvegardes(true)
    }
}
