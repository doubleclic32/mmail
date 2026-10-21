// SPDX-License-Identifier: GPL-3.0-or-later
// Connexion Microsoft dans une page intégrée à MMail, sous Android seulement
// (0.6.0). Retour de Manu du 18/10 : dans le navigateur du système, MMail
// passait en arrière-plan et Android le gelait ; l'écoute locale ne recevait
// jamais la réponse de Microsoft. Ici, MMail reste au premier plan : la page
// suit la connexion jusqu'à l'adresse de retour (http://localhost?code=…),
// l'intercepte sans la charger et la remet au noyau (Boite.terminerMicrosoft).
// Aucune adresse de retour supplémentaire n'est à déclarer chez Microsoft.
//
// N'entre dans le module QML que sous Android (cf. build.rs), où QtWebView
// est lié ; Main.qml la charge par un Loader.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtWebView

Popup {
    id: connexionIntegree
    parent: Overlay.overlay
    x: 0
    y: 0
    width: parent ? parent.width : 400
    height: parent ? parent.height : 800
    modal: true
    closePolicy: Popup.NoAutoClose
    padding: 0

    property bool termine: false
    /// L'adresse de retour, interceptée : à remettre au noyau.
    signal retour(string url)
    /// Fermée par « Annuler », sans retour.
    signal abandon()

    function ouvrir(url) {
        termine = false
        vue.url = url
        open()
    }
    /// « Annuler », ou la touche Retour d'Android : connexion abandonnée.
    function annuler() {
        termine = true
        vue.stop()
        close()
        abandon()
    }
    function verifier(url) {
        if (termine || url.indexOf("http://localhost") !== 0)
            return
        termine = true
        vue.stop()
        close()
        retour(url)
    }

    background: Rectangle { color: palette.window }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 48
            spacing: 4
            ToolButton {
                text: qsTr("Annuler")
                onClicked: connexionIntegree.annuler()
            }
            Label {
                text: qsTr("Connexion à Microsoft")
                font.bold: true
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            BusyIndicator {
                running: vue.loading
                Layout.preferredHeight: 28
                Layout.preferredWidth: 28
            }
        }
        WebView {
            id: vue
            Layout.fillWidth: true
            Layout.fillHeight: true
            onUrlChanged: connexionIntegree.verifier(url.toString())
            onLoadingChanged: function(requete) { connexionIntegree.verifier(requete.url.toString()) }
        }
    }
}
