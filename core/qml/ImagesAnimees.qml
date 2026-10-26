// SPDX-License-Identifier: GPL-3.0-or-later
// GIF animés d'une zone de texte riche (0.6.18) : le texte riche de Qt n'en
// dessine que la première image — le badge animé de la signature restait
// fixe. Une AnimatedImage se pose sur chacun, à la place que le texte lui
// réserve ; la MiseEnForme de la zone les trouve et donne leur taille.
//
// Se place dans le TextEdit (ses coordonnées sont celles de la zone, et il
// défile avec elle) : « ImagesAnimees { zone: vueCorps; miseEnForme: mefLecture } ».
// Sous chaque animation, un aplat de la couleur du fond cache l'image fixe :
// sans lui, elle transparaîtrait là où le GIF s'efface (fentes du badge).
import QtQuick

Item {
    id: racine
    required property Item zone
    required property var miseEnForme
    property color fond: "#ffffff"
    // 40 animations au plus : une lettre d'information en porte parfois des
    // dizaines.
    readonly property int plafond: 40
    readonly property alias nombre: modele.count

    /// Repose les animations : position, taille et source de chaque GIF.
    function recalculer() {
        var l = racine.miseEnForme ? racine.miseEnForme.imagesAnimees() : []
        if (l.length > plafond)
            l = l.slice(0, plafond)
        if (l.length === 0 && modele.count === 0)
            return
        var memes = l.length === modele.count
        for (var i = 0; memes && i < l.length; ++i)
            memes = modele.get(i).source === l[i].source
        // Mêmes images : seules leurs places changent, l'animation continue
        // sans repartir de la première image à chaque frappe.
        if (!memes)
            modele.clear()
        for (var k = 0; k < l.length; ++k) {
            var r = racine.zone.positionToRectangle(l[k].position)
            var e = { source: l[k].source, gauche: r.x, haut: r.y + l[k].haut,
                      largeur: l[k].largeur, hauteur: l[k].hauteur }
            if (memes)
                modele.set(k, e)
            else
                modele.append(e)
        }
    }
    function bientot() { Qt.callLater(racine.recalculer) }

    /// Animations posées (scénarios d'essai) : place, taille, images.
    function etat() {
        var r = []
        for (var i = 0; i < repeteur.count; ++i) {
            var d = repeteur.itemAt(i)
            var a = d.children[0]
            r.push({ x: d.x, y: d.y, largeur: d.width, hauteur: d.height, source: d.source,
                     images: a.frameCount, image: a.currentFrame, joue: a.playing })
        }
        return r
    }

    Connections {
        target: racine.zone
        function onTextChanged() { racine.bientot() }
        function onWidthChanged() { racine.bientot() }
        function onContentHeightChanged() { racine.bientot() }
        function onContentWidthChanged() { racine.bientot() }
    }
    Component.onCompleted: bientot()

    ListModel { id: modele }
    Repeater {
        id: repeteur
        model: modele
        delegate: Rectangle {
            required property string source
            required property real gauche
            required property real haut
            required property real largeur
            required property real hauteur
            x: gauche
            y: haut
            width: largeur
            height: hauteur
            color: racine.fond
            AnimatedImage {
                anchors.fill: parent
                source: parent.source
                smooth: true
                playing: racine.visible && racine.zone.visible
            }
        }
    }
}
