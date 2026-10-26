// SPDX-License-Identifier: GPL-3.0-or-later
// Fenêtre d'un message, ouverte d'un double-clic (0.6.16, demande de
// Dominique BARTHELEMY précisée par Manu le 23/10) : comme dans Outlook, le
// message dans sa propre fenêtre, avec ses actions en haut. Plusieurs
// peuvent être ouvertes à la fois ; chacune vise son message — compte,
// dossier, UID —, quel que soit le dossier ouvert dans la fenêtre principale.
//
// Les noms employés sans être déclarés ici (fenetre, boite, reglages…) sont
// ceux de Main.qml, où « composantFenetreMessage » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// ApplicationWindow et non Window : c'est elle qui transmet police et
// palette aux éléments flottants (menus, dialogues).
ApplicationWindow {
    id: fenetreMessage
    font: fenetre.font
    palette: fenetre.palette
    width: Math.min(900, Screen.desktopAvailableWidth - 40)
    height: Math.min(760, Screen.desktopAvailableHeight - 60)
    minimumWidth: 480
    minimumHeight: 360
    // Fenêtre indépendante, qui peut passer derrière la principale.
    transientParent: null
    visible: true
    color: fenetre.palette.window
    title: sujet

    // Le message, tel que la liste le montrait à l'ouverture.
    // Animations des GIF du corps (scénarios d'essai).
    property alias animations: animationsFenetre
    property alias corps: vueCorps
    property int compte: 0
    property string chemin: ""
    property int uid: 0
    property string role: ""
    property string sujet: ""
    property string expediteur: ""
    property string adresse: ""
    property string date: ""
    property bool lu: true
    property bool suivi: false
    property string motsCles: ""
    // Ce que la lecture du message apporte (`corpsRecu`).
    property bool charge: false
    property bool html: false
    property bool source: false
    property var pieces: []
    property var entete: ({})
    readonly property var cible: ({ compte: compte, chemin: chemin, uid: uid })
    readonly property bool brouillon: role === "Drafts" || role === "Differe"
    readonly property var categories: fenetre.pastilles(compte, chemin, motsCles, boite.revisionCategories)

    // Bouton qui ouvre un menu : le libellé suivi d'un triangle, dessiné —
    // le caractère ▾ manque à certaines polices.
    component BoutonMenu: BoutonAction {
        id: bm
        property Menu menu: null
        onClicked: bm.menu.popup(bm, 0, bm.height)
        Canvas {
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 8
            height: 5
            property color encre: bm.contentItem.color
            onEncreChanged: requestPaint()
            onPaint: {
                var c = getContext("2d")
                c.reset()
                c.fillStyle = encre
                c.beginPath()
                c.moveTo(0, 0)
                c.lineTo(width, 0)
                c.lineTo(width / 2, height)
                c.closePath()
                c.fill()
            }
        }
    }

    Component.onCompleted: boite.demanderDans(compte, chemin, uid, false)
    onClosing: {
        fenetre.oublierFenetreMessage(fenetreMessage)
        Qt.callLater(function() { fenetreMessage.destroy() })
    }

    // Le corps lu pour cette fenêtre — ou pour le volet de lecture, s'il
    // s'agit du même message.
    Connections {
        target: boite
        function onCorpsRecu(uidRecu, texte, brut, estHtml, bloquees, piecesRecues, confirmation, enteteRecu,
                             compteRecu, cheminRecu) {
            if (uidRecu !== fenetreMessage.uid || compteRecu !== fenetreMessage.compte
                    || cheminRecu !== fenetreMessage.chemin || brut !== fenetreMessage.source)
                return
            fenetreMessage.html = estHtml && !brut
            vueCorps.text = texte
            vueCorps.cursorPosition = 0
            if (!brut) {
                fenetreMessage.pieces = JSON.parse(piecesRecues)
                try { fenetreMessage.entete = JSON.parse(enteteRecu) } catch (e) { fenetreMessage.entete = {} }
            }
            fenetreMessage.charge = true
            bornerImages()
            if (fenetre.essai && fenetre.essai.scenario === "double-clic")
                scenarios.etapeScenarioDoubleClic(fenetreMessage)
        }
    }

    function bornerImages() {
        if (html)
            mef.bornerImages(Math.floor(vueCorps.width - vueCorps.leftPadding - vueCorps.rightPadding) - 2)
    }

    function basculerSource() {
        source = !source
        charge = false
        html = false
        vueCorps.text = qsTr("Chargement…")
        boite.demanderDans(compte, chemin, uid, source)
    }

    function marquer(nouveau) {
        if (boite.marquerLuDans(compte, chemin, String(uid), nouveau)) {
            lu = nouveau
            fenetre.apresMarquageFenetre(cible, "lu", nouveau)
        }
    }

    function basculerDrapeau() {
        if (boite.marquerSuiviDans(compte, chemin, String(uid), !suivi)) {
            suivi = !suivi
            fenetre.apresMarquageFenetre(cible, "suivi", suivi)
        }
    }

    function basculerCategorie(motCle, poser) {
        if (!boite.marquerCategorieDans(compte, chemin, String(uid), motCle, poser))
            return
        motsCles = fenetre.modifierMotsCles(motsCles, motCle, poser)
        fenetre.apresMarquageFenetre(cible, "motsCles", motsCles)
    }

    /// Le message quitte son dossier : la fenêtre se ferme, la liste suit.
    function retirer(fait) {
        if (!fait)
            return
        fenetre.retirerMessageDeLaListe(cible)
        close()
    }

    function supprimer() {
        retirer(boite.supprimerDans(compte, chemin, String(uid)))
    }

    function indesirable() {
        retirer(boite.signalerIndesirableDans(compte, chemin, String(uid)))
    }

    function deplacerVers(compteCible, cheminCible) {
        retirer(boite.deplacerDans(compte, chemin, String(uid), compteCible, cheminCible))
    }

    function ouvrirDeplacer() {
        dlgDeplacerFenetre.cibles = JSON.parse(boite.dossiersCibles())
        dlgDeplacerFenetre.open()
    }

    function rediger(mode) {
        fenetre.redigerDepuis(cible, mode)
    }

    /// En-tête d'impression : De, À, Cc, date et objet, comme le volet.
    function enteteImpression() {
        var lignes = [qsTr("De : %1").arg(entete.de || expediteur)]
        if ((entete.a || "").length > 0)
            lignes.push(qsTr("À : %1").arg(entete.a))
        if ((entete.cc || "").length > 0)
            lignes.push(qsTr("Cc : %1").arg(entete.cc))
        lignes.push(qsTr("Date : %1").arg(fenetre.dateLongue(entete.date || date)))
        lignes.push(qsTr("Objet : %1").arg(sujet))
        return lignes.join("\n")
    }

    function imprimer() {
        if (impression.imprimer(vueCorps.textDocument, enteteImpression(), sujet))
            messageEtat.texte = qsTr("Message envoyé à l'imprimante.")
    }

    function enregistrerPdf() {
        choixPdfFenetre.currentFolder = impression.dossierDocuments()
        choixPdfFenetre.selectedFile = choixPdfFenetre.currentFolder + "/" + fenetre.nomDeFichier(sujet)
        choixPdfFenetre.open()
    }

    function ouvrirPiece(piece) {
        if (!piece || piece.risquee)
            return
        fenetre.ouvertureAvec = null
        if (boite.ouvrirPieceDans(compte, chemin, uid, piece.indice))
            messageEtat.texte = qsTr("Préparation de %1…").arg(piece.nom)
    }

    function enregistrerPiece(piece) {
        dlgPieceFenetre.piece = piece
        var dossier = fenetre.dossierEnregistrement()
        dlgPieceFenetre.currentFolder = dossier
        dlgPieceFenetre.selectedFile = dossier + "/" + encodeURIComponent(piece.nom)
        dlgPieceFenetre.open()
    }

    FileDialog {
        id: dlgPieceFenetre
        property var piece: null
        title: qsTr("Enregistrer la pièce jointe")
        fileMode: FileDialog.SaveFile
        onAccepted: {
            var fichier = selectedFile.toString()
            reglagesPieces.dernierDossier = fichier.substring(0, fichier.lastIndexOf("/"))
            boite.enregistrerPieceDans(fenetreMessage.compte, fenetreMessage.chemin, fenetreMessage.uid,
                                       piece.indice, fichier)
        }
    }

    FileDialog {
        id: choixPdfFenetre
        title: qsTr("Enregistrer le message en PDF")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("Documents PDF (*.pdf)")]
        defaultSuffix: "pdf"
        onAccepted: {
            var ok = impression.enregistrerPdf(vueCorps.textDocument, fenetreMessage.enteteImpression(),
                                               fenetreMessage.sujet, selectedFile.toString())
            messageEtat.texte = ok ? qsTr("Message enregistré en PDF.") : qsTr("Le PDF n'a pas pu être écrit.")
        }
    }

    DialogueDeplacer {
        id: dlgDeplacerFenetre
        pour: fenetreMessage
    }

    // Raccourcis d'Outlook, dans cette fenêtre ; Échap la ferme.
    Shortcut { sequence: "Escape"; onActivated: fenetreMessage.close() }
    Shortcut { sequence: "Ctrl+R"; enabled: !fenetreMessage.brouillon; onActivated: fenetreMessage.rediger("repondre") }
    Shortcut { sequence: "Ctrl+Shift+R"; enabled: !fenetreMessage.brouillon; onActivated: fenetreMessage.rediger("repondre_tous") }
    Shortcut { sequence: "Ctrl+F"; enabled: !fenetreMessage.brouillon; onActivated: fenetreMessage.rediger("transferer") }
    Shortcut { sequence: "Ctrl+Q"; onActivated: fenetreMessage.marquer(true) }
    Shortcut { sequence: "Ctrl+U"; onActivated: fenetreMessage.marquer(false) }
    Shortcut { sequence: "Ins"; onActivated: fenetreMessage.basculerDrapeau() }
    Shortcut { sequence: "Ctrl+Shift+V"; onActivated: fenetreMessage.ouvrirDeplacer() }
    Shortcut { sequence: "Ctrl+Alt+J"; enabled: !fenetreMessage.brouillon; onActivated: fenetreMessage.indesirable() }
    Shortcut { sequences: [StandardKey.Delete]; onActivated: fenetreMessage.supprimer() }
    Shortcut {
        sequences: [StandardKey.Print]
        enabled: impression.disponible && fenetreMessage.charge
        onActivated: fenetreMessage.imprimer()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ToolBar {
            Layout.fillWidth: true
            leftPadding: 8
            rightPadding: 8
            // Comme le volet de lecture : de vrais boutons, en groupes —
            // répondre, marquer, ranger —, puis un menu pour le reste.
            Flow {
                width: parent.width
                topPadding: 4
                bottomPadding: 6
                spacing: 6
                BoutonAction {
                    visible: fenetreMessage.brouillon
                    text: qsTr("Reprendre")
                    onClicked: fenetreMessage.rediger("brouillon")
                }
                BoutonAction {
                    visible: !fenetreMessage.brouillon
                    text: qsTr("Répondre")
                    onClicked: fenetreMessage.rediger("repondre")
                }
                BoutonAction {
                    visible: !fenetreMessage.brouillon
                    text: qsTr("Répondre à tous")
                    onClicked: fenetreMessage.rediger("repondre_tous")
                }
                BoutonAction {
                    visible: !fenetreMessage.brouillon
                    text: qsTr("Transférer")
                    onClicked: fenetreMessage.rediger("transferer")
                }
                ToolSeparator { height: boutonDeplacerFenetre.height }
                BoutonAction {
                    text: fenetreMessage.lu ? qsTr("Marquer comme non lu") : qsTr("Marquer comme lu")
                    onClicked: fenetreMessage.marquer(!fenetreMessage.lu)
                }
                BoutonAction {
                    text: fenetreMessage.suivi ? qsTr("Retirer le drapeau") : qsTr("Drapeau")
                    onClicked: fenetreMessage.basculerDrapeau()
                }
                BoutonMenu {
                    // De la place pour le triangle.
                    text: qsTr("Catégoriser") + "    "
                    menu: menuCategoriesFenetre
                }
                ToolSeparator { height: boutonDeplacerFenetre.height }
                BoutonAction {
                    id: boutonDeplacerFenetre
                    text: qsTr("Déplacer…")
                    onClicked: fenetreMessage.ouvrirDeplacer()
                }
                BoutonAction {
                    visible: !fenetreMessage.brouillon
                    text: fenetreMessage.role === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
                    onClicked: fenetreMessage.indesirable()
                }
                BoutonAction {
                    text: qsTr("Supprimer")
                    onClicked: fenetreMessage.supprimer()
                }
                ToolSeparator { height: boutonDeplacerFenetre.height }
                BoutonMenu {
                    text: qsTr("Autres actions") + "    "
                    menu: menuAutresFenetre
                }
            }
        }

        // En-tête : objet, puis De, À, Cc, date ; les catégories à leur nom.
        Pane {
            Layout.fillWidth: true
            visible: !fenetreMessage.source
            padding: 8
            topPadding: 4
            font.pointSize: fenetre.tailleMessage
            background: Rectangle { color: fenetre.palette.window }
            GridLayout {
                width: parent.width
                columns: 2
                columnSpacing: 8
                rowSpacing: 2
                EnTeteSelectionnable {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    text: fenetreMessage.sujet
                    font.bold: true
                    font.family: fenetre.marque ? fenetre.policeTitres : fenetre.font.family
                    font.pointSize: fenetre.tailleMessage * (fenetre.marque ? 1.2 : 1.1)
                }
                Label { text: qsTr("De :"); opacity: 0.7 }
                EnTeteSelectionnable {
                    Layout.fillWidth: true
                    font.family: fenetre.font.family
                    text: fenetreMessage.entete.de || (fenetreMessage.expediteur
                          + (fenetreMessage.adresse.length > 0 && fenetreMessage.adresse !== fenetreMessage.expediteur
                             ? " <" + fenetreMessage.adresse + ">" : ""))
                }
                Label { text: qsTr("À :"); opacity: 0.7; visible: (fenetreMessage.entete.a || "").length > 0 }
                EnTeteSelectionnable {
                    Layout.fillWidth: true
                    font.family: fenetre.font.family
                    visible: (fenetreMessage.entete.a || "").length > 0
                    text: fenetreMessage.entete.a || ""
                }
                Label { text: qsTr("Cc :"); opacity: 0.7; visible: (fenetreMessage.entete.cc || "").length > 0 }
                EnTeteSelectionnable {
                    Layout.fillWidth: true
                    font.family: fenetre.font.family
                    visible: (fenetreMessage.entete.cc || "").length > 0
                    text: fenetreMessage.entete.cc || ""
                }
                Label { text: qsTr("Date :"); opacity: 0.7 }
                Label {
                    Layout.fillWidth: true
                    text: fenetre.dateLongue(fenetreMessage.entete.date || fenetreMessage.date)
                }
                Flow {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.topMargin: 3
                    spacing: 6
                    visible: fenetreMessage.categories.length > 0
                    Repeater {
                        model: fenetreMessage.categories
                        PastilleCategorie {
                            required property var modelData
                            avecNom: true
                            nom: modelData.nom
                            couleur: modelData.couleur
                            taillePolice: fenetre.tailleMessage * 0.85
                        }
                    }
                }
            }
        }

        // Pièces jointes : une ligne par pièce, comme au volet de lecture.
        Pane {
            Layout.fillWidth: true
            visible: !fenetreMessage.source && fenetreMessage.pieces.length > 0
            font.pointSize: fenetre.tailleMessage
            padding: 6
            background: Rectangle { color: fenetre.palette.window }
            ColumnLayout {
                width: parent.width
                spacing: 2
                Label {
                    text: qsTr("Pièces jointes (%1)").arg(fenetreMessage.pieces.length)
                    font.bold: true
                }
                Repeater {
                    model: fenetreMessage.pieces
                    delegate: RowLayout {
                        id: lignePieceFenetre
                        required property var modelData
                        readonly property var genre: fenetre.genrePiece(modelData.nom)
                        Layout.fillWidth: true
                        spacing: 8
                        TapHandler {
                            acceptedButtons: Qt.LeftButton
                            onDoubleTapped: fenetreMessage.ouvrirPiece(lignePieceFenetre.modelData)
                        }
                        Rectangle {
                            Layout.preferredWidth: 38
                            Layout.preferredHeight: 20
                            radius: 3
                            color: lignePieceFenetre.genre.couleur
                            Label {
                                anchors.centerIn: parent
                                text: lignePieceFenetre.genre.libelle
                                color: "#ffffff"
                                font.bold: true
                                font.pointSize: fenetre.tailleMessage * 0.7
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: lignePieceFenetre.modelData.nom
                            elide: Text.ElideMiddle
                        }
                        Label {
                            text: fenetre.tailleLisible(lignePieceFenetre.modelData.taille)
                            opacity: 0.7
                        }
                        Button {
                            text: qsTr("Ouvrir")
                            flat: true
                            enabled: !lignePieceFenetre.modelData.risquee
                            onClicked: fenetreMessage.ouvrirPiece(lignePieceFenetre.modelData)
                        }
                        Button {
                            text: qsTr("Enregistrer sous…")
                            flat: true
                            onClicked: fenetreMessage.enregistrerPiece(lignePieceFenetre.modelData)
                        }
                    }
                }
            }
        }

        ScrollView {
            id: cadreCorps
            Layout.fillWidth: true
            Layout.fillHeight: true
            rightPadding: ScrollBar.vertical.width
            clip: true
            contentWidth: fenetreMessage.source ? -1 : availableWidth
            TextArea {
                id: vueCorps
                readOnly: true
                selectByMouse: true
                textFormat: fenetreMessage.html ? TextEdit.RichText : TextEdit.PlainText
                wrapMode: fenetreMessage.source ? TextEdit.NoWrap : TextEdit.Wrap
                font.pointSize: fenetre.tailleMessage
                font.family: fenetreMessage.source ? fenetre.policeMono : fenetre.font.family
                // Un message HTML se lit sur fond blanc, quelle que soit
                // l'apparence : ses couleurs supposent ce fond.
                color: fenetreMessage.html ? "#1f1f1f" : fenetre.palette.text
                background: Rectangle { color: fenetreMessage.html ? "#ffffff" : fenetre.palette.base }
                text: qsTr("Chargement…")
                onLinkActivated: function(lien) { fenetre.ouvrirLien(lien) }
                MiseEnForme {
                    id: mef
                    document: vueCorps.textDocument
                }
                ImagesAnimees {
                    id: animationsFenetre
                    zone: vueCorps
                    miseEnForme: mef
                }
                RenduComplet { zone: vueCorps }
                onWidthChanged: minuterieImagesFenetre.restart()
                Timer {
                    id: minuterieImagesFenetre
                    interval: 120
                    onTriggered: fenetreMessage.bornerImages()
                }
                HoverHandler {
                    cursorShape: vueCorps.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
                }
            }
        }
    }

    // « Catégoriser ▾ » : les catégories de la boîte du message, cochées
    // quand il les porte.
    Menu {
        id: menuCategoriesFenetre
        property var etat: ({ liste: [], refusees: false })
        onAboutToShow: {
            var c = {}
            try { c = JSON.parse(boite.categories(fenetreMessage.compte, fenetreMessage.chemin)) } catch (e) { c = {} }
            etat = { liste: c.liste || [], refusees: c.refusees === true }
            width = fenetre.largeurMenu(menuCategoriesFenetre)
        }
        MenuItem {
            visible: menuCategoriesFenetre.etat.refusees
            height: visible ? implicitHeight : 0
            enabled: false
            text: qsTr("Ce serveur ne garde pas les catégories")
        }
        Repeater {
            model: menuCategoriesFenetre.etat.refusees ? [] : menuCategoriesFenetre.etat.liste
            EntreeMenu {
                required property var modelData
                readonly property bool porte: (" " + fenetreMessage.motsCles.toLowerCase() + " ")
                                              .indexOf(" " + modelData.motCle.toLowerCase() + " ") >= 0
                text: modelData.nom
                checkable: true
                checked: porte
                rightPadding: padding + carreFenetre.width + 12
                onTriggered: fenetreMessage.basculerCategorie(modelData.motCle, !porte)
                PastilleCategorie {
                    id: carreFenetre
                    anchors.right: parent.right
                    anchors.rightMargin: parent.padding
                    anchors.verticalCenter: parent.verticalCenter
                    taille: Math.round(parent.height * 0.42)
                    nom: parent.modelData.nom
                    couleur: parent.modelData.couleur
                }
            }
        }
    }

    Menu {
        id: menuAutresFenetre
        onAboutToShow: width = fenetre.largeurMenu(menuAutresFenetre)
        EntreeMenu {
            text: qsTr("Imprimer…")
            raccourci: "Ctrl+P"
            enabled: impression.disponible && fenetreMessage.charge && !fenetreMessage.source
            onTriggered: fenetreMessage.imprimer()
        }
        EntreeMenu {
            text: qsTr("Enregistrer en PDF…")
            enabled: fenetreMessage.charge && !fenetreMessage.source
            onTriggered: fenetreMessage.enregistrerPdf()
        }
        EntreeMenu {
            text: fenetreMessage.source ? qsTr("Afficher le message") : qsTr("Afficher la source")
            onTriggered: fenetreMessage.basculerSource()
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: fenetreMessage.adresse.length > 0
            onTriggered: fenetre.copierTexte(fenetreMessage.adresse, "message")
        }
        EntreeMenu {
            text: qsTr("Copier l'objet")
            onTriggered: fenetre.copierTexte(fenetreMessage.sujet, "message")
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Créer un filtre…")
            enabled: fenetreMessage.adresse.length > 0
            onTriggered: fenetre.creerFiltrePour(fenetreMessage.cible, fenetreMessage.expediteur, fenetreMessage.adresse)
        }
    }
}
