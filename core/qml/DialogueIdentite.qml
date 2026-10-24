// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Nom et signature » d’un compte, ou de l’une de ses autres
// adresses d’envoi (0.6.7) ; signatures nommées, chaque adresse choisit la
// sienne pour les nouveaux messages et pour les réponses (0.6.18).
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueIdentite { id: dlgIdentite } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgIdentite
    title: qsTr("Nom et signature")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(760, fenetre.width - 24)
    standardButtons: Dialog.Ok | Dialog.Cancel
    property string note: ""
    // Avertissement d'une adresse qui reprend l'identité de son compte : une
    // information, pas une erreur.
    property string noteReprise: ""
    // Fenêtre du badge animé (scénarios d'essai).
    property alias badge: dlgBadge
    property alias animations: animationsSignature
    // Signatures nommées (0.6.18) : la bibliothèque en cours de modification,
    // [{id, nom, signature, html, origine}], et la signature dans l'éditeur.
    property var biblio: []
    property int choisie: -1
    property string biblioChargee: ""
    // Signature telle que l'éditeur l'a reçue : non touchée, elle est gardée
    // telle quelle, sans repasser par le HTML de Qt.
    property string texteCharge: ""
    property string htmlCharge: ""
    // Identités des adresses vues dans cette ouverture, adresse → {nom,
    // sigNouveaux, sigReponses}, et leur valeur au chargement : une adresse
    // à laquelle on n'a pas touché n'est pas écrite — celle qui reprend
    // l'identité de son compte continue de la suivre.
    property var travail: ({})
    property var chargees: ({})
    property string adresseCourante: ""
    // Choix des deux listes « nouveaux messages » et « réponses ».
    readonly property var choixSignatures: [{ id: "", nom: qsTr("(aucune)") }].concat(
        biblio.map(function(s) { return { id: s.id, nom: s.nom } }))

    function ouvrir(compte) {
        fenetre.migrerSignatures()
        biblio = JSON.parse(JSON.stringify(fenetre.signaturesNommees()))
        biblioChargee = JSON.stringify(biblio)
        travail = {}
        chargees = {}
        choisie = -1
        var i = 0
        var liste = fenetre.expediteursConnus
        for (var k = liste.length - 1; k >= 0; --k)
            if (liste[k].compte === compte)
                i = k
        choixIdentite.currentIndex = i
        adresseCourante = ""
        charger()
        // Première signature : une signature à écrire, gardée si on l'écrit.
        if (biblio.length === 0)
            biblio = [{ id: "1", nom: qsTr("Signature"), signature: "", html: "", provisoire: true }]
        var t = travail[adresseCourante] || {}
        editer(Math.max(0, indexSignature(t.sigNouveaux || t.sigReponses || "")))
        open()
    }
    /// Passe à l'adresse `adresse` de la liste (scénarios d'essai).
    function choisirAdresse(adresse) {
        var liste = fenetre.expediteursConnus
        for (var k = 0; k < liste.length; ++k)
            if (liste[k].adresse === adresse)
                choixIdentite.currentIndex = k
        charger()
    }
    function expediteur() {
        return fenetre.expediteursConnus[choixIdentite.currentIndex] || null
    }
    function adresse() {
        var e = expediteur()
        return e ? e.adresse : ""
    }
    /// Nom et signature affichés (scénarios d'essai) : la signature est celle
    /// de l'éditeur.
    function valeurs() {
        return { nom: champNomAffiche.text.trim(), signature: mefSignature.texte(),
                 html: mefSignature.enrichi() ? mefSignature.html() : "",
                 nouveaux: choixNouveaux.currentIndex > 0, reponses: choixReponses.currentIndex > 0 }
    }
    function indexSignature(id) {
        for (var k = 0; k < biblio.length; ++k)
            if (biblio[k].id === id)
                return k
        return -1
    }
    function indexChoix(id) {
        return Math.max(0, indexSignature(id) + 1)
    }
    /// Ce que l'on change pour l'adresse affichée — nom, l'une des deux
    /// signatures — est gardé aussitôt.
    function changerAdresse(champ, valeur) {
        if (travail[adresseCourante])
            travail[adresseCourante][champ] = valeur
    }
    /// Affiche l'identité de l'adresse choisie dans la liste du haut.
    function charger() {
        var e = expediteur()
        var a = e ? e.adresse : ""
        if (!(a in travail)) {
            var i = fenetre.identiteExpediteur(e)
            var t = travail, c = chargees
            t[a] = { nom: i.nom, sigNouveaux: i.sigNouveaux, sigReponses: i.sigReponses }
            c[a] = JSON.stringify(t[a])
            travail = t
            chargees = c
        }
        adresseCourante = a
        champNomAffiche.text = travail[a].nom
        poserChoix()
        var reprise = e && e.adresse !== e.compteAdresse && !fenetre.aUneIdentite(e.adresse)
        noteReprise = reprise ? qsTr("Cette adresse reprend le nom et la signature de %1 ; modifiés ici, ils lui deviennent propres.").arg(e.compteAdresse) : ""
        note = noteReprise
    }
    function poserChoix() {
        var t = travail[adresseCourante] || {}
        choixNouveaux.currentIndex = indexChoix(t.sigNouveaux || "")
        choixReponses.currentIndex = indexChoix(t.sigReponses || "")
    }
    /// Garde le contenu de l'éditeur dans la signature qu'il montre.
    function garderSignature() {
        if (choisie < 0 || choisie >= biblio.length)
            return
        var s = biblio[choisie]
        if (mefSignature.texte() !== texteCharge || mefSignature.html() !== htmlCharge) {
            s.signature = mefSignature.texte()
            s.html = mefSignature.enrichi() ? mefSignature.html() : ""
        }
        var nom = champNomSignature.text.trim()
        if (nom.length > 0 && nom !== s.nom)
            s.nom = fenetre.nomLibreSignature(biblio.filter(function(x) { return x !== s }), nom)
    }
    /// Recharge les listes après un changement de la bibliothèque ; les choix
    /// de l'adresse affichée sont ceux qui ont été gardés.
    function rafraichir() {
        biblio = biblio.slice()
        poserChoix()
        choixSignatureEditee.currentIndex = choisie
    }
    /// Montre la signature `k` dans l'éditeur.
    function editer(k) {
        garderSignature()
        choisie = Math.min(k, biblio.length - 1)
        var s = biblio[choisie]
        champNomSignature.text = s ? s.nom : ""
        if (s && s.html.length > 0)
            mefSignature.poserHtml(s.html)
        else
            mefSignature.poserTexte(s ? s.signature : "")
        texteCharge = mefSignature.texte()
        htmlCharge = mefSignature.html()
        champSignature.cursorPosition = 0
        rafraichir()
    }
    /// Scénarios d'essai : nom et texte de la signature éditée ; choix d'une
    /// signature pour l'adresse affichée.
    function essaiSignature(nom, texte) {
        champNomSignature.text = nom
        mefSignature.poserTexte(texte)
        garderSignature()
        rafraichir()
    }
    function essaiChoix(champ, id) {
        changerAdresse(champ, id)
        poserChoix()
    }
    function nouvelle(modele) {
        garderSignature()
        var nom = modele ? modele.nom + qsTr(" (copie)") : qsTr("Nouvelle signature")
        var b = biblio.slice()
        b.push({ id: fenetre.nouvelIdSignature(b), nom: fenetre.nomLibreSignature(b, nom),
                 signature: modele ? modele.signature : "", html: modele ? modele.html : "",
                 origine: modele ? modele.origine : undefined })
        biblio = b
        choisie = -1
        editer(biblio.length - 1)
        champNomSignature.forceActiveFocus()
        champNomSignature.selectAll()
    }
    function supprimer() {
        if (choisie < 0)
            return
        var b = biblio.slice()
        var parti = b.splice(choisie, 1)[0].id
        for (var a in travail) {
            if (travail[a].sigNouveaux === parti)
                travail[a].sigNouveaux = ""
            if (travail[a].sigReponses === parti)
                travail[a].sigReponses = ""
        }
        biblio = b
        var k = choisie
        choisie = -1
        if (biblio.length === 0)
            biblio = [{ id: fenetre.nouvelIdSignature([]), nom: qsTr("Signature"), signature: "", html: "", provisoire: true }]
        editer(Math.min(k, biblio.length - 1))
    }
    /// Dossier des images de la signature éditée, sous `signatures/`.
    function cleSignature() {
        var s = biblio[choisie]
        return s ? "signature-" + s.id : adresse()
    }
    onAccepted: {
        garderSignature()
        // La signature provisoire d'une bibliothèque vide : gardée si on l'a
        // écrite, et choisie alors pour l'adresse affichée qui n'en avait pas.
        var b = biblio.filter(function(s) {
            return !s.provisoire || s.signature.trim().length > 0 || s.html.indexOf("<img") >= 0
        })
        var t = travail
        b.forEach(function(s) {
            if (!s.provisoire)
                return
            delete s.provisoire
            var c = t[adresseCourante]
            if (c && !c.sigNouveaux && !c.sigReponses) {
                c.sigNouveaux = s.id
                c.sigReponses = s.id
            }
        })
        var modifiees = {}
        var aEcrire = JSON.stringify(b) !== biblioChargee
        for (var a in t) {
            if (JSON.stringify(t[a]) !== chargees[a]) {
                modifiees[a] = t[a]
                aEcrire = true
            }
        }
        if (aEcrire)
            fenetre.poserSignatures(b, modifiees)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        ComboBox {
            id: choixIdentite
            Layout.fillWidth: true
            model: fenetre.expediteursConnus
            displayText: currentIndex >= 0 && fenetre.expediteursConnus[currentIndex]
                         ? fenetre.expediteursConnus[currentIndex].adresse : ""
            delegate: ItemDelegate {
                required property var modelData
                width: ListView.view ? ListView.view.width : implicitWidth
                text: modelData.adresse
            }
            onActivated: dlgIdentite.charger()
        }
        Label { text: qsTr("Nom affiché chez les destinataires :") }
        TextField {
            id: champNomAffiche
            MenuSaisie {}
            Layout.fillWidth: true
            placeholderText: qsTr("Prénom Nom")
            onTextEdited: dlgIdentite.changerAdresse("nom", text.trim())
        }
        GridLayout {
            Layout.fillWidth: true
            columns: fenetre.tactile ? 1 : 2
            columnSpacing: 8
            Label { text: qsTr("Signature des nouveaux messages :") }
            ComboBox {
                id: choixNouveaux
                Layout.fillWidth: true
                model: dlgIdentite.choixSignatures
                textRole: "nom"
                onActivated: function(k) { dlgIdentite.changerAdresse("sigNouveaux", dlgIdentite.choixSignatures[k].id) }
            }
            Label { text: qsTr("Signature des réponses et transferts :") }
            ComboBox {
                id: choixReponses
                Layout.fillWidth: true
                model: dlgIdentite.choixSignatures
                textRole: "nom"
                onActivated: function(k) { dlgIdentite.changerAdresse("sigReponses", dlgIdentite.choixSignatures[k].id) }
            }
        }
        MenuSeparator { Layout.fillWidth: true }
        // Bibliothèque (0.6.18) : la signature à modifier, son nom, et ce
        // qu'on en fait ; une signature sert à autant d'adresses qu'on veut.
        Label { text: qsTr("Modifier la signature :") }
        Flow {
            Layout.fillWidth: true
            spacing: 4
            ComboBox {
                id: choixSignatureEditee
                width: Math.min(260, dlgIdentite.availableWidth)
                model: dlgIdentite.biblio
                textRole: "nom"
                onActivated: function(k) { dlgIdentite.editer(k) }
            }
            TextField {
                id: champNomSignature
                MenuSaisie {}
                width: Math.min(220, dlgIdentite.availableWidth)
                placeholderText: qsTr("Nom de la signature")
                onEditingFinished: {
                    dlgIdentite.garderSignature()
                    dlgIdentite.rafraichir()
                }
            }
            ToolButton {
                text: qsTr("Nouvelle")
                focusPolicy: Qt.NoFocus
                onClicked: dlgIdentite.nouvelle(null)
            }
            ToolButton {
                text: qsTr("Dupliquer")
                focusPolicy: Qt.NoFocus
                onClicked: {
                    dlgIdentite.garderSignature()
                    dlgIdentite.nouvelle(dlgIdentite.biblio[dlgIdentite.choisie])
                }
            }
            ToolButton {
                text: qsTr("Supprimer")
                focusPolicy: Qt.NoFocus
                onClicked: dlgIdentite.supprimer()
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: qsTr("Retire la signature de la liste ; les adresses qui l'avaient choisie n'en ont plus. « Annuler » la rend.")
            }
        }
        // Barre de mise en forme de la signature ; les boutons ne prennent
        // pas le focus, la sélection reste celle sur laquelle ils agissent.
        Flow {
            Layout.fillWidth: true
            spacing: 4
            ToolButton {
                text: qsTr("G")
                font.bold: true
                checkable: true
                checked: mefSignature.gras
                focusPolicy: Qt.NoFocus
                onClicked: mefSignature.basculerGras()
            }
            ToolButton {
                text: qsTr("I")
                font.italic: true
                checkable: true
                checked: mefSignature.italique
                focusPolicy: Qt.NoFocus
                onClicked: mefSignature.basculerItalique()
            }
            ToolButton {
                text: qsTr("S")
                font.underline: true
                checkable: true
                checked: mefSignature.souligne
                focusPolicy: Qt.NoFocus
                onClicked: mefSignature.basculerSouligne()
            }
            ToolButton {
                text: qsTr("Lien…")
                focusPolicy: Qt.NoFocus
                onClicked: dlgLienSignature.open()
            }
            ToolButton {
                text: qsTr("Image…")
                focusPolicy: Qt.NoFocus
                onClicked: choixImageSignature.open()
            }
            // Option (0.6.17) : une image animée composée ici, insérée comme
            // les autres ; la signature elle-même ne change pas. Pas en vue
            // tactile : le sélecteur d'Android rend des adresses content:,
            // que le noyau ne lit pas.
            ToolButton {
                text: qsTr("Badge animé…")
                visible: !fenetre.tactile
                focusPolicy: Qt.NoFocus
                onClicked: dlgBadge.ouvrir(dlgIdentite.cleSignature(),
                                           (dlgIdentite.biblio[dlgIdentite.choisie] || {}).origine || dlgIdentite.adresse())
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: qsTr("Une image qui fait défiler en boucle le logo de la société, votre photo et d'autres logos, insérée dans la signature. Facultatif.")
            }
            ToolButton {
                text: qsTr("Importer une signature Outlook…")
                focusPolicy: Qt.NoFocus
                onClicked: choixImportSignature.open()
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: qsTr("Le fichier .htm du dossier %APPDATA%\\Microsoft\\Signatures, avec ses images.")
            }
            ToolButton {
                text: qsTr("Effacer la mise en forme")
                focusPolicy: Qt.NoFocus
                onClicked: mefSignature.effacerMiseEnForme()
            }
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(260, fenetre.height * 0.3)
            TextArea {
                id: champSignature
                MenuSaisie {}
                textFormat: TextEdit.RichText
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                placeholderText: qsTr("Prénom Nom\nFonction — Société\nTéléphone")
                color: "#1f1f1f"
                background: Rectangle { color: "#ffffff"; border.color: fenetre.palette.mid }
                ImagesAnimees {
                    id: animationsSignature
                    zone: champSignature
                    miseEnForme: mefSignature
                }
            }
        }
        MiseEnForme {
            id: mefSignature
            document: champSignature.textDocument
            curseur: champSignature.cursorPosition
            debut: champSignature.selectionStart
            fin: champSignature.selectionEnd
        }
        Label {
            visible: dlgIdentite.note.length > 0
            text: dlgIdentite.note
            color: dlgIdentite.note === dlgIdentite.noteReprise ? dlgIdentite.palette.text : "#b00020"
            opacity: dlgIdentite.note === dlgIdentite.noteReprise ? 0.75 : 1
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
    }

    FileDialog {
        id: choixImageSignature
        title: qsTr("Insérer une image dans la signature")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.bmp *.webp)")]
        onAccepted: {
            var url = boite.imageSignature(dlgIdentite.cleSignature(), selectedFile)
            if (url.length > 0) {
                mefSignature.insererImage(url, 640)
                dlgIdentite.note = ""
            } else {
                dlgIdentite.note = boite.erreur
            }
        }
    }
    FileDialog {
        id: choixImportSignature
        title: qsTr("Importer une signature Outlook")
        nameFilters: [qsTr("Signatures (*.htm *.html)")]
        onAccepted: {
            var html = boite.importerSignature(dlgIdentite.cleSignature(), selectedFile)
            if (html.length > 0) {
                mefSignature.poserHtml(html)
                champSignature.cursorPosition = 0
                dlgIdentite.note = ""
            } else {
                dlgIdentite.note = boite.erreur
            }
        }
    }
    DialogueBadge {
        id: dlgBadge
        onInsere: function(url) {
            mefSignature.insererImage(url, dlgBadge.largeur)
            dlgIdentite.note = ""
        }
    }
    Dialog {
        id: dlgLienSignature
        title: qsTr("Insérer un lien")
        modal: true
        anchors.centerIn: parent
        width: Math.min(520, dlgIdentite.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: {
            champLienSignature.text = "https://"
            champLienSignature.forceActiveFocus()
            champLienSignature.cursorPosition = champLienSignature.text.length
        }
        onAccepted: mefSignature.poserLien(champLienSignature.text)
        TextField {
            id: champLienSignature
            MenuSaisie {}
            anchors.left: parent.left
            anchors.right: parent.right
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            onAccepted: dlgLienSignature.accept()
        }
    }
}
