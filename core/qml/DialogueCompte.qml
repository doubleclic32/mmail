// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue d’ajout ou de modification d’un compte (adresse, serveurs, mot de passe).
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueCompte { id: dlgCompte } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// ----------------------------------------------------------- dialogues
// Trois usages : ajouter un compte, redemander un mot de passe, ou
// modifier un compte présent (`modification`) — serveurs, mot de passe,
// et, de là, nom et signature.
Dialog {
    id: dlgCompte
    // Désignés hors de ce fichier (Main.qml, fichiers voisins) : dlgCompte.<nom>.
    property alias noteCompte: noteCompte
    property alias champAdresse: champAdresse
    property alias champHote: champHote
    property alias champHoteEnvoi: champHoteEnvoi
    property alias champMotDePasse: champMotDePasse
    property alias caseMemoriser: caseMemoriser
    property alias boutonReconnexion: boutonReconnexion
    property int compteId: 0
    property bool modification: false
    // Compte à jetons en modification — « microsoft » ou « google » : ni
    // serveur ni mot de passe à saisir, la connexion se refait chez le
    // service. Vide pour un compte à mot de passe.
    property string service: ""
    readonly property bool microsoft: service === "microsoft"
    readonly property bool oauth: service.length > 0
    // Serveurs du compte à l'ouverture : ce qui a changé se compare à eux.
    property string ancienHote: ""
    property string ancienHoteEnvoi: ""
    title: modification ? qsTr("Modifier le compte")
                        : compteId > 0 ? qsTr("Mot de passe du compte") : qsTr("Ajouter un compte")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(460, fenetre.width - 24)
    standardButtons: Dialog.Ok | Dialog.Cancel
    // Vrai dès que la personne a touché au serveur : la configuration
    // automatique ne réécrit plus ce champ.
    property bool hoteSaisi: false
    property string infoServeur: ""
    onAccepted: fenetre.validerCompte()
    onRejected: fenetre.demandeSuivante()
    onOpened: (dlgCompte.oauth ? boutonReconnexion : dlgCompte.modification ? champHote
               : champAdresse.text.length > 0 ? champMotDePasse : champAdresse).forceActiveFocus()

    GridLayout {
        anchors.fill: parent
        columns: 2
        columnSpacing: 8
        rowSpacing: 6

        Label {
            id: noteCompte
            Layout.columnSpan: 2
            Layout.fillWidth: true
            visible: text.length > 0
            wrapMode: Text.Wrap
            color: "#b00020"
        }
        Label { text: qsTr("Adresse :") }
        TextField {
            id: champAdresse
            placeholderText: "nom@exemple.fr"
            enabled: dlgCompte.compteId === 0
            inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
            Layout.fillWidth: true
            onEditingFinished: fenetre.chercherServeur()
        }
        Label { text: qsTr("Serveur IMAP :"); visible: !dlgCompte.oauth }
        TextField {
            id: champHote
            visible: !dlgCompte.oauth
            placeholderText: "mail.exemple.fr"
            enabled: dlgCompte.compteId === 0 || dlgCompte.modification
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            Layout.fillWidth: true
            onTextEdited: dlgCompte.hoteSaisi = text.length > 0
        }
        Label {
            Layout.columnSpan: 2
            Layout.fillWidth: true
            visible: text.length > 0 && !dlgCompte.oauth
            wrapMode: Text.Wrap
            opacity: 0.75
            font.pixelSize: 11
            text: dlgCompte.infoServeur
        }
        // Facultatif : la plupart des serveurs reçoivent l'envoi sur le
        // nom de la lecture ; Orange, Free, SFR ont un serveur à part.
        Label { text: qsTr("Serveur d'envoi :"); visible: !dlgCompte.oauth }
        TextField {
            id: champHoteEnvoi
            visible: !dlgCompte.oauth
            placeholderText: qsTr("identique au serveur IMAP")
            enabled: dlgCompte.compteId === 0 || dlgCompte.modification
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            Layout.fillWidth: true
        }
        Label { text: qsTr("Mot de passe :"); visible: !dlgCompte.oauth }
        TextField {
            id: champMotDePasse
            visible: !dlgCompte.oauth
            echoMode: TextInput.Password
            // Vide : le mot de passe ne change pas.
            placeholderText: dlgCompte.modification ? qsTr("inchangé") : ""
            Layout.fillWidth: true
            onAccepted: dlgCompte.accept()
        }
        CheckBox {
            id: caseMemoriser
            Layout.columnSpan: 2
            visible: !dlgCompte.oauth
            checked: reglages.memoriser
            text: qsTr("Mémoriser le mot de passe dans le coffre du système")
        }
        // Office 365 n'accepte plus de mot de passe : la connexion se fait
        // chez Microsoft, dans le navigateur.
        Button {
            Layout.columnSpan: 2
            visible: dlgCompte.compteId === 0 && boite.microsoftDisponible()
            text: qsTr("Compte Microsoft (Office 365, Outlook.com)…")
            onClicked: {
                var adresse = champAdresse.text
                dlgCompte.close()
                fenetre.ajouterMicrosoft(adresse)
            }
        }
        // Gmail : la connexion se fait chez Google, dans le navigateur.
        Button {
            Layout.columnSpan: 2
            visible: dlgCompte.compteId === 0 && boite.googleAjoutOuvert()
            text: qsTr("Compte Google (Gmail)…")
            onClicked: {
                dlgCompte.close()
                fenetre.connecterGoogle("")
            }
        }
        Button {
            id: boutonReconnexion
            Layout.columnSpan: 2
            visible: dlgCompte.oauth
            text: dlgCompte.service === "google" ? qsTr("Se reconnecter à Google…")
                                                 : qsTr("Se reconnecter à Microsoft…")
            onClicked: {
                var adresse = champAdresse.text
                var service = dlgCompte.service
                dlgCompte.close()
                if (service === "google")
                    fenetre.connecterGoogle(adresse)
                else
                    fenetre.connecterMicrosoft(adresse)
            }
        }
        Button {
            Layout.columnSpan: 2
            visible: dlgCompte.modification
            text: qsTr("Nom et signature…")
            onClicked: {
                var id = dlgCompte.compteId
                dlgCompte.close()
                dlgIdentite.ouvrir(id)
            }
        }
        Label {
            Layout.columnSpan: 2
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.75
            font.pixelSize: 11
            text: dlgCompte.microsoft
                  ? qsTr("Compte Microsoft : la connexion se fait chez Microsoft, dans votre navigateur ; MMail ne voit jamais le mot de passe. Il garde, dans le coffre du système, le jeton que Microsoft lui délivre. Lecture en IMAPS (outlook.office365.com), envoi par Microsoft Graph.")
                  : dlgCompte.service === "google"
                  ? qsTr("Compte Google : la connexion se fait chez Google, dans votre navigateur ; MMail ne voit jamais le mot de passe. Il garde, dans le coffre du système, le jeton que Google lui délivre. Lecture en IMAPS (imap.gmail.com), envoi en SMTP sur TLS (smtp.gmail.com).")
                  : qsTr("Le mot de passe n'est jamais écrit par MMail : il est confié au coffre du système (Gestionnaire d'identifiants sous Windows, Secret Service ou KWallet sous Linux, Keystore sous Android). Connexion en IMAPS, port 993 ; envoi en SMTP sur TLS, port 465.")
        }
    }
}
