// SPDX-License-Identifier: GPL-3.0-or-later
// Ligne de l’arborescence : rubrique Favoris, compte, dossier ou favori.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où le Component « ligneArborescence » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

ItemDelegate {
    id: ligne
    width: ListView.view.width
    readonly property bool estDossier: model.genre === "dossier" || model.genre === "favori"
    // Pas en vue multi-dossiers : le noyau y suit le message choisi,
    // ce n'est pas le dossier qu'on regarde.
    readonly property bool estCourant: estDossier && !fenetre.listeMulti
            && model.compte === boite.compteCourant
            && model.chemin === boite.dossierCourant
    // Vrai pendant qu'un glisser-déposer accepté survole la ligne.
    property bool survol: false
    highlighted: estCourant

    // Lecteurs d'écran : un élément d'arborescence, nommé comme à l'écran,
    // avec ses non-lus.
    Accessible.role: Accessible.TreeItem
    Accessible.name: fenetre.libelleLigne(model)
                     + (model.nonLus > 0 ? ", " + fenetre.accord(model.nonLus, qsTr("non lu"), qsTr("non lus")) : "")
    Accessible.selectable: estDossier
    Accessible.selected: estCourant
    hoverEnabled: true

    function cliquer() {
        if (model.genre === "compte")
            boite.replierCompte(model.compte, !model.replie)
        else if (estDossier && model.selectionnable)
            fenetre.ouvrirDossier(model.compte, model.chemin)
    }

    /// Vrai si le clic tombe sur le chevron d'un dossier parent (marge
    /// comprise, pour le doigt comme pour la souris). Mesuré depuis le
    /// centre du chevron : le chevron d'un dossier déplié est tourné
    /// d'un quart de tour autour de ce centre, et son coin supérieur
    /// gauche, lui, se déplace — le clic sur un dossier déplié tombait à
    /// côté de la zone (retour de Manu, 01/10).
    function surChevron(x) {
        if (!chevronDossier.visible || !model.enfants)
            return false
        var centre = chevronDossier.mapToItem(zoneLigne, chevronDossier.width / 2, chevronDossier.height / 2)
        return Math.abs(x - centre.x) <= chevronDossier.width / 2 + 6
    }

    // La rubrique Favoris sur un fond plus soutenu que celui des
    // comptes. Au-dessus du fond propre à la ligne (z -1, opaque dans
    // le style Fusion), sauf quand la ligne est choisie ou pressée.
    Rectangle {
        z: -0.5
        anchors.fill: parent
        visible: model.groupe === "favoris" && !ligne.highlighted && !ligne.down
        color: fenetre.fondFavoris
    }

    // Menu de la ligne, ouvert au point du clic ou de l'appui (x, y dans la
    // ligne) — cf. LigneMessage.
    function menuLigne(x, y) {
        if (model.genre === "compte")
            menuCompte.ouvrir(model.compte, model.adresse, model.hote, model.etat, ligne, x, y)
        else if (estDossier)
            menuDossier.ouvrir(model.compte, model.chemin, model.favori, model.masque, model.nom,
                               model.selectionnable, ligne, x, y)
    }

    // Cible de dépôt : des messages glissés depuis la liste (sur un
    // dossier), ou un dossier glissé depuis l'arborescence (sur la
    // rubrique Favoris ou l'un de ses favoris, décision 3).
    // Un compte glissé sur un autre compte prend sa place (moitié haute)
    // ou la suivante (moitié basse).
    DropArea {
        id: depot
        anchors.fill: parent
        keys: ["mmail/messages", "mmail/dossier", "mmail/compte"]
        onEntered: function(glisse) {
            var dossierGlisse = glisse.keys.indexOf("mmail/dossier") >= 0
            var compteGlisse = glisse.keys.indexOf("mmail/compte") >= 0
            var accepte = compteGlisse
                    ? (model.genre === "compte" && glisse.source && glisse.source.compte !== model.compte)
                    : dossierGlisse
                    ? (model.genre === "rubrique" || model.genre === "favori"
                       || model.genre === "favori-vide")
                    : (ligne.estDossier && model.selectionnable)
            glisse.accepted = accepte
            ligne.survol = accepte
        }
        onExited: ligne.survol = false
        onDropped: function(depose) {
            ligne.survol = false
            if (depose.keys.indexOf("mmail/compte") >= 0) {
                boite.placerCompte(depose.source.compte,
                                   depose.y < height / 2 ? model.compte : fenetre.compteSuivant(model.compte))
            } else if (depose.keys.indexOf("mmail/dossier") >= 0) {
                var source = depose.source
                boite.placerFavori(source.compte, source.chemin,
                                   model.genre === "favori" ? model.compte : 0,
                                   model.genre === "favori" ? model.chemin : "")
            } else {
                fenetre.deplacerVers(model.compte, model.chemin)
            }
            depose.accept(Qt.MoveAction)
        }
    }
    Rectangle {
        anchors.fill: parent
        visible: ligne.survol
        color: "transparent"
        border.color: fenetre.accent
        border.width: 2
        radius: 3
    }

    // Ce qui part quand on glisse un dossier : son nom, qui suit le
    // pointeur. Il porte de quoi désigner le dossier au dépôt.
    Rectangle {
        id: etiquetteDossier
        property int compte: model.compte
        property string chemin: model.chemin
        width: texteEtiquetteDossier.implicitWidth + 16
        height: texteEtiquetteDossier.implicitHeight + 8
        radius: 4
        color: fenetre.palette.highlight
        visible: zoneLigne.drag.active
        Drag.active: zoneLigne.drag.active
        Drag.keys: model.genre === "compte" ? ["mmail/compte"] : ["mmail/dossier"]
        // Le point de dépôt est la pointe de la souris : l'étiquette
        // part de l'appui, coin haut gauche sous le pointeur (cf.
        // onPressed). Un point au milieu de son bord gauche, à une
        // demi-hauteur sous la pointe, déposait souvent dans le
        // dossier du dessous (retour d'une utilisatrice du 16/10).
        Drag.hotSpot.x: 0
        Drag.hotSpot.y: 0
        Drag.supportedActions: Qt.MoveAction
        Label {
            id: texteEtiquetteDossier
            anchors.centerIn: parent
            color: fenetre.palette.highlightedText
            text: fenetre.libelleLigne(model)
        }
        states: State {
            when: zoneLigne.drag.active
            ParentChange { target: etiquetteDossier; parent: fenetre.contentItem }
        }
    }

    MouseArea {
        id: zoneLigne
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        // Un dossier (vers les Favoris) ou un en-tête de compte (pour
        // l'ordre des comptes) se glisse, et pas au doigt : glisser y
        // fait défiler l'arborescence.
        drag.target: (ligne.estDossier || model.genre === "compte") && !fenetre.compact
                     ? etiquetteDossier : null
        drag.threshold: 8
        onPressed: function(souris) {
            etiquetteDossier.x = souris.x
            etiquetteDossier.y = souris.y
        }
        onClicked: function(souris) {
            if (souris.button === Qt.RightButton)
                ligne.menuLigne(souris.x, souris.y)
            else if (ligne.surChevron(souris.x))
                boite.replierDossier(model.compte, model.chemin, !model.replie)
            else
                ligne.cliquer()
        }
        onPressAndHold: function(souris) { ligne.menuLigne(souris.x, souris.y) }
        drag.onActiveChanged: fenetre.glisse = drag.active ? etiquetteDossier : null
        onReleased: {
            if (drag.active)
                etiquetteDossier.Drag.drop()
        }
    }

    contentItem: RowLayout {
        spacing: 6
        Item {
            Layout.preferredWidth: model.genre === "dossier" ? model.profondeur * 14
                                 : model.genre === "favori" || model.genre === "favori-vide" ? 14 : 0
        }
        // Chevron d'un dossier qui a des sous-dossiers : un clic les
        // replie ou les déplie (retour de Manu, 01/10). La place est
        // gardée pour les autres, pour que les noms restent alignés.
        Label {
            id: chevronDossier
            visible: model.genre === "dossier"
            text: model.enfants ? "›" : ""
            color: fenetre.palette.windowText
            font.bold: true
            rotation: model.replie ? 0 : 90
            opacity: 0.7
            horizontalAlignment: Text.AlignHCenter
            Layout.preferredWidth: 14
        }
        // « › » plutôt qu'un triangle : présent dans toutes les polices.
        Label {
            visible: model.genre === "compte"
            text: "›"
            color: fenetre.palette.windowText
            font.bold: true
            rotation: model.replie ? 0 : 90
            opacity: 0.7
        }
        Label {
            readonly property bool entete: model.genre === "compte" || model.genre === "rubrique"
            text: fenetre.libelleLigne(model)
            color: ligne.highlighted ? fenetre.palette.highlightedText
                 : model.genre === "rubrique" ? fenetre.couleurRubrique
                 : fenetre.palette.windowText
            font.bold: entete || model.nonLus > 0
            // Les en-têtes, rubrique et comptes, un cran au-dessus des
            // dossiers ; sous une apparence de marque, dans la police
            // des titres, la rubrique en capitales espacées.
            font.pointSize: entete ? fenetre.tailleArborescence * (fenetre.titresEtroits ? 1.25 : 1.12)
                                   : fenetre.tailleArborescence
            font.family: fenetre.marque && entete ? fenetre.policeTitres : fenetre.font.family
            font.capitalization: fenetre.marque && model.genre === "rubrique" ? Font.AllUppercase : Font.MixedCase
            font.letterSpacing: fenetre.marque && model.genre === "rubrique" ? 0.8 : 0
            font.italic: model.masque || model.genre === "favori-vide"
            opacity: model.masque || model.genre === "favori-vide"
                     || (ligne.estDossier && !model.selectionnable) ? 0.5 : 1
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
        Label {
            visible: model.genre === "compte" && model.etat !== "pret"
            text: model.etat === "connexion" ? qsTr("connexion…")
                : model.etat === "erreur" ? qsTr("erreur") : qsTr("hors ligne")
            color: model.etat === "erreur" ? "#b00020" : fenetre.palette.windowText
            opacity: 0.7
            font.pointSize: fenetre.tailleArborescence * 0.85
            // Le motif de l'erreur reste lisible au survol, une fois le
            // message passager effacé.
            HoverHandler { id: survolEtat }
            ToolTip.visible: survolEtat.hovered && model.motif.length > 0
            ToolTip.text: model.motif
        }
        Label {
            visible: ligne.estDossier && model.nonLus > 0 && fenetre.marque === null
            text: model.nonLus
            font.bold: true
            color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.texteAccent
        }
        CompteurNonLus {
            visible: ligne.estDossier && model.nonLus > 0 && fenetre.marque !== null
            nombre: model.nonLus
            Layout.rightMargin: 2
        }
    }

    // Dossier ouvert, sous une apparence de marque : un filet cyan à
    // gauche de la ligne.
    Rectangle {
        visible: fenetre.marque !== null && ligne.highlighted
        width: 4
        height: parent.height
        color: fenetre.accent
    }
}
