// SPDX-License-Identifier: GPL-3.0-or-later
// Sous-menu « Catégoriser » du menu d'un message (0.6.15, demande de Manu du
// 22/10, comme « Catégoriser » d'Outlook) : les catégories de la boîte des
// messages choisis, cochées quand ils la portent tous ; en créer une, gérer la
// liste, tout effacer.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuCategories
    title: qsTr("Catégoriser")
    // Cf. fenetre.etatCategories : `liste`, `porte` (motCle → « tous » ou
    // « certains »), `refusees`, `aucune`.
    property var etat: ({ liste: [], porte: {}, refusees: false, aucune: true })
    onAboutToShow: {
        etat = fenetre.etatCategories()
        width = fenetre.largeurMenu(menuCategories)
    }
    // Le serveur de ces messages ne garde pas de mots-clés (Microsoft 365) :
    // le motif, à la place des catégories.
    MenuItem {
        visible: menuCategories.etat.refusees
        height: visible ? implicitHeight : 0
        enabled: false
        text: qsTr("Ce serveur ne garde pas les catégories")
    }
    Repeater {
        model: menuCategories.etat.refusees ? [] : menuCategories.etat.liste
        EntreeMenu {
            required property var modelData
            readonly property string porte: menuCategories.etat.porte[modelData.motCle.toLowerCase()] || ""
            text: modelData.nom
            checkable: true
            checked: porte === "tous"
            // Portée par une partie des messages choisis seulement : en
            // italique, et un clic la pose sur tous.
            font.italic: porte === "certains"
            rightPadding: padding + carre.width + 12
            onTriggered: fenetre.basculerCategorie(modelData.motCle, porte !== "tous")
            PastilleCategorie {
                id: carre
                anchors.right: parent.right
                anchors.rightMargin: parent.padding
                anchors.verticalCenter: parent.verticalCenter
                taille: Math.round(parent.height * 0.42)
                nom: parent.modelData.nom
                couleur: parent.modelData.couleur
            }
        }
    }
    MenuSeparator {}
    EntreeMenu {
        text: qsTr("Nouvelle catégorie…")
        enabled: !menuCategories.etat.refusees
        onTriggered: fenetre.nouvelleCategorie(true)
    }
    EntreeMenu {
        text: qsTr("Gérer les catégories…")
        enabled: !menuCategories.etat.refusees
        onTriggered: fenetre.gererCategories()
    }
    EntreeMenu {
        text: qsTr("Effacer toutes les catégories")
        enabled: !menuCategories.etat.refusees && !menuCategories.etat.aucune
        onTriggered: fenetre.effacerCategories()
    }
}
