// SPDX-License-Identifier: GPL-3.0-or-later
// Ligne de la liste des messages.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où le Component « ligneMessage » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

SwipeDelegate {
    id: ligne
    width: ListView.view.width
    readonly property bool choisi: fenetre.listeMulti
                                   ? fenetre.cleChoisie === fenetre.cleResultat(model.compte, model.chemin, model.uid)
                                   : fenetre.selection[model.uid] === true
    highlighted: choisi

    // Lecteurs d'écran (Narrateur, Orca, TalkBack) : une entrée de liste qui
    // dit ce que la ligne montre — expéditeur, objet, date, puis les marques —,
    // et non un bouton muet.
    Accessible.role: Accessible.ListItem
    Accessible.name: [model.expediteur, model.sujet, fenetre.dateCourte(model.date),
                      model.lu ? "" : qsTr("non lu"),
                      model.importance > 0 ? qsTr("importance haute") : model.importance < 0 ? qsTr("importance basse") : "",
                      model.pieces === true ? qsTr("pièce jointe") : "",
                      model.reponduTous === true ? qsTr("répondu à tous") : model.repondu === true ? qsTr("répondu") : "",
                      model.transfere === true ? qsTr("transféré") : "",
                      model.suivi === true ? qsTr("suivi") : ""]
                     .filter(function(x) { return x }).join(", ")
    Accessible.selectable: true
    Accessible.selected: choisi
    padding: 0

    // Vue tactile : glisser la ligne (demande de Manu du 14/10) — vers la
    // gauche, la corbeille ; vers la droite, les indésirables (« Pas
    // indésirable » depuis ce dossier). Le fond qui se découvre a les deux
    // tiers de la largeur : lâchée passé le tiers, la ligne part. L'action
    // attend derrière le bandeau « Annuler » (cf. fenetre.glisserLigne).
    readonly property string roleLigne: fenetre.listeMulti ? (model.role || "") : fenetre.roleCourant
    swipe.enabled: fenetre.tactile
    swipe.left: roleLigne === "Trash" ? null : fondIndesirable
    swipe.right: roleLigne === "Trash" ? null : fondCorbeille
    swipe.onCompleted: fenetre.glisserLigne(index, swipe.position > 0 ? "indesirable" : "corbeille")
    // La liste recycle ses lignes (reuseItems) : une ligne glissée, rangée
    // puis reprise pour un autre message, gardait son décalage — d'autres
    // messages paraissaient glissés (retour de Manu du 18/10).
    ListView.onPooled: swipe.close()
    ListView.onReused: swipe.close()
    Component {
        id: fondIndesirable
        Rectangle {
            width: ligne.width * 2 / 3
            height: ligne.height
            color: "#d35400"
            Label {
                anchors.left: parent.left
                anchors.leftMargin: 18
                anchors.verticalCenter: parent.verticalCenter
                text: ligne.roleLigne === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
                color: "#ffffff"
                font.bold: true
            }
        }
    }
    Component {
        id: fondCorbeille
        Rectangle {
            width: ligne.width * 2 / 3
            height: ligne.height
            color: "#c0392b"
            RowLayout {
                anchors.right: parent.right
                anchors.rightMargin: 18
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Label {
                    text: qsTr("Corbeille")
                    color: "#ffffff"
                    font.bold: true
                }
                Canvas {
                    Layout.preferredHeight: 18
                    Layout.preferredWidth: 15
                    onPaint: fenetre.dessinerCorbeille(getContext("2d"), width, height, "#ffffff")
                }
            }
        }
    }
    // Au doigt (vue tactile), la ligne répond elle-même : la zone de la
    // souris lui laisserait sinon le geste.
    onClicked: if (fenetre.tactile) fenetre.choisir(index, 0)
    onPressAndHold: {
        if (!fenetre.tactile)
            return
        if (!ligne.choisi)
            fenetre.choisir(index, 0)
        menuMessage.popup(ligne, ligne.pressX, ligne.pressY)
    }
    onDoubleClicked: if (fenetre.tactile) fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")

    // Bandeau des non-lus, comme Outlook ; sous une apparence de
    // marque, le message choisi porte aussi un filet, pétrole s'il est lu.
    Rectangle {
        width: fenetre.marque ? 4 : 3
        height: parent.height
        visible: !model.lu || (fenetre.marque !== null && ligne.highlighted)
        color: fenetre.marque && model.lu ? fenetre.couleurRubrique : fenetre.accent
    }

    // Des lignes plus aérées, séparées d'un trait (retour de Fabienne,
    // 29/09 : les messages se distinguaient mal les uns des autres).
    topPadding: 7
    bottomPadding: 7
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: fenetre.palette.windowText
        opacity: 0.18
    }
    contentItem: ColumnLayout {
        spacing: 2
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            // Place de la corbeille, posée par-dessus en bout de ligne
            // (au survol de la souris seulement : rien au doigt).
            Layout.rightMargin: 8 + (fenetre.compact ? 0 : corbeilleLigne.width + 6)
            Label {
                text: model.expediteur
                color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                font.bold: !model.lu
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            // Trombone : le message porte des pièces jointes.
            Canvas {
                visible: model.pieces === true
                Layout.preferredHeight: dateLigne.implicitHeight
                Layout.preferredWidth: Math.round(dateLigne.implicitHeight * 0.62)
                property color encre: ligne.highlighted ? fenetre.palette.highlightedText
                                                        : fenetre.palette.windowText
                onEncreChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: fenetre.dessinerTrombone(getContext("2d"), width, height, encre)
            }
            Label {
                id: dateLigne
                text: fenetre.dateCourte(model.date)
                color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                opacity: 0.7
                font.pointSize: fenetre.tailleListe * 0.85
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            // Place du drapeau, posé par-dessus en bout de ligne.
            Layout.rightMargin: 8 + drapeauLigne.width
            spacing: 4
            // Importance annoncée par l'expéditeur : « ! » haute, « ↓ » basse.
            Label {
                visible: model.importance !== 0
                text: model.importance > 0 ? "!" : "↓"
                font.bold: true
                color: ligne.highlighted ? fenetre.palette.highlightedText
                     : model.importance > 0 ? fenetre.couleurAlerte : "#1a4480"
            }
            // Répondu, répondu à tous, transféré — par MMail ou par un
            // autre logiciel, le serveur porte les drapeaux : des
            // pastilles devant l'objet, aux préfixes d'Outlook (forme A,
            // choisie par Manu le 10/10 ; les flèches de la 0.5.4 et les
            // mots en bout de ligne de la 0.5.5 ne lui convenaient pas).
            MarqueSuite {
                visible: model.repondu === true
                texte: model.reponduTous === true ? qsTr("RE TOUS") : qsTr("RE")
                infobulle: model.reponduTous === true ? qsTr("Répondu à tous") : qsTr("Répondu")
                color: "#6b3fa0"
                border.width: ligne.highlighted ? 1 : 0
            }
            MarqueSuite {
                visible: model.transfere === true
                texte: qsTr("TR")
                infobulle: qsTr("Transféré")
                color: "#1a4480"
                border.width: ligne.highlighted ? 1 : 0
            }
            Label {
                text: model.sujet
                font.bold: !model.lu
                color: ligne.highlighted ? fenetre.palette.highlightedText
                     : model.lu ? fenetre.palette.windowText : fenetre.texteAccent
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
        }
        // Recherche dans toutes les boîtes : le dossier du message, sur
        // sa ligne, coupé au milieu au besoin — le compte et le dossier
        // restent lisibles (demande de Manu, 08/10).
        Label {
            visible: fenetre.listeMulti
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 8
            elide: Text.ElideMiddle
            font.pointSize: fenetre.tailleListe * 0.9
            color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.texteAccent
            text: model.cheminAffiche || ""
        }
    }

    // Ce qui part quand on glisse : l'étiquette qui suit le pointeur.
    Rectangle {
        id: etiquette
        property string uids: ""
        width: texteEtiquette.implicitWidth + 16
        height: texteEtiquette.implicitHeight + 8
        radius: 4
        color: fenetre.palette.highlight
        visible: zone.drag.active
        Drag.active: zone.drag.active
        Drag.keys: ["mmail/messages"]
        // Le point de dépôt est la pointe de la souris : l'étiquette
        // part de l'appui, coin haut gauche sous le pointeur (cf.
        // onPressed). Un point au milieu de son bord gauche, à une
        // demi-hauteur sous la pointe, déposait souvent dans le
        // dossier du dessous (retour d'une utilisatrice du 16/10).
        Drag.hotSpot.x: 0
        Drag.hotSpot.y: 0
        Drag.supportedActions: Qt.MoveAction
        Label {
            id: texteEtiquette
            anchors.centerIn: parent
            color: fenetre.palette.highlightedText
            text: fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages"))
        }
        states: State {
            when: zone.drag.active
            ParentChange { target: etiquette; parent: fenetre.contentItem }
        }
    }

    MouseArea {
        id: zone
        anchors.fill: parent
        enabled: !fenetre.tactile
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        // Au doigt — téléphone, ou écran tactile —, glisser fait défiler
        // la liste : pas de glisser-déposer (le tri passe par l'appui
        // long et le menu), et pas de choix à l'appui. Choisir à l'appui
        // ouvrait le message au premier contact, avant que la liste ait
        // reconnu le geste : elle ne défilait plus (APK, retour de Manu
        // du 05/10). Au doigt, un message s'ouvre au toucher bref.
        property bool auDoigt: fenetre.compact
        drag.target: auDoigt ? null : etiquette
        drag.threshold: 8
        // Vrai si l'appui a déjà choisi la ligne : le clic n'a alors
        // plus rien à faire.
        property bool choisieAuPress: false

        onPressed: function(souris) {
            auDoigt = fenetre.compact || souris.source !== Qt.MouseEventNotSynthesized
            choisieAuPress = !auDoigt && souris.button === Qt.LeftButton && !ligne.choisi
                    && !(souris.modifiers & (Qt.ControlModifier | Qt.ShiftModifier))
            // Glisser une ligne non choisie la choisit d'abord.
            if (choisieAuPress)
                fenetre.choisir(index, 0)
            etiquette.x = souris.x
            etiquette.y = souris.y
        }
        onClicked: function(souris) {
            if (souris.button === Qt.RightButton) {
                if (!ligne.choisi)
                    fenetre.choisir(index, 0)
                menuMessage.popup(zone, souris.x, souris.y)
                return
            }
            if (!choisieAuPress)
                fenetre.choisir(index, souris.modifiers)
        }
        // Le menu s'ouvre au point d'appui : sans position, Qt le place sous
        // le pointeur, que le doigt ne déplace pas (au milieu de l'écran sous
        // Android, retour de Manu du 12/10).
        onPressAndHold: function(souris) {
            if (!ligne.choisi)
                fenetre.choisir(index, 0)
            menuMessage.popup(zone, souris.x, souris.y)
        }
        onDoubleClicked: function(souris) {
            if (souris.button === Qt.LeftButton)
                fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")
        }
        drag.onActiveChanged: fenetre.glisse = drag.active ? etiquette : null
        onReleased: {
            if (drag.active)
                etiquette.Drag.drop()
        }
    }

    // Drapeau de suivi, en bout de seconde ligne, comme Outlook : plein
    // s'il est posé ; en creux au survol, pour montrer qu'un clic le pose.
    Canvas {
        id: drapeauLigne
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 7
        height: dateLigne.implicitHeight
        width: Math.round(height * 0.8)
        visible: model.suivi === true || ligne.hovered || survolDrapeau.containsMouse
        property color encre: model.suivi === true
                              ? (ligne.highlighted ? fenetre.palette.highlightedText : fenetre.couleurAlerte)
                              : (ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText)
        property bool plein: model.suivi === true
        opacity: plein ? 1 : 0.45
        onEncreChanged: requestPaint()
        onPleinChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: fenetre.dessinerDrapeau(getContext("2d"), width, height, encre, plein)
        // Dessiné, donc muet sans ceci : une case « Suivi ».
        Accessible.role: Accessible.CheckBox
        Accessible.name: qsTr("Suivi")
        Accessible.checkable: true
        Accessible.checked: model.suivi === true
        Accessible.onPressAction: fenetre.basculerSuivi([model.uid], model.suivi !== true)
        Accessible.onToggleAction: fenetre.basculerSuivi([model.uid], model.suivi !== true)
        MouseArea {
            id: survolDrapeau
            anchors.fill: parent
            anchors.margins: -4
            hoverEnabled: true
            onClicked: fenetre.basculerSuivi([model.uid], model.suivi !== true)
        }
    }

    // Corbeille en bout de première ligne, au survol (demande de Manu
    // du 15/10) : le message part à la corbeille sans être ouvert.
    // Rien dans la corbeille elle-même.
    Canvas {
        id: corbeilleLigne
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.top: parent.top
        anchors.topMargin: 7
        height: dateLigne.implicitHeight
        width: Math.round(height * 0.8)
        visible: !fenetre.compact
                 && (fenetre.listeMulti ? model.role : fenetre.roleCourant) !== "Trash"
                 && (ligne.hovered || survolCorbeille.containsMouse)
        property color encre: ligne.highlighted ? fenetre.palette.highlightedText
                                                : fenetre.palette.windowText
        opacity: survolCorbeille.containsMouse ? 1 : 0.55
        onEncreChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: fenetre.dessinerCorbeille(getContext("2d"), width, height, encre)
        ToolTip.visible: survolCorbeille.containsMouse
        ToolTip.delay: 600
        ToolTip.text: qsTr("Mettre à la corbeille")
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Mettre à la corbeille")
        Accessible.onPressAction: fenetre.supprimerLigne(index)
        MouseArea {
            id: survolCorbeille
            anchors.fill: parent
            anchors.margins: -4
            hoverEnabled: true
            onClicked: fenetre.supprimerLigne(index)
        }
    }
}
