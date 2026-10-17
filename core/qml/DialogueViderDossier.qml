// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue de confirmation « Vider le dossier ».
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueViderDossier { id: dlgViderDossier } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgViderDossier
    property int compte: 0
    property string chemin: ""
    property string nom: ""
    function ouvrir(c, ch, n) { compte = c; chemin = ch; nom = n; open() }
    title: qsTr("Vider ce dossier ?")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(480, fenetre.width - 24)
    standardButtons: Dialog.Yes | Dialog.No
    RowLayout {
        width: dlgViderDossier.availableWidth
        spacing: 12
        Avertissement {}
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Vider définitivement le dossier « %1 » ? Tous ses messages sont effacés du serveur, sans repasser par une corbeille : l'action est irréversible.").arg(dlgViderDossier.nom)
        }
    }
    onAccepted: {
        fenetre.messagesVides = 0
        boite.viderDossier(dlgViderDossier.compte, dlgViderDossier.chemin)
        messageEtat.texte = qsTr("Vidage de « %1 » en cours…").arg(dlgViderDossier.nom)
    }
}
