// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Nouvelle catégorie » / « Modifier la catégorie » (0.6.15) : un
// nom et une couleur, rangés dans le catalogue de la boîte — sur le serveur,
// pour tous ceux qui l'ouvrent.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueCategorie { id: dlgCategorie } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgCategorie
    property alias champNom: champNom
    property int compte: 0
    property string chemin: ""
    // Vide : une catégorie à créer.
    property string motCle: ""
    property string couleur: fenetre.paletteCategories[0]
    // Vrai : la catégorie créée se pose aussitôt sur les messages choisis.
    property bool appliquer: false
    property string motif: ""

    function ouvrir(c, ch, mot, nom, teinte, poser) {
        compte = c
        chemin = ch
        motCle = mot
        couleur = teinte
        appliquer = poser
        motif = ""
        champNom.text = nom
        open()
        champNom.forceActiveFocus()
        champNom.selectAll()
    }

    function valider() {
        if (champNom.text.trim().length === 0) {
            motif = qsTr("Donnez un nom à la catégorie.")
            return
        }
        var pose = boite.poserCategorie(compte, chemin, motCle, champNom.text, couleur)
        if (pose.length === 0) {
            motif = boite.erreur.length > 0 ? boite.erreur : qsTr("Catégorie refusée.")
            return
        }
        close()
        if (appliquer)
            fenetre.basculerCategorie(pose, true)
        messageEtat.texte = motCle.length === 0
                ? qsTr("Catégorie « %1 » créée.").arg(champNom.text.trim())
                : qsTr("Catégorie « %1 » modifiée.").arg(champNom.text.trim())
    }

    title: motCle.length === 0 ? qsTr("Nouvelle catégorie") : qsTr("Modifier la catégorie")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(440, fenetre.width - 24)
    closePolicy: Popup.CloseOnEscape
    // « Enregistrer » ne ferme pas d'office : un refus laisse corriger.
    footer: DialogButtonBox {
        Button {
            text: qsTr("Enregistrer")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgCategorie.valider()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    ColumnLayout {
        width: dlgCategorie.availableWidth
        spacing: 8
        Label { text: qsTr("Nom :") }
        TextField {
            id: champNom
            MenuSaisie {}
            Layout.fillWidth: true
            maximumLength: 40
            placeholderText: qsTr("Par exemple : Devis à faire")
            onTextEdited: dlgCategorie.motif = ""
            Keys.onReturnPressed: dlgCategorie.valider()
            Keys.onEnterPressed: dlgCategorie.valider()
        }
        Label { text: qsTr("Couleur :") }
        // La palette, et la couleur actuelle si elle n'en est pas (celles
        // d'origine du webmail).
        Flow {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: fenetre.paletteCategories.indexOf(dlgCategorie.couleur.toUpperCase()) >= 0
                       ? fenetre.paletteCategories : [dlgCategorie.couleur.toUpperCase()].concat(fenetre.paletteCategories)
                Rectangle {
                    required property string modelData
                    readonly property bool choisie: modelData === dlgCategorie.couleur.toUpperCase()
                    width: 28
                    height: 28
                    radius: 4
                    color: modelData
                    border.width: choisie ? 3 : 1
                    border.color: choisie ? fenetre.palette.windowText : Qt.darker(modelData, 1.35)
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData
                    Accessible.checkable: true
                    Accessible.checked: choisie
                    Accessible.onPressAction: dlgCategorie.couleur = modelData
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: dlgCategorie.couleur = parent.modelData
                    }
                }
            }
        }
        RowLayout {
            spacing: 8
            Label { text: qsTr("Aperçu :") }
            PastilleCategorie {
                avecNom: true
                nom: champNom.text.trim().length > 0 ? champNom.text.trim() : qsTr("Catégorie")
                couleur: dlgCategorie.couleur
                taillePolice: fenetre.font.pointSize
            }
        }
        Label {
            Layout.fillWidth: true
            visible: dlgCategorie.motif.length > 0
            wrapMode: Text.Wrap
            color: "#b00020"
            text: dlgCategorie.motif
        }
    }
}
