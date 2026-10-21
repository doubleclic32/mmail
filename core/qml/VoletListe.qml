// SPDX-License-Identifier: GPL-3.0-or-later
// Volet de la liste des messages : recherche, historique, tri, liste.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages, modeleMessages…) sont ceux de
// Main.qml, où « VoletListe { id: colonneListe } » l'instancie dans les volets.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// --- liste des messages -------------------------------------------
ColumnLayout {
    id: colonneListe
    // Désignés hors de ce volet (Main.qml, fichiers voisins) : colonneListe.<nom>.
    property alias champRecherche: champRecherche
    property alias panneauHistorique: panneauHistorique
    property alias porteeRecherche: porteeRecherche
    property alias vueMessages: vueMessages
    visible: !fenetre.compact || fenetre.vue === 1
    SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurListe
    SplitView.minimumWidth: 240
    SplitView.fillWidth: fenetre.compact
    spacing: 0
    onWidthChanged: if (!fenetre.compact && visible) reglages.largeurListe = width

    // Recherche, comme dans Outlook : dans le dossier ouvert (objet,
    // expéditeur, puis texte entier par le serveur), ou dans toutes les
    // boîtes (index local).
    // Vue tactile : la recherche se montre par la loupe de la barre
    // d'application, et se range une fois vide et quittée.
    property bool rechercheOuverte: false
    RowLayout {
        visible: !fenetre.tactile || colonneListe.rechercheOuverte || champRecherche.text.length > 0
        Layout.fillWidth: true
        Layout.margins: 4
        spacing: 4
        TextField {
            id: champRecherche
            Layout.fillWidth: true
            placeholderText: fenetre.tactile ? qsTr("Rechercher") : qsTr("Rechercher (Ctrl+E)")
            selectByMouse: true
            // Mis en valeur (demande de Manu, 08/10) : une loupe, un
            // cadre de la couleur d'accent sur un fond teinté ; le cadre
            // s'épaissit quand le champ a la main.
            leftPadding: loupe.width + 13
            background: Rectangle {
                implicitHeight: 30
                radius: fenetre.marque ? height / 2 : 4
                color: champRecherche.activeFocus ? fenetre.palette.base
                       : Qt.rgba(fenetre.accent.r, fenetre.accent.g, fenetre.accent.b, 0.08)
                border.width: champRecherche.activeFocus ? 2 : 1
                border.color: fenetre.accent
            }
            Canvas {
                id: loupe
                anchors.left: parent.left
                anchors.leftMargin: 7
                anchors.verticalCenter: parent.verticalCenter
                height: Math.round(parent.height * 0.5)
                width: height
                property color encre: fenetre.accent
                onEncreChanged: requestPaint()
                onWidthChanged: requestPaint()
                onPaint: fenetre.dessinerLoupe(getContext("2d"), width, height, encre)
            }
            // La frappe ne cherche plus : elle propose les dernières
            // recherches, que l'on choisit ; Entrée lance la recherche
            // (demande de Manu, 09/10).
            onActiveFocusChanged: {
                if (activeFocus)
                    fenetre.proposerHistorique()
                else if (text.length === 0)
                    colonneListe.rechercheOuverte = false
            }
            // Le mot effacé au clavier, la liste de départ revient, comme par
            // ✕ (0.6.12, demande de Fabienne : elle restait filtrée).
            onTextEdited: {
                if (text.length === 0 && fenetre.recherche.length > 0)
                    fenetre.quitterRecherche()
                else
                    fenetre.proposerHistorique()
            }
            onAccepted: {
                if (panneauHistorique.opened && vueHistorique.currentIndex >= 0)
                    fenetre.lancerRecherche(panneauHistorique.entrees[vueHistorique.currentIndex])
                else
                    fenetre.lancerRecherche()
            }
            Keys.onDownPressed: function(ev) {
                if (panneauHistorique.opened)
                    vueHistorique.incrementCurrentIndex()
                else
                    fenetre.proposerHistorique()
            }
            Keys.onUpPressed: if (panneauHistorique.opened) vueHistorique.decrementCurrentIndex()
            Keys.onEscapePressed: {
                if (panneauHistorique.opened)
                    panneauHistorique.close()
                else
                    fenetre.quitterRecherche()
            }
            // Dernières recherches sous le champ : dix visibles au plus,
            // un ascenseur au-delà.
            Popup {
                id: panneauHistorique
                property var entrees: []
                y: champRecherche.height + 2
                width: champRecherche.width
                padding: 1
                closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                onAboutToShow: vueHistorique.currentIndex = -1
                background: Rectangle {
                    color: fenetre.palette.base
                    border.color: fenetre.accent
                    radius: 3
                }
                contentItem: ListView {
                    id: vueHistorique
                    implicitHeight: Math.min(panneauHistorique.entrees.length, 10) * 30
                    clip: true
                    model: panneauHistorique.entrees
                    currentIndex: -1
                    ScrollBar.vertical: ScrollBar {
                        policy: panneauHistorique.entrees.length > 10 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                    }
                    delegate: ItemDelegate {
                        width: ListView.view.width
                        height: 30
                        text: modelData
                        highlighted: ListView.isCurrentItem
                        onClicked: fenetre.lancerRecherche(modelData)
                    }
                }
            }
        }
        ComboBox {
            id: porteeRecherche
            readonly property bool partout: currentIndex === 1
            model: [qsTr("Dossier actif"), qsTr("Toutes les boîtes")]
            implicitContentWidthPolicy: ComboBox.WidestText
            // Le champ s'étire, la portée garde sa largeur : « Toutes les
            // boîtes » était tronqué.
            Layout.minimumWidth: Math.max(implicitWidth, 150)
            onActivated: if (champRecherche.text.trim().length > 0) fenetre.lancerRecherche()
        }
        ToolButton {
            text: "✕"
            visible: champRecherche.text.length > 0 || fenetre.recherche.length > 0
            onClicked: fenetre.quitterRecherche()
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Effacer la recherche (Échap)")
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 8
        Layout.rightMargin: 4
        Layout.bottomMargin: 4
        spacing: 6
        visible: fenetre.recherche.length > 0
        Label { text: qsTr("Trier par") }
        ComboBox {
            id: choixTri
            model: fenetre.trisRecherche
            textRole: "libelle"
            implicitContentWidthPolicy: ComboBox.WidestText
            function suivre() {
                var i = fenetre.trisRecherche.findIndex(function(t) { return t.cle === fenetre.triRecherche })
                if (i < 0) {
                    // « Dossier » n'a plus cours hors de toutes les boîtes.
                    fenetre.triRecherche = "recents"
                    i = 0
                }
                currentIndex = i
            }
            onModelChanged: suivre()
            Component.onCompleted: suivre()
            onActivated: fenetre.trierRecherche(fenetre.trisRecherche[currentIndex].cle)
        }
        Item { Layout.fillWidth: true }
    }

    ScrollView {
        id: voletListe
        Layout.fillWidth: true
        Layout.fillHeight: true
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
        clip: true
        contentWidth: availableWidth
        font.pointSize: fenetre.tailleListe
        background: Rectangle { color: fenetre.palette.base }

        ListView {
            id: vueMessages
            model: modeleMessages
            // Les lignes qui sortent de l'écran servent à celles qui entrent,
            // au lieu d'être détruites et recréées à chaque défilement.
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            currentIndex: -1
            focus: true
            keyNavigationEnabled: false
            delegate: ligneMessage

            Keys.onUpPressed: function(touche) {
                fenetre.deplacerCurseur(-1, touche.modifiers & Qt.ShiftModifier)
            }
            Keys.onDownPressed: function(touche) {
                fenetre.deplacerCurseur(1, touche.modifiers & Qt.ShiftModifier)
            }

            HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "liste" }

            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: function(roue) {
                    fenetre.zoomer("liste", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                }
            }
            PinchHandler {
                target: null
                property real depart: 1
                onActiveChanged: if (active) depart = reglages.zoomListe
                onActiveScaleChanged: if (active) fenetre.poserZoom("liste", depart * activeScale)
            }

            Label {
                anchors.centerIn: parent
                visible: modeleMessages.count === 0
                opacity: 0.6
                text: boite.dossierCourant.length === 0
                      ? qsTr("Choisissez un dossier.")
                      : qsTr("Aucun message.")
            }
        }
    }

}
