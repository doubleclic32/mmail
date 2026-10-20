// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Nom et signature » d’un compte, ou de l’une de ses autres
// adresses d’envoi (0.6.7).
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
    // Ce qui a été chargé, pour une adresse d'envoi qui reprend l'identité de
    // son compte : inchangé, rien ne lui est écrit, et elle continue de la
    // suivre.
    property string chargee: ""
    // Avertissement de cette reprise : une information, pas une erreur.
    property string noteReprise: ""
    function ouvrir(compte) {
        var i = 0
        var liste = fenetre.expediteursConnus
        for (var k = liste.length - 1; k >= 0; --k)
            if (liste[k].compte === compte)
                i = k
        choixIdentite.currentIndex = i
        charger()
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
    function valeurs() {
        return { nom: champNomAffiche.text.trim(), signature: mefSignature.texte(),
                 html: mefSignature.enrichi() ? mefSignature.html() : "",
                 nouveaux: caseSignatureNouveaux.checked, reponses: caseSignatureReponses.checked }
    }
    function charger() {
        var e = expediteur()
        var i = fenetre.identiteExpediteur(e)
        champNomAffiche.text = i.nom
        if (i.html.length > 0)
            mefSignature.poserHtml(i.html)
        else
            mefSignature.poserTexte(i.signature)
        champSignature.cursorPosition = 0
        caseSignatureNouveaux.checked = i.nouveaux
        caseSignatureReponses.checked = i.reponses
        var reprise = e && e.adresse !== e.compteAdresse && !fenetre.aUneIdentite(e.adresse)
        chargee = reprise ? JSON.stringify(valeurs()) : ""
        noteReprise = reprise ? qsTr("Cette adresse reprend le nom et la signature de %1 ; modifiés ici, ils lui deviennent propres.").arg(e.compteAdresse) : ""
        note = noteReprise
    }
    // Deux versions : le texte, pour les messages sans mise en forme, et
    // le HTML s'il y a de quoi — images, gras, liens.
    onAccepted: {
        var v = valeurs()
        if (chargee.length > 0 && JSON.stringify(v) === chargee)
            return
        fenetre.poserIdentite(adresse(), v)
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
            Layout.fillWidth: true
            placeholderText: qsTr("Prénom Nom")
        }
        Label { text: qsTr("Signature :") }
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
            Layout.preferredHeight: Math.min(320, fenetre.height * 0.4)
            TextArea {
                id: champSignature
                textFormat: TextEdit.RichText
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                placeholderText: qsTr("Prénom Nom\nFonction — Société\nTéléphone")
                color: "#1f1f1f"
                background: Rectangle { color: "#ffffff"; border.color: fenetre.palette.mid }
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
        CheckBox {
            id: caseSignatureNouveaux
            text: qsTr("L'ajouter aux nouveaux messages")
        }
        CheckBox {
            id: caseSignatureReponses
            text: qsTr("L'ajouter aux réponses et aux transferts")
        }
    }

    FileDialog {
        id: choixImageSignature
        title: qsTr("Insérer une image dans la signature")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.bmp *.webp)")]
        onAccepted: {
            var url = boite.imageSignature(dlgIdentite.adresse(), selectedFile)
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
            var html = boite.importerSignature(dlgIdentite.adresse(), selectedFile)
            if (html.length > 0) {
                mefSignature.poserHtml(html)
                champSignature.cursorPosition = 0
                dlgIdentite.note = ""
            } else {
                dlgIdentite.note = boite.erreur
            }
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
            anchors.left: parent.left
            anchors.right: parent.right
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            onAccepted: dlgLienSignature.accept()
        }
    }
}
