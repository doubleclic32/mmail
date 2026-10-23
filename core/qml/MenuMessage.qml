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
    // 0.6.14 (demande de Manu du 22/10, le menu devenait « un foutoir
    // illisible ») : dix lignes au plus — lu ou non lu et le drapeau en une
    // ligne chacun, qui fait ce qui change quelque chose ; la sélection et
    // les actions rares en sous-menus ; les raccourcis à droite.
    property bool unNonLu: false
    property bool unSansDrapeau: false
    onAboutToShow: {
        var etat = fenetre.etatSelection()
        unNonLu = etat.nonLu
        unSansDrapeau = etat.sansDrapeau
        width = fenetre.largeurMenu(menuMessage)
    }
    EntreeMenu {
        visible: fenetre.dossierDeReprise
        text: qsTr("Reprendre")
        raccourci: qsTr("Ctrl+R")
        onTriggered: fenetre.rediger("brouillon")
    }
    EntreeMenu {
        visible: !fenetre.dossierDeReprise
        text: qsTr("Répondre")
        raccourci: qsTr("Ctrl+R")
        onTriggered: fenetre.rediger("repondre")
    }
    EntreeMenu {
        visible: !fenetre.dossierDeReprise
        text: qsTr("Répondre à tous")
        raccourci: qsTr("Ctrl+Maj+R")
        onTriggered: fenetre.rediger("repondre_tous")
    }
    EntreeMenu {
        text: qsTr("Transférer")
        raccourci: qsTr("Ctrl+F")
        onTriggered: fenetre.rediger("transferer")
    }
    MenuSeparator {}
    // Un message non lu parmi les choisis : « lu » ; sinon « non lu ».
    EntreeMenu {
        text: menuMessage.unNonLu ? qsTr("Marquer comme lu") : qsTr("Marquer comme non lu")
        raccourci: menuMessage.unNonLu ? qsTr("Ctrl+Q") : qsTr("Ctrl+U")
        onTriggered: fenetre.marquerSelection(menuMessage.unNonLu)
    }
    EntreeMenu {
        text: menuMessage.unSansDrapeau ? qsTr("Poser un drapeau de suivi") : qsTr("Retirer le drapeau")
        raccourci: qsTr("Inser")
        onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), menuMessage.unSansDrapeau)
    }
    MenuSeparator {}
    EntreeMenu {
        text: qsTr("Déplacer vers…")
        raccourci: qsTr("Ctrl+Maj+V")
        onTriggered: fenetre.ouvrirDeplacer()
    }
    EntreeMenu {
        visible: !fenetre.dossierDeReprise
        text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Courrier indésirable")
        raccourci: qsTr("Ctrl+Alt+J")
        onTriggered: fenetre.signalerIndesirable()
    }
    EntreeMenu {
        text: qsTr("Supprimer")
        raccourci: qsTr("Suppr")
        onTriggered: fenetre.supprimerSelection()
    }
    MenuSeparator {}
    // Dans « Toutes les BàL » et la recherche dans toutes les boîtes, un seul
    // message se choisit à la fois.
    Menu {
        id: menuSelection
        title: qsTr("Sélection")
        font: menuMessage.font
        onAboutToShow: width = fenetre.largeurMenu(menuSelection)
        EntreeMenu {
            text: qsTr("Tout sélectionner")
            raccourci: qsTr("Ctrl+A")
            enabled: !fenetre.listeMulti
            onTriggered: fenetre.toutChoisir()
        }
        EntreeMenu {
            text: qsTr("Inverser la sélection")
            enabled: !fenetre.listeMulti
            onTriggered: fenetre.inverserSelection()
        }
    }
    Menu {
        id: menuAutres
        title: qsTr("Autres actions")
        font: menuMessage.font
        onAboutToShow: width = fenetre.largeurMenu(menuAutres)
        EntreeMenu {
            text: qsTr("Créer un filtre…")
            onTriggered: fenetre.creerFiltre()
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("adresse")
        }
        EntreeMenu {
            text: qsTr("Copier l'objet")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("sujet")
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Imprimer…")
            raccourci: qsTr("Ctrl+P")
            visible: impression.disponible
            enabled: fenetre.uidCourant > 0
            onTriggered: fenetre.imprimerMessage()
        }
        EntreeMenu {
            text: qsTr("Enregistrer en PDF…")
            visible: Qt.platform.os !== "android"
            enabled: fenetre.uidCourant > 0
            onTriggered: fenetre.enregistrerPdf()
        }
        EntreeMenu {
            text: qsTr("Afficher la source")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: {
                fenetre.sourceVisible = false
                fenetre.basculerSource()
            }
        }
    }
}
