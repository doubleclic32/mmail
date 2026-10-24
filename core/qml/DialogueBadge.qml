// SPDX-License-Identifier: GPL-3.0-or-later
// Fenêtre « Badge animé » (0.6.17), une option de la signature : compose un
// GIF qui fait défiler en boucle le logo de la société, la photo de la
// personne, puis d'autres logos, et l'insère dans la signature en cours
// d'édition comme toute autre image. Moteur : core/src/badge.rs, repris du
// générateur de signature de Guillaume LUDGER (Double Clic).
//
// L'aperçu est le badge lui-même, fabriqué par le même code que celui inséré.
// La composition (images, cadrages, forme, réglages) est gardée par adresse :
// le badge se reprend et se retouche plus tard.
//
// Ouverte depuis « Nom et signature » (DialogueIdentite.qml), qui l'instancie ;
// les noms employés sans être déclarés ici (fenetre, boite) sont ceux de
// Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Qt.labs.settings
import fr.mmedia.mmail

Dialog {
    id: dlgBadge
    title: qsTr("Badge animé")
    modal: true
    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: Math.min(880, fenetre.width - 24)
    height: Math.min(680, fenetre.height - 24)
    closePolicy: Popup.CloseOnEscape

    /// Le badge est inséré : adresse `file:` de sa copie dans le dossier des
    /// signatures du compte.
    signal insere(string url)

    property string adresse: ""
    // Images de la boucle, dans l'ordre : { url, nom, genre, zoom, dx, dy }.
    property var visuels: []
    property int choisi: -1
    property string forme: "rond"
    property var reglages: ({})
    // Aperçu affiché (GIF provisoire) et son bilan.
    property string apercu: ""
    property string bilan: ""
    property string note: ""
    // Le badge à insérer est demandé : un aperçu qui arrive ne compte plus.
    property bool insertion: false
    readonly property int visuelsMax: badge.visuelsMax()
    // Largeur du badge dans la signature : il est fabriqué plus grand, pour
    // rester net sur un écran à forte densité.
    readonly property int largeur: badge.largeurAffichee()
    readonly property var visuelChoisi: choisi >= 0 && choisi < visuels.length ? visuels[choisi] : null

    function ouvrir(a) {
        adresse = a
        var c = lireCompositions()[a] || {}
        visuels = c.visuels || []
        forme = c.forme === "carre" ? "carre" : "rond"
        reglages = c.reglages || JSON.parse(badge.reglagesParDefaut())
        choisi = visuels.length > 0 ? 0 : -1
        apercu = ""
        bilan = ""
        note = ""
        insertion = false
        open()
        recalculer()
    }
    function lireCompositions() {
        try {
            return JSON.parse(reglagesBadge.compositions)
        } catch (e) {
            return {}
        }
    }
    function composition() {
        return { visuels: visuels.map(function(v) {
                     return { url: v.url, genre: v.genre, zoom: v.zoom, dx: v.dx, dy: v.dy }
                 }), forme: forme, reglages: reglages }
    }
    /// Un nouvel aperçu, un instant après le dernier changement : un curseur
    /// que l'on fait glisser ne lance pas une fabrication par pas.
    function recalculer() {
        if (visuels.length > 0) {
            minuterieApercu.restart()
        } else {
            minuterieApercu.stop()
            apercu = ""
            bilan = ""
        }
    }
    function ajouter(url) {
        if (visuels.length >= visuelsMax) {
            note = qsTr("%1 images au plus dans un badge.").arg(visuelsMax)
            return
        }
        var texte = url.toString()
        var nom = decodeURIComponent(texte.substring(texte.lastIndexOf("/") + 1))
        // Une photo est le plus souvent un JPEG ; un logo, un PNG.
        var photo = /\.(jpe?g)$/i.test(nom)
        var l = visuels.slice()
        l.push({ url: texte, nom: nom, genre: photo ? "photo" : "logo", zoom: 1, dx: 0, dy: 0 })
        visuels = l
        choisi = l.length - 1
        note = ""
        recalculer()
    }
    function modifier(i, champ, valeur) {
        var l = visuels.slice()
        var v = Object.assign({}, l[i])
        v[champ] = valeur
        l[i] = v
        visuels = l
        recalculer()
    }
    function deplacer(i, sens) {
        var j = i + sens
        if (j < 0 || j >= visuels.length)
            return
        var l = visuels.slice()
        var v = l[i]
        l[i] = l[j]
        l[j] = v
        visuels = l
        choisi = j
        recalculer()
    }
    function retirer(i) {
        var l = visuels.slice()
        l.splice(i, 1)
        visuels = l
        choisi = Math.min(choisi, l.length - 1)
        recalculer()
    }
    function regler(champ, valeur) {
        if (reglages[champ] === valeur)
            return
        var r = Object.assign({}, reglages)
        r[champ] = valeur
        reglages = r
        recalculer()
    }
    function inserer() {
        if (visuels.length === 0 || insertion)
            return
        insertion = true
        note = ""
        badge.fabriquer(JSON.stringify(composition()), false)
    }

    Settings {
        id: reglagesBadge
        category: "badge"
        // Composition gardée par adresse (JSON).
        property string compositions: "{}"
    }
    BadgeAnime {
        id: badge
        onPret: function(url, apercu, images, octets) {
            if (!apercu) {
                if (!dlgBadge.insertion)
                    return
                dlgBadge.insertion = false
                var copie = boite.imageSignature(dlgBadge.adresse, url)
                if (copie.length === 0) {
                    dlgBadge.note = boite.erreur
                    return
                }
                var gardees = dlgBadge.lireCompositions()
                gardees[dlgBadge.adresse] = dlgBadge.composition()
                reglagesBadge.compositions = JSON.stringify(gardees)
                dlgBadge.insere(copie)
                dlgBadge.close()
                return
            }
            dlgBadge.apercu = url
            dlgBadge.bilan = qsTr("%1 images, %2 Ko").arg(images).arg(Math.round(octets / 1024))
            dlgBadge.note = ""
        }
        onEchec: function(apercu, message) {
            if (!apercu)
                dlgBadge.insertion = false
            dlgBadge.note = message
        }
    }
    Timer {
        id: minuterieApercu
        interval: 500
        onTriggered: badge.fabriquer(JSON.stringify(dlgBadge.composition()), true)
    }
    FileDialog {
        id: choixVisuel
        title: qsTr("Ajouter des images au badge")
        fileMode: FileDialog.OpenFiles
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.bmp *.webp)")]
        onAccepted: {
            for (var i = 0; i < selectedFiles.length; ++i)
                dlgBadge.ajouter(selectedFiles[i])
        }
    }

    footer: DialogButtonBox {
        Button {
            text: dlgBadge.insertion ? qsTr("Préparation…") : qsTr("Insérer dans la signature")
            enabled: dlgBadge.visuels.length > 0 && !dlgBadge.insertion
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgBadge.inserer()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 16

        // Composition : images, cadrage, forme, réglages.
        ScrollView {
            id: defilement
            Layout.fillWidth: true
            Layout.fillHeight: true
            // Largeur donnée par la rangée, jamais tirée du contenu : son
            // contenu suit la largeur disponible, la boucle se fermerait.
            Layout.preferredWidth: 1
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: defilement.availableWidth
                spacing: 6
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Une image ronde ou carrée qui fait défiler en boucle vos images — le logo de la société d'abord, puis votre photo, puis d'autres logos. Elle s'insère dans la signature comme toute autre image.")
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        text: qsTr("Images, dans l'ordre :")
                        font.bold: true
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Ajouter des images…")
                        enabled: dlgBadge.visuels.length < dlgBadge.visuelsMax
                        onClicked: choixVisuel.open()
                    }
                }
                Label {
                    visible: dlgBadge.visuels.length === 0
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: qsTr("Aucune image : ajoutez le logo de la société, votre photo, d'autres logos (PNG, JPEG…).")
                }
                Repeater {
                    model: dlgBadge.visuels
                    Rectangle {
                        id: ligneVisuel
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        implicitHeight: 48
                        radius: 4
                        color: index === dlgBadge.choisi ? fenetre.palette.highlight : "transparent"
                        border.color: fenetre.palette.mid
                        border.width: 1
                        MouseArea {
                            anchors.fill: parent
                            onClicked: dlgBadge.choisi = ligneVisuel.index
                        }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 6
                            anchors.rightMargin: 4
                            spacing: 6
                            Rectangle {
                                width: 40
                                height: 40
                                color: "#ffffff"
                                border.color: fenetre.palette.mid
                                Image {
                                    anchors.fill: parent
                                    anchors.margins: 2
                                    source: ligneVisuel.modelData.url
                                    sourceSize.width: 80
                                    sourceSize.height: 80
                                    fillMode: Image.PreserveAspectFit
                                    asynchronous: true
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: (ligneVisuel.index + 1) + ". " + ligneVisuel.modelData.nom
                                elide: Text.ElideMiddle
                                color: ligneVisuel.index === dlgBadge.choisi ? fenetre.palette.highlightedText : fenetre.palette.text
                            }
                            ComboBox {
                                model: [qsTr("Logo"), qsTr("Photo")]
                                currentIndex: ligneVisuel.modelData.genre === "photo" ? 1 : 0
                                onActivated: function(i) {
                                    dlgBadge.choisi = ligneVisuel.index
                                    dlgBadge.modifier(ligneVisuel.index, "genre", i === 1 ? "photo" : "logo")
                                }
                            }
                            ToolButton {
                                text: "↑"
                                enabled: ligneVisuel.index > 0
                                onClicked: dlgBadge.deplacer(ligneVisuel.index, -1)
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Plus tôt dans la boucle")
                            }
                            ToolButton {
                                text: "↓"
                                enabled: ligneVisuel.index < dlgBadge.visuels.length - 1
                                onClicked: dlgBadge.deplacer(ligneVisuel.index, 1)
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Plus tard dans la boucle")
                            }
                            ToolButton {
                                text: "✕"
                                onClicked: dlgBadge.retirer(ligneVisuel.index)
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Retirer du badge")
                            }
                        }
                    }
                }
                // Cadrage de l'image choisie : une photo couvre le badge, à
                // agrandir et décaler ; un logo s'y ajuste seul.
                Label {
                    visible: dlgBadge.visuelChoisi !== null
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    font.bold: true
                    wrapMode: Text.Wrap
                    text: dlgBadge.visuelChoisi && dlgBadge.visuelChoisi.genre === "photo"
                          ? qsTr("Cadrage de la photo")
                          : qsTr("Logo : marges rognées, ajusté seul à la forme du badge.")
                }
                GridLayout {
                    visible: dlgBadge.visuelChoisi !== null && dlgBadge.visuelChoisi.genre === "photo"
                    columns: 2
                    Layout.fillWidth: true
                    Label { text: qsTr("Agrandir") }
                    Slider {
                        Layout.fillWidth: true
                        from: 1
                        to: 3
                        stepSize: 0.05
                        value: dlgBadge.visuelChoisi ? dlgBadge.visuelChoisi.zoom : 1
                        onMoved: dlgBadge.modifier(dlgBadge.choisi, "zoom", value)
                    }
                    Label { text: qsTr("Gauche / droite") }
                    Slider {
                        Layout.fillWidth: true
                        from: -104
                        to: 104
                        stepSize: 2
                        value: dlgBadge.visuelChoisi ? dlgBadge.visuelChoisi.dx : 0
                        onMoved: dlgBadge.modifier(dlgBadge.choisi, "dx", value)
                    }
                    Label { text: qsTr("Haut / bas") }
                    Slider {
                        Layout.fillWidth: true
                        from: -104
                        to: 104
                        stepSize: 2
                        value: dlgBadge.visuelChoisi ? dlgBadge.visuelChoisi.dy : 0
                        onMoved: dlgBadge.modifier(dlgBadge.choisi, "dy", value)
                    }
                }
                RowLayout {
                    Layout.topMargin: 6
                    Label {
                        text: qsTr("Forme :")
                        font.bold: true
                    }
                    RadioButton {
                        text: qsTr("Ronde")
                        checked: dlgBadge.forme === "rond"
                        onToggled: if (checked) { dlgBadge.forme = "rond"; dlgBadge.recalculer() }
                    }
                    RadioButton {
                        text: qsTr("Carrée")
                        checked: dlgBadge.forme === "carre"
                        onToggled: if (checked) { dlgBadge.forme = "carre"; dlgBadge.recalculer() }
                    }
                }
                Label {
                    Layout.topMargin: 6
                    text: qsTr("Animation")
                    font.bold: true
                }
                GridLayout {
                    columns: 3
                    Layout.fillWidth: true
                    Repeater {
                        model: [
                            { champ: "lamelles", libelle: qsTr("Lamelles"), de: 4, a: 22, pas: 1, unite: "" },
                            { champ: "angle", libelle: qsTr("Angle"), de: 20, a: 80, pas: 1, unite: "°" },
                            { champ: "glissement", libelle: qsTr("Glissement"), de: 0, a: 120, pas: 2, unite: " px" },
                            { champ: "gris", libelle: qsTr("Voile gris"), de: 0, a: 1, pas: 0.05, unite: "" },
                            { champ: "pause", libelle: qsTr("Pause sur chaque image"), de: 300, a: 2500, pas: 50, unite: " ms" }
                        ]
                        delegate: Item {
                            id: reglage
                            required property var modelData
                            Layout.columnSpan: 3
                            Layout.fillWidth: true
                            implicitHeight: rangee.implicitHeight
                            RowLayout {
                                id: rangee
                                anchors.left: parent.left
                                anchors.right: parent.right
                                Label {
                                    text: reglage.modelData.libelle
                                    Layout.preferredWidth: 170
                                }
                                Slider {
                                    id: curseur
                                    Layout.fillWidth: true
                                    from: reglage.modelData.de
                                    to: reglage.modelData.a
                                    stepSize: reglage.modelData.pas
                                    snapMode: Slider.SnapAlways
                                    value: dlgBadge.reglages[reglage.modelData.champ] !== undefined
                                           ? dlgBadge.reglages[reglage.modelData.champ] : from
                                    onMoved: dlgBadge.regler(reglage.modelData.champ,
                                                             reglage.modelData.pas < 1 ? Math.round(value * 100) / 100 : Math.round(value))
                                }
                                Label {
                                    text: (reglage.modelData.pas < 1 ? curseur.value.toFixed(2) : Math.round(curseur.value)) + reglage.modelData.unite
                                    Layout.preferredWidth: 64
                                    horizontalAlignment: Text.AlignRight
                                }
                            }
                        }
                    }
                }
                Button {
                    text: qsTr("Réglages par défaut")
                    flat: true
                    onClicked: {
                        dlgBadge.reglages = JSON.parse(badge.reglagesParDefaut())
                        dlgBadge.recalculer()
                    }
                }
            }
        }

        // Aperçu, à la taille où il paraîtra dans la signature.
        ColumnLayout {
            Layout.preferredWidth: 230
            Layout.fillHeight: true
            spacing: 8
            Label {
                text: qsTr("Aperçu")
                font.bold: true
            }
            Rectangle {
                Layout.preferredWidth: 220
                Layout.preferredHeight: 200
                color: "#ffffff"
                border.color: fenetre.palette.mid
                AnimatedImage {
                    id: imageApercu
                    anchors.centerIn: parent
                    width: dlgBadge.largeur
                    height: width
                    source: dlgBadge.apercu
                    cache: false
                    playing: dlgBadge.visible
                }
                BusyIndicator {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    width: 32
                    height: 32
                    running: badge.occupe
                }
            }
            Label {
                Layout.fillWidth: true
                text: dlgBadge.bilan
                opacity: 0.75
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pointSize: Math.max(7, fenetre.font.pointSize - 1)
                text: qsTr("Les versions de bureau d'Outlook n'affichent que la première image : mettez le logo de la société en tête. Le badge alourdit chaque message de son poids.")
            }
            Label {
                visible: dlgBadge.note.length > 0
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#b00020"
                text: dlgBadge.note
            }
            Item { Layout.fillHeight: true }
        }
    }
}
