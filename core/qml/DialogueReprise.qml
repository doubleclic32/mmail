// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue de reprise des rédactions gardées sur le poste.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueReprise { id: dlgReprise } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgReprise
    title: qsTr("Rédactions retrouvées")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    Label {
        width: dlgReprise.availableWidth
        wrapMode: Text.Wrap
        text: {
            var liste = fenetre.redactionsRetrouvees
            var lignes = []
            for (var k = 0; k < liste.length; ++k) {
                var c = liste[k].contenu || {}
                var objet = (c.objet || "").length > 0 ? "« " + c.objet + " »" : qsTr("(sans objet)")
                var a = (c.a || "").length > 0 ? qsTr(" à %1").arg(c.a) : ""
                lignes.push("• " + objet + a + qsTr(", modifié le %1").arg(fenetre.dateLongue(liste[k].modifie * 1000)))
            }
            return (liste.length > 1
                    ? qsTr("MMail s'est fermé alors que %1 messages étaient en cours de rédaction :").arg(liste.length)
                    : qsTr("MMail s'est fermé alors qu'un message était en cours de rédaction :"))
                   + "\n" + lignes.join("\n")
        }
    }
    footer: DialogButtonBox {
        Button {
            text: qsTr("Rouvrir")
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        Button {
            text: qsTr("Abandonner")
            DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
        }
        Button {
            text: qsTr("Plus tard")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    onAccepted: fenetre.rouvrirRedactions()
    onDiscarded: {
        for (var k = 0; k < fenetre.redactionsRetrouvees.length; ++k)
            boite.oublierRedactionGardee(fenetre.redactionsRetrouvees[k].id)
        fenetre.redactionsRetrouvees = []
        dlgReprise.close()
    }
}
