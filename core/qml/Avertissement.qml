// SPDX-License-Identifier: GPL-3.0-or-later
// Triangle d’avertissement dessiné (dialogues de confirmation).
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

Canvas {
    implicitWidth: 30
    implicitHeight: 30
    Layout.alignment: Qt.AlignTop
    onPaint: {
        var c = getContext("2d")
        c.reset()
        c.fillStyle = "#E6A100"
        c.strokeStyle = "#9A6B00"
        c.lineWidth = 1
        c.beginPath()
        c.moveTo(15, 2); c.lineTo(29, 27); c.lineTo(1, 27); c.closePath()
        c.fill(); c.stroke()
        c.fillStyle = "#1A1A1A"
        c.font = "bold 19px sans-serif"
        c.textAlign = "center"; c.textBaseline = "middle"
        c.fillText("!", 15, 18)
    }
}
