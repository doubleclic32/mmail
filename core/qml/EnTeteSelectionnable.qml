// SPDX-License-Identifier: GPL-3.0-or-later
// Texte sélectionnable de l’en-tête du message (objet, expéditeur).
//
// Composant en ligne de Main.qml jusqu'à la 0.5.16 : sorti pour servir aussi
// aux fichiers découpés, un composant en ligne n'étant visible que du sien.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

/// Ligne d'en-tête du message affiché (objet, expéditeur) : un texte
/// sélectionnable, sans cadre, aux couleurs d'un Label.
TextEdit {
    readOnly: true
    selectByMouse: true
    textFormat: TextEdit.PlainText
    wrapMode: TextEdit.Wrap
    color: fenetre.palette.windowText
    selectionColor: fenetre.palette.highlight
    selectedTextColor: fenetre.palette.highlightedText
    font.pointSize: fenetre.tailleMessage
    // Comme dans le corps : un Ctrl+C explicite passe par la règle du
    // presse-papier, qui sait ainsi que le dépôt vient de MMail.
    Keys.onPressed: function(touche) {
        if (touche.matches(StandardKey.Copy) && selectedText.length > 0) {
            fenetre.copierTexte(selectedText, "message")
            touche.accepted = true
        }
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
        onTapped: {
            menuEnTete.texteChoisi = parent.selectedText
            menuEnTete.popup()
        }
    }
}
