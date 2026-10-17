// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Ouvrir avec… » d’une pièce jointe.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueOuvrirAvec { id: dlgOuvrirAvec } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgOuvrirAvec
    property var piece: null
    property string programme: ""
    readonly property string extension: piece ? fenetre.extensionDe(piece.nom) : ""
    function ouvrir(p, prog, toujours) {
        piece = p
        programme = prog
        toujoursAvec.checked = toujours === true
        open()
    }
    title: qsTr("Ouvrir avec %1").arg(lanceur.nom(programme))
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(480, fenetre.width - 24)
    standardButtons: Dialog.Ok | Dialog.Cancel
    ColumnLayout {
        width: dlgOuvrirAvec.availableWidth
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Ouvrir « %1 » avec %2.").arg(dlgOuvrirAvec.piece ? dlgOuvrirAvec.piece.nom : "")
                  .arg(lanceur.nom(dlgOuvrirAvec.programme))
        }
        CheckBox {
            id: toujoursAvec
            visible: dlgOuvrirAvec.extension.length > 0
            text: qsTr("Toujours ouvrir les fichiers .%1 avec ce logiciel").arg(dlgOuvrirAvec.extension)
        }
    }
    onAccepted: {
        if (toujoursAvec.checked && extension.length > 0)
            fenetre.retenirLogiciel(extension, programme)
        fenetre.ouvrirPiece(piece, programme)
    }
}
