// SPDX-License-Identifier: GPL-3.0-or-later
// Volet du message : barre d'actions, en-tête, invitation, pièces jointes, corps.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages, modeleMessages…) sont ceux de
// Main.qml, où « VoletLecture { id: colonneLecture } » l'instancie dans les volets.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// --- message ------------------------------------------------------
ColumnLayout {
    id: colonneLecture
    // Désignés hors de ce volet (Main.qml, fichiers voisins) : colonneLecture.<nom>.
    property alias sujetAffiche: sujetAffiche
    property alias auteurAffiche: auteurAffiche
    property alias cadreCorps: cadreCorps
    property alias vueCorps: vueCorps
    property alias mefLecture: mefLecture
    visible: !fenetre.compact || fenetre.vue === 2
    SplitView.fillWidth: true
    spacing: 0

    /// Retire la sélection du message sans bouger la lecture. Placé dans un
    /// ScrollView, le TextArea ramène la vue à son curseur dès que celui-ci
    /// change — deselect() suffit — : sous Android, chaque nouveau glissement
    /// remontait ainsi le message en haut (0.6.10, constaté par Manu).
    function deselectionner() {
        if (vueCorps.selectionStart === vueCorps.selectionEnd)
            return
        var flick = cadreCorps.contentItem
        var x = flick.contentX
        var y = flick.contentY
        vueCorps.deselect()
        flick.contentX = x
        flick.contentY = y
    }

    HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "message" }

    ToolBar {
        Layout.fillWidth: true
        visible: fenetre.uidCourant > 0
        font.pointSize: fenetre.tailleMessage
        leftPadding: 8
        rightPadding: 8
        // Les actions sur leur ligne, au-dessus de l'objet, comme dans
        // le volet de lecture d'Outlook : à côté, elles tronquaient
        // l'objet et l'expéditeur. Elles passent à la ligne si la
        // colonne est étroite.
        ColumnLayout {
            width: parent.width
            spacing: 0
            // De vrais boutons, encadrés et espacés, en trois groupes —
            // répondre, ranger, voir la source — : des boutons plats de
            // barre d'outils se lisaient mal (retour de Manu, 01/10).
            Flow {
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.bottomMargin: 6
                spacing: 6
                // Un brouillon se reprend ; un message reçu reçoit une réponse.
                BoutonAction {
                    visible: fenetre.dossierDeReprise
                    text: qsTr("Reprendre")
                    onClicked: fenetre.rediger("brouillon")
                }
                BoutonAction {
                    visible: !fenetre.dossierDeReprise
                    text: qsTr("Répondre")
                    onClicked: fenetre.rediger("repondre")
                }
                BoutonAction {
                    visible: !fenetre.dossierDeReprise
                    text: qsTr("Répondre à tous")
                    onClicked: fenetre.rediger("repondre_tous")
                }
                BoutonAction {
                    visible: !fenetre.dossierDeReprise
                    text: qsTr("Transférer")
                    onClicked: fenetre.rediger("transferer")
                }
                ToolSeparator { height: boutonDeplacer.height }
                BoutonAction {
                    id: boutonDeplacer
                    text: qsTr("Déplacer…")
                    onClicked: fenetre.ouvrirDeplacer()
                }
                BoutonAction {
                    visible: !fenetre.dossierDeReprise
                    text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
                    onClicked: fenetre.signalerIndesirable()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: fenetre.roleCourant === "Junk"
                                  ? qsTr("Remettre en boîte de réception ; le filtre du serveur apprend que le message est légitime (Ctrl+Alt+J).")
                                  : qsTr("Classer en courrier indésirable ; le filtre du serveur l'apprend (Ctrl+Alt+J).")
                }
                BoutonAction {
                    text: qsTr("Supprimer")
                    onClicked: fenetre.supprimerSelection()
                }
                ToolSeparator { height: boutonDeplacer.height }
                // Décision 6 : voir le message brut, en-têtes compris, en un geste.
                BoutonAction {
                    text: fenetre.sourceVisible ? qsTr("Message") : qsTr("Source")
                    onClicked: fenetre.basculerSource()
                }
            }
            // L'objet, en titre dans la police des titres sous une
            // apparence de marque. L'objet et l'expéditeur se
            // sélectionnent et se copient (Ctrl+C, clic droit) ; trop
            // longs, ils passent à la ligne au lieu d'être coupés.
            EnTeteSelectionnable {
                id: sujetAffiche
                font.bold: true
                font.family: fenetre.marque ? fenetre.policeTitres : fenetre.font.family
                font.pointSize: fenetre.tailleMessage * (fenetre.titresEtroits ? 1.4
                                                         : fenetre.marque ? 1.2 : 1)
                Layout.fillWidth: true
                Layout.topMargin: fenetre.marque ? 2 : 0
            }
            EnTeteSelectionnable {
                id: auteurAffiche
                opacity: 0.8
                Layout.fillWidth: true
            }
            Label {
                visible: fenetre.uidCourant > 0 && fenetre.importanceCourante !== 0
                text: fenetre.importanceCourante > 0 ? qsTr("Importance haute") : qsTr("Importance basse")
                color: fenetre.importanceCourante > 0 ? fenetre.couleurAlerte : "#1a4480"
                font.bold: true
            }
        }
    }

    // L'expéditeur demande une confirmation de lecture : on la propose,
    // sans l'envoyer d'office — comme Outlook.
    Pane {
        Layout.fillWidth: true
        visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.confirmationDemandee.length > 0
        padding: 4
        background: Rectangle { color: "#fff8e1" }
        RowLayout {
            width: parent.width
            Label {
                text: qsTr("L'expéditeur demande une confirmation de lecture (à %1).").arg(fenetre.confirmationDemandee)
                color: "#1c1f24"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Envoyer")
                onClicked: {
                    boite.repondreConfirmation(fenetre.uidCourant, true)
                    fenetre.confirmationDemandee = ""
                    messageEtat.texte = qsTr("Confirmation de lecture envoyée.")
                }
            }
            Button {
                text: qsTr("Ignorer")
                onClicked: {
                    boite.repondreConfirmation(fenetre.uidCourant, false)
                    fenetre.confirmationDemandee = ""
                }
            }
        }
    }

    // Invitation, réponse ou annulation portée par le message : le
    // bandeau d'Outlook, avec de quoi y donner suite. Le serveur
    // d'agenda (SOGo) envoie lui-même la réponse à l'organisateur.
    Pane {
        id: bandeauInvitation
        readonly property var inv: fenetre.invitation.invitation || ({})
        readonly property string methode: inv.methode || ""
        readonly property var organisateur: inv.organisateur || ({})
        readonly property var repondant: (inv.participants || [])[0] || ({})
        readonly property bool active: fenetre.invitation.planification !== false
        Layout.fillWidth: true
        visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && methode.length > 0
        font.pointSize: fenetre.tailleMessage
        padding: 8
        background: Rectangle {
            color: Qt.tint(fenetre.palette.base, "#263a7bd5")
            border.color: "#3a7bd5"
            border.width: 1
            radius: 3
        }
        ColumnLayout {
            width: parent.width
            spacing: 3
            Label {
                Layout.fillWidth: true
                font.bold: true
                wrapMode: Text.Wrap
                text: bandeauInvitation.methode === "CANCEL" ? qsTr("Réunion annulée : %1").arg(bandeauInvitation.inv.titre || "")
                      : bandeauInvitation.methode === "REPLY"
                        ? qsTr("Réponse de %1 : %2 « %3 »").arg(bandeauInvitation.repondant.nom || bandeauInvitation.repondant.adresse || "")
                              .arg(fenetre.verbeReponse(bandeauInvitation.repondant.statut)).arg(bandeauInvitation.inv.titre || "")
                      : qsTr("Invitation : %1").arg(bandeauInvitation.inv.titre || "")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: (bandeauInvitation.inv.quand || "") + (bandeauInvitation.inv.repete ? qsTr(" — se répète") : "")
                      + ((bandeauInvitation.inv.lieu || "").length > 0 ? " — " + bandeauInvitation.inv.lieu : "")
            }
            Label {
                Layout.fillWidth: true
                visible: bandeauInvitation.methode !== "REPLY" && (bandeauInvitation.organisateur.adresse || "").length > 0
                wrapMode: Text.Wrap
                opacity: 0.8
                text: qsTr("Organisateur : %1").arg(bandeauInvitation.organisateur.nom
                      ? bandeauInvitation.organisateur.nom + " <" + bandeauInvitation.organisateur.adresse + ">"
                      : bandeauInvitation.organisateur.adresse || "")
                      + ((bandeauInvitation.inv.participants || []).length > 0
                         ? qsTr(" — %1").arg(fenetre.accord(bandeauInvitation.inv.participants.length, qsTr("participant"), qsTr("participants"))) : "")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.8
                text: fenetre.etatInvitation()
                visible: text.length > 0
            }
            Flow {
                Layout.fillWidth: true
                spacing: 6
                visible: bandeauInvitation.methode === "REQUEST" || (bandeauInvitation.methode === "CANCEL" && fenetre.invitation.dansAgenda)
                Repeater {
                    model: bandeauInvitation.methode === "REQUEST"
                           ? [["ACCEPTED", qsTr("Accepter")], ["TENTATIVE", qsTr("Provisoire")], ["DECLINED", qsTr("Refuser")]]
                           : [["RETIRER", qsTr("Retirer de l'agenda")]]
                    delegate: Button {
                        required property var modelData
                        text: modelData[1]
                        enabled: bandeauInvitation.active && fenetre.reponseInvitation.length === 0
                        highlighted: fenetre.invitation.maReponse === modelData[0]
                        onClicked: fenetre.donnerSuite(modelData[0])
                    }
                }
                BusyIndicator {
                    running: fenetre.reponseInvitation.length > 0
                    visible: running
                    height: 28
                    width: 28
                }
            }
        }
    }

    // Pièces jointes du message affiché, mises en valeur sous l'en-tête :
    // une ligne par pièce — type, nom, taille, « Ouvrir », « Enregistrer
    // sous… » (forme choisie par Manu le 09/10 sur maquettes). Clic droit,
    // ou appui long : ouvrir avec un autre logiciel, joindre à un message.
    Pane {
        Layout.fillWidth: true
        visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.pieces.length > 0
        font.pointSize: fenetre.tailleMessage
        padding: 6
        background: Rectangle { color: fenetre.palette.window }
        ColumnLayout {
            width: parent.width
            spacing: 2
            Label {
                text: qsTr("Pièces jointes (%1)").arg(fenetre.pieces.length)
                font.bold: true
            }
            Repeater {
                model: fenetre.pieces
                delegate: RowLayout {
                    id: lignePiece
                    required property var modelData
                    readonly property var genre: fenetre.genrePiece(modelData.nom)
                    readonly property string logiciel: Qt.platform.os === "android" ? ""
                                                       : fenetre.logicielPour(modelData.nom)
                    Layout.fillWidth: true
                    spacing: 8
                    // Au doigt, c'est l'appui long ci-dessous : le
                    // toucher ne doit pas passer pour un clic droit.
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
                        onTapped: menuPiece.ouvrir(lignePiece.modelData, lignePiece)
                    }
                    TapHandler {
                        acceptedDevices: PointerDevice.TouchScreen
                        onLongPressed: menuPiece.ouvrir(lignePiece.modelData, lignePiece)
                    }
                    // Étiquette du type, en couleur : PDF, XLSX, JPG…
                    Rectangle {
                        implicitWidth: 38
                        implicitHeight: 20
                        radius: 3
                        color: lignePiece.genre.couleur
                        Label {
                            anchors.centerIn: parent
                            text: lignePiece.genre.libelle
                            color: "white"
                            font.bold: true
                            font.pointSize: fenetre.tailleMessage * 0.6
                        }
                    }
                    Label {
                        text: lignePiece.modelData.nom
                        elide: Text.ElideMiddle
                        Layout.fillWidth: true
                    }
                    Label {
                        text: fenetre.tailleLisible(lignePiece.modelData.taille)
                        opacity: 0.65
                    }
                    Button {
                        text: qsTr("Ouvrir")
                        flat: true
                        enabled: !lignePiece.modelData.risquee
                        onClicked: fenetre.ouvrirPiece(lignePiece.modelData)
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: lignePiece.logiciel.length > 0
                                      ? qsTr("Avec %1, choisi pour les fichiers .%2 (clic droit : ouvrir avec…)")
                                        .arg(lanceur.nom(lignePiece.logiciel)).arg(fenetre.extensionDe(lignePiece.modelData.nom))
                                      : qsTr("Avec le logiciel du système (clic droit : ouvrir avec…)")
                    }
                    Button {
                        text: qsTr("Enregistrer sous…")
                        flat: true
                        // Sous Android, le dialogue rend une adresse
                        // « content:// » que le noyau ne sait pas écrire.
                        visible: Qt.platform.os !== "android"
                        onClicked: fenetre.enregistrerPiece(lignePiece.modelData)
                        ToolTip.visible: hovered && lignePiece.modelData.risquee
                        ToolTip.text: qsTr("Programme ou script : il s'enregistre, il ne s'ouvre pas depuis MMail.")
                    }
                }
            }
        }
    }

    // Images distantes : rien ne se télécharge sans le demander — une
    // image distante dit à l'expéditeur que le message a été ouvert.
    Pane {
        Layout.fillWidth: true
        visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.corpsHtml
                 && fenetre.imagesBloquees > 0
        font.pointSize: fenetre.tailleMessage
        padding: 4
        background: Rectangle { color: "#e8f0fb" }
        RowLayout {
            width: parent.width
            Label {
                text: (fenetre.imagesBloquees > 1
                       ? qsTr("%1 images distantes non téléchargées").arg(fenetre.imagesBloquees)
                       : qsTr("1 image distante non téléchargée"))
                      + qsTr(" : l'expéditeur saurait que vous avez ouvert le message.")
                color: "#1c1f24"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Télécharger les images")
                onClicked: {
                    messageEtat.texte = qsTr("Téléchargement des images…")
                    boite.afficherImages(fenetre.uidCourant)
                }
            }
        }
    }

    ScrollView {
        id: cadreCorps
        Layout.fillWidth: true
        Layout.fillHeight: true
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        // Réserve fixe : le texte se replie, sa hauteur dépend donc de la
        // largeur, et une réserve qui suivrait le débordement bouclerait.
        rightPadding: ScrollBar.vertical.width
        // La source n'est pas repliée : elle défile aussi en largeur.
        ScrollBar.horizontal.policy: ScrollBar.horizontal.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        bottomPadding: fenetre.sourceVisible ? ScrollBar.horizontal.height : 0
        clip: true
        contentWidth: fenetre.sourceVisible ? -1 : availableWidth

        // Android : faire défiler le message retire la sélection, et
        // avec elle la barre d'Android (cf. fenetre.androidSansSelection).
        Connections {
            target: cadreCorps.contentItem
            ignoreUnknownSignals: true
            enabled: Qt.platform.os === "android"
            function onMovementStarted() { colonneLecture.deselectionner() }
        }

        TextArea {
            id: vueCorps
            readOnly: true
            selectByMouse: true
            // Un message HTML en texte riche ; tout le reste — source,
            // texte brut, attente — en texte simple, où une balise
            // reste une balise.
            textFormat: fenetre.corpsHtml && !fenetre.sourceVisible ? TextEdit.RichText : TextEdit.PlainText
            wrapMode: fenetre.sourceVisible ? TextEdit.NoWrap : TextEdit.Wrap
            font.pointSize: fenetre.tailleMessage
            font.family: fenetre.sourceVisible ? fenetre.policeMono : fenetre.font.family
            // Un message HTML se lit sur fond blanc, quelle que soit
            // l'apparence : ses couleurs supposent ce fond.
            color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#1f1f1f" : fenetre.palette.text
            background: Rectangle {
                color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#ffffff" : fenetre.palette.base
            }
            text: qsTr("Aucun message sélectionné.")

            onLinkActivated: function(lien) { fenetre.ouvrirLien(lien) }
            onLinkHovered: function(lien) { fenetre.lienSurvole = lien }

            // Une image plus large que la colonne y est ramenée, et
            // suit la colonne quand elle change de largeur.
            MiseEnForme {
                id: mefLecture
                document: vueCorps.textDocument
            }
            onWidthChanged: minuterieImages.restart()
            Timer {
                id: minuterieImages
                interval: 120
                onTriggered: fenetre.bornerImages()
            }
            HoverHandler {
                cursorShape: vueCorps.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
            }

            onSelectedTextChanged: if (selectedText.length > 0) fenetre.selectionChangee()
            // Un Ctrl+C explicite écrit par la même règle que la copie
            // automatique : sans cela, la règle y verrait un dépôt venu
            // d'ailleurs, et se protégerait contre MMail lui-même.
            Keys.onPressed: function(touche) {
                if (touche.matches(StandardKey.Copy) && selectedText.length > 0) {
                    fenetre.copierTexte(selectedText, fenetre.sourceVisible ? "source" : "message")
                    touche.accepted = true
                    // « Copier » de la barre d'Android passe par ici :
                    // la copie faite, la sélection et la barre s'en vont.
                    if (Qt.platform.os === "android")
                        colonneLecture.deselectionner()
                }
            }
            // Clic droit de la souris seulement : un TapHandler ne trie
            // les boutons que pour la souris, et le simple toucher d'un
            // doigt ouvrait ce menu (constaté le 13/10 sous Android).
            TapHandler {
                acceptedButtons: Qt.RightButton
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
                onTapped: menuCorps.popup()
            }
            // Sous Android, l'appui long est à Android : il choisit le mot
            // et ouvre sa barre (Copier, Tout sélectionner). Le menu de
            // MMail, ouvert par-dessus, lui prenait le focus et le
            // rendait au texte, ce qui rouvrait la barre.
            onPressAndHold: if (fenetre.compact && Qt.platform.os !== "android") menuCorps.popup()

            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: function(roue) {
                    fenetre.zoomer("message", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                }
            }
            PinchHandler {
                target: null
                property real depart: 1
                onActiveChanged: if (active) depart = reglages.zoomMessage
                onActiveScaleChanged: if (active) fenetre.poserZoom("message", depart * activeScale)
            }
        }
    }
}
