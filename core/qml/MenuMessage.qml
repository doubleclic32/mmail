// SPDX-License-Identifier: GPL-3.0-or-later
// Menu contextuel d’un message de la liste.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuMessage { id: menuMessage } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuMessage
    font.pointSize: fenetre.tailleColonne("liste")
    onAboutToShow: width = fenetre.largeurMenu(menuMessage)
    MenuItem {
        visible: fenetre.dossierDeReprise
        height: visible ? implicitHeight : 0
        text: qsTr("Reprendre")
        onTriggered: fenetre.rediger("brouillon")
    }
    MenuItem {
        visible: !fenetre.dossierDeReprise
        height: visible ? implicitHeight : 0
        text: qsTr("Répondre")
        onTriggered: fenetre.rediger("repondre")
    }
    MenuItem {
        visible: !fenetre.dossierDeReprise
        height: visible ? implicitHeight : 0
        text: qsTr("Répondre à tous")
        onTriggered: fenetre.rediger("repondre_tous")
    }
    MenuItem {
        text: qsTr("Transférer")
        onTriggered: fenetre.rediger("transferer")
    }
    MenuItem {
        text: qsTr("Créer un filtre…")
        onTriggered: fenetre.creerFiltre()
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Poser un drapeau de suivi")
        onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), true)
    }
    MenuItem {
        text: qsTr("Retirer le drapeau")
        onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), false)
    }
    MenuSeparator {}
    MenuItem { text: qsTr("Déplacer vers…"); onTriggered: fenetre.ouvrirDeplacer() }
    MenuItem {
        visible: !fenetre.dossierDeReprise
        height: visible ? implicitHeight : 0
        text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Courrier indésirable")
        onTriggered: fenetre.signalerIndesirable()
    }
    MenuSeparator {}
    MenuItem { text: qsTr("Marquer comme lu"); onTriggered: fenetre.marquerSelection(true) }
    MenuItem { text: qsTr("Marquer comme non lu"); onTriggered: fenetre.marquerSelection(false) }
    MenuSeparator {}
    MenuItem { text: qsTr("Supprimer"); onTriggered: fenetre.supprimerSelection() }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Copier l'adresse de l'expéditeur")
        enabled: Object.keys(fenetre.selection).length === 1
        onTriggered: fenetre.copierChamp("adresse")
    }
    MenuItem {
        text: qsTr("Copier l'objet")
        enabled: Object.keys(fenetre.selection).length === 1
        onTriggered: fenetre.copierChamp("sujet")
    }
    MenuSeparator {}
    MenuItem {
        text: fenetre.tactile ? qsTr("Imprimer…") : qsTr("Imprimer… (Ctrl+P)")
        visible: impression.disponible
        height: visible ? implicitHeight : 0
        enabled: fenetre.uidCourant > 0
        onTriggered: fenetre.imprimerMessage()
    }
    MenuItem {
        text: qsTr("Enregistrer en PDF…")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        enabled: fenetre.uidCourant > 0
        onTriggered: fenetre.enregistrerPdf()
    }
    MenuItem {
        text: qsTr("Afficher la source")
        enabled: Object.keys(fenetre.selection).length === 1
        onTriggered: {
            fenetre.sourceVisible = false
            fenetre.basculerSource()
        }
    }
}
