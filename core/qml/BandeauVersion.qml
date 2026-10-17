// SPDX-License-Identifier: GPL-3.0-or-later
// Bandeau de nouvelle version et de mise à jour assistée.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « BandeauVersion { id: bandeauVersion } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Nouvelle version publiée : proposée jusqu'à un choix ; puis son
// téléchargement, et l'invitation à redémarrer pour « maintenant ».
Rectangle {
    id: bandeauVersion
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: parent.top
    height: visible ? grilleVersion.implicitHeight + 12 : 0
    visible: fenetre.etatMiseAJour.length > 0
    color: Qt.tint(fenetre.palette.base, fenetre.etatMiseAJour === "echec" ? "#33c42b1c" : "#3388c86a")
    readonly property string etat: fenetre.etatMiseAJour
    readonly property bool installable: fenetre.genreMiseAJour.length > 0
    GridLayout {
        id: grilleVersion
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 6
        anchors.topMargin: 6
        anchors.bottomMargin: 6
        columns: fenetre.compact ? 1 : 2
        columnSpacing: 8
        rowSpacing: 4
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.StyledText
            text: fenetre.texteMiseAJour()
            onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
        }
        RowLayout {
            spacing: 6
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
            ProgressBar {
                visible: bandeauVersion.etat === "telechargement"
                from: 0
                to: 100
                value: fenetre.progressionMiseAJour
                Layout.preferredWidth: 160
            }
            Button {
                visible: bandeauVersion.etat === "proposee" && bandeauVersion.installable
                text: qsTr("Maintenant")
                highlighted: true
                onClicked: fenetre.choisirMiseAJour("maintenant")
            }
            Button {
                visible: bandeauVersion.etat === "proposee" && bandeauVersion.installable
                text: qsTr("À la fermeture")
                onClicked: fenetre.choisirMiseAJour("fermeture")
            }
            Button {
                visible: bandeauVersion.etat === "proposee" && !bandeauVersion.installable
                text: qsTr("Télécharger")
                onClicked: Qt.openUrlExternally(fenetre.pageVersion)
            }
            Button {
                visible: bandeauVersion.etat === "proposee" && !bandeauVersion.installable
                text: qsTr("Plus tard")
                flat: true
                onClicked: fenetre.etatMiseAJour = ""
            }
            Button {
                visible: bandeauVersion.etat === "proposee"
                text: qsTr("Jamais")
                flat: true
                onClicked: fenetre.choisirMiseAJour("jamais")
            }
            Button {
                visible: bandeauVersion.etat === "prete"
                text: qsTr("Redémarrer MMail")
                highlighted: true
                onClicked: fenetre.redemarrerPourMettreAJour()
            }
            Button {
                visible: bandeauVersion.etat === "prete"
                text: qsTr("À la fermeture")
                flat: true
                onClicked: {
                    reglagesMiseAJour.choix = "fermeture"
                    fenetre.etatMiseAJour = ""
                }
            }
            Button {
                visible: bandeauVersion.etat === "echec" && bandeauVersion.installable
                text: qsTr("Réessayer")
                onClicked: fenetre.telechargerMiseAJour()
            }
            Button {
                visible: bandeauVersion.etat === "echec"
                text: qsTr("Page de téléchargement")
                flat: true
                onClicked: Qt.openUrlExternally(fenetre.pageVersion)
            }
            Button {
                visible: bandeauVersion.etat === "echec"
                text: qsTr("Fermer")
                flat: true
                onClicked: fenetre.etatMiseAJour = ""
            }
        }
    }
}
