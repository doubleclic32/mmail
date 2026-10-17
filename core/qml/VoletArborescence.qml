// SPDX-License-Identifier: GPL-3.0-or-later
// Volet de l'arborescence : rubrique Favoris, puis les comptes et leurs dossiers.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages, modeleMessages…) sont ceux de
// Main.qml, où « VoletArborescence { id: colonneArborescence } » l'instancie dans les volets.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// --- arborescence -------------------------------------------------
// Deux listes : la rubrique Favoris, fixée en haut — elle reste à
// l'écran quand les comptes défilent (demande de Manu du 01/10) —,
// puis les comptes, seuls à défiler.
ColumnLayout {
    id: colonneArborescence
    // Désignés hors de ce volet (Main.qml, fichiers voisins) : colonneArborescence.<nom>.
    property alias vueFavoris: vueFavoris
    property alias vueArborescence: vueArborescence
    visible: !fenetre.compact || fenetre.vue === 0
    SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurArborescence
    SplitView.minimumWidth: 160
    SplitView.fillWidth: fenetre.compact
    spacing: 0
    onWidthChanged: if (!fenetre.compact && visible) reglages.largeurArborescence = width

    // Les Favoris ne défilent qu'au-delà de 40 % de la hauteur.
    ScrollView {
        id: voletFavoris
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(vueFavoris.contentHeight, colonneArborescence.height * 0.4)
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
        clip: true
        contentWidth: availableWidth
        font.pointSize: fenetre.tailleArborescence
        background: Rectangle { color: fenetre.fondFavoris }

        ListView {
            id: vueFavoris
            model: modeleFavoris
            boundsBehavior: Flickable.StopAtBounds
            delegate: ligneArborescence

            HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "arborescence" }
            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: function(roue) {
                    fenetre.zoomer("arborescence", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                }
            }
        }
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 1
        color: fenetre.palette.windowText
        opacity: 0.25
    }

    ScrollView {
        id: voletArborescence
        Layout.fillWidth: true
        Layout.fillHeight: true
        // Ascenseurs toujours visibles dès qu'il y a de quoi défiler, et non
        // seulement pendant le défilement (demande Manu du 2026-09-29). Leur
        // place est réservée : posés par-dessus, ils masquaient les compteurs
        // de non-lus. La réserve suit `size`, pas `visible` : un ascenseur
        // qui n'a rien à faire défiler reste `visible`, seulement transparent.
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
        clip: true
        contentWidth: availableWidth
        font.pointSize: fenetre.tailleArborescence
        background: Rectangle { color: fenetre.palette.base }

        // Les gestionnaires vivent dans la liste, pas dans le ScrollView :
        // celui-ci ne reconnaît sa liste comme contenu défilant que si elle
        // est son seul enfant — sinon elle prend une largeur nulle.
        ListView {
            id: vueArborescence
            model: modeleArborescence
            boundsBehavior: Flickable.StopAtBounds
            delegate: ligneArborescence
            // Toutes les lignes restent construites : un dossier qu'on glisse
            // vers les Favoris fait défiler la liste, et sa ligne d'origine,
            // qui porte le glisser, ne doit pas être détruite en sortant de
            // l'écran — le glisser serait annulé.
            // Seulement pendant un glisser : hors de là, garder toutes
            // les lignes construites coûtait mémoire et reconstructions
            // pour rien.
            cacheBuffer: fenetre.glisse !== null ? 100000 : 400

            // Défilement au bord pendant un glisser-déposer (retour de Manu,
            // 01/10 : les Favoris, en haut, étaient hors d'atteinte).
            Timer {
                interval: 30
                repeat: true
                running: fenetre.glisse !== null
                onTriggered: {
                    var g = fenetre.glisse
                    if (!g)
                        return
                    var p = g.mapToItem(vueArborescence, g.width / 2, g.height / 2)
                    if (p.x < 0 || p.x > vueArborescence.width)
                        return
                    var bord = 40
                    var pas = 0
                    // Plus vite à mesure qu'on s'approche du bord, ou qu'on le passe.
                    if (p.y < bord && p.y > -2 * bord)
                        pas = -Math.max(4, (bord - p.y) / 2)
                    else if (p.y > vueArborescence.height - bord && p.y < vueArborescence.height + 2 * bord)
                        pas = Math.max(4, (p.y - vueArborescence.height + bord) / 2)
                    if (pas === 0)
                        return
                    var haut = vueArborescence.originY
                    var bas = Math.max(haut, haut + vueArborescence.contentHeight - vueArborescence.height)
                    var y = Math.min(bas, Math.max(haut, vueArborescence.contentY + pas))
                    if (y === vueArborescence.contentY)
                        return
                    vueArborescence.contentY = y
                    // Le contenu a bougé sous un pointeur immobile : la cible
                    // du dépôt est réévaluée.
                    g.y += 0.5
                    g.y -= 0.5
                }
            }

            // Un blanc et un trait au-dessus de chaque compte.
            section.property: "groupe"
            section.delegate: Item {
                required property string section
                width: vueArborescence.width
                height: 16
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: 6
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    height: 2
                    color: fenetre.palette.windowText
                    opacity: 0.3
                }
            }

            HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "arborescence" }

            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: function(roue) {
                    fenetre.zoomer("arborescence", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                }
            }
            PinchHandler {
                target: null
                property real depart: 1
                onActiveChanged: if (active) depart = reglages.zoomArborescence
                onActiveScaleChanged: if (active) fenetre.poserZoom("arborescence", depart * activeScale)
            }
        }
    }
}
