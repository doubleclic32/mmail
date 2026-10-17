// SPDX-License-Identifier: GPL-3.0-or-later
// Menu d’une pièce jointe (ouvrir, enregistrer sous…).
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuPiece { id: menuPiece } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuPiece
    font.pointSize: fenetre.tailleColonne("message")
    onAboutToShow: width = fenetre.largeurMenu(menuPiece)
    property var piece: null
    function ouvrir(p, bouton) {
        piece = p
        popup(bouton, 0, bouton.height)
    }
    MenuItem {
        text: qsTr("Ouvrir")
        enabled: menuPiece.piece !== null && !menuPiece.piece.risquee
        onTriggered: fenetre.ouvrirPiece(menuPiece.piece)
    }
    MenuItem {
        text: qsTr("Ouvrir avec…")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        enabled: menuPiece.piece !== null && !menuPiece.piece.risquee
        onTriggered: dlgChoisirLogiciel.choisir(menuPiece.piece, false)
    }
    MenuItem {
        text: qsTr("Enregistrer sous…")
        // Sous Android, le dialogue rend une adresse « content:// » que le
        // noyau ne sait pas écrire : l'ouverture y suffit.
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        onTriggered: fenetre.enregistrerPiece(menuPiece.piece)
    }
    MenuSeparator {}
    // Reprendre une pièce reçue dans un autre courrier (demande de Manu,
    // 01/10) : la rédaction ouverte en dernier, ou une nouvelle.
    MenuItem {
        readonly property var cible: fenetre.redactions[fenetre.redactionActive] || null
        visible: cible !== null
        height: visible ? implicitHeight : 0
        text: cible ? qsTr("Joindre au message en cours « %1 »").arg(cible.objetActuel() || qsTr("sans objet"))
                    : ""
        onTriggered: fenetre.joindrePiece(menuPiece.piece, fenetre.redactionActive)
    }
    MenuItem {
        text: qsTr("Joindre à un nouveau message")
        onTriggered: fenetre.joindrePiece(menuPiece.piece, "nouveau")
    }
    MenuSeparator {
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
    }
    MenuItem {
        text: qsTr("Logiciels d'ouverture…")
        visible: Qt.platform.os !== "android"
        height: visible ? implicitHeight : 0
        onTriggered: dlgLogiciels.open()
    }
}
