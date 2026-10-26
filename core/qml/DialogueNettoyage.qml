// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Fiches vides et doublons » (0.6.20) : par carnet d'adresses
// modifiable, les fiches sans rien, celles qui n'ont qu'un nom, et les
// doublons (même adresse de courriel ; même nom et même numéro, ou sans
// adresse). On coche ce qui part ; un doublon se fusionne dans la fiche la
// plus complète avant que les autres soient retirées. Rien ne part sans
// confirmation, et une copie des fiches d'avant reste sur le poste.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueNettoyage { id: dlgNettoyage } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgNettoyage
    // Lignes affichées : carnet, titre de rubrique, fiche à retirer, groupe
    // de doublons à fusionner.
    property var lignes: []
    // Cochées, par clé (« r<id> » : fiche à retirer ; « f<id> » : groupe dont
    // la fiche gardée est <id>).
    property var coches: ({})
    property int version: 0
    property bool attente: false
    property string bilan: ""
    property bool rien: false

    function ouvrir() {
        bilan = ""
        construire()
        open()
    }

    function construire() {
        var carnets = JSON.parse(boite.nettoyageCarnets())
        var l = [], c = {}
        for (var i = 0; i < carnets.length; ++i) {
            var k = carnets[i]
            if (k.vides.length + k.sansCoordonnee.length + k.doublons.length === 0)
                continue
            var actif = k.modifiable
            l.push({ genre: "carnet", texte: qsTr("Carnet « %1 » de %2 : %3 fiches").arg(k.nom).arg(k.boite).arg(k.fiches)
                     + (actif ? "" : " — " + qsTr("non modifiable ici : %1").arg(k.motif)), actif: actif })
            var rubrique = function(nom, liste, cocher, prefixe, texteDe, idsDe) {
                if (liste.length === 0)
                    return
                var section = prefixe + k.carnet
                l.push({ genre: "titre", texte: nom.arg(liste.length), section: section, actif: actif })
                for (var j = 0; j < liste.length; ++j) {
                    var e = liste[j]
                    var cle = prefixe + (e.id || e.garde.id)
                    l.push({ genre: "choix", cle: cle, section: section, texte: texteDe(e), ids: idsDe(e), actif: actif,
                             fusion: prefixe === "f" })
                    c[cle] = actif && cocher
                }
            }
            rubrique(qsTr("Fiches vides (%1)"), k.vides, true, "r",
                     function(e) { return e.resume }, function(e) { return [e.id] })
            rubrique(qsTr("Un nom, aucune coordonnée (%1) — à vérifier avant de retirer"), k.sansCoordonnee, false, "r",
                     function(e) { return e.resume }, function(e) { return [e.id] })
            rubrique(qsTr("Doublons (%1 groupes)"), k.doublons, true, "f",
                     function(e) {
                         return qsTr("Garder : %1").arg(e.garde.resume)
                                 + "\n" + qsTr("Fusionner et retirer : %1").arg(e.autres.map(function(a) { return a.resume }).join(" ; "))
                                 + "\n" + qsTr("Résultat : %1").arg(e.resultat)
                     },
                     function(e) { return [e.garde.id].concat(e.autres.map(function(a) { return a.id })) })
        }
        lignes = l
        coches = c
        rien = l.length === 0
        version++
    }

    function cocher(cle, oui) {
        coches[cle] = oui
        version++
    }

    function cocherSection(section, oui) {
        for (var i = 0; i < lignes.length; ++i)
            if (lignes[i].genre === "choix" && lignes[i].section === section && lignes[i].actif)
                coches[lignes[i].cle] = oui
        version++
    }

    function plan() {
        var p = { retirer: [], fusions: [], textesRetrait: [], textesFusion: [] }
        for (var i = 0; i < lignes.length; ++i) {
            var e = lignes[i]
            if (e.genre !== "choix" || !coches[e.cle])
                continue
            if (e.fusion) {
                p.fusions.push(e.ids)
                p.textesFusion.push(e.texte.split("\n")[0])
            } else {
                p.retirer.push(e.ids[0])
                p.textesRetrait.push(e.texte)
            }
        }
        return p
    }

    function appliquer(p) {
        attente = true
        bilan = ""
        boite.nettoyerCarnets(JSON.stringify({ retirer: p.retirer, fusions: p.fusions }))
    }

    Connections {
        target: boite
        function onContactsEcrits(ok, message, conflit) {
            if (!dlgNettoyage.attente)
                return
            dlgNettoyage.attente = false
            dlgNettoyage.bilan = message
            dlgNettoyage.construire()
        }
    }

    title: qsTr("Fiches vides et doublons")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(720, fenetre.width - 16)
    height: Math.min(680, fenetre.height - 24)
    padding: 10
    closePolicy: Popup.CloseOnEscape
    footer: DialogButtonBox {
        Button {
            readonly property var p: (dlgNettoyage.version, dlgNettoyage.plan())
            text: dlgNettoyage.attente ? qsTr("Nettoyage en cours…") : qsTr("Retirer et fusionner…")
            enabled: !dlgNettoyage.attente && !boite.contactsOccupe && p.retirer.length + p.fusions.length > 0
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: {
                confirmation.p = p
                confirmation.open()
            }
        }
        Button {
            text: qsTr("Fermer")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }

    Dialog {
        id: confirmation
        property var p: ({ retirer: [], fusions: [], textesRetrait: [], textesFusion: [] })
        title: qsTr("Retirer ces fiches ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, fenetre.width - 24)
        height: Math.min(520, fenetre.height - 40)
        standardButtons: Dialog.Yes | Dialog.No
        ColumnLayout {
            anchors.fill: parent
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Avertissement {}
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("%1 fiche(s) retirée(s) du serveur, %2 groupe(s) de doublons fusionné(s). Une copie des fiches d'avant est gardée sur ce poste.")
                          .arg(confirmation.p.retirer.length).arg(confirmation.p.fusions.length)
                }
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                Label {
                    width: confirmation.availableWidth - 14
                    wrapMode: Text.Wrap
                    text: (confirmation.p.textesRetrait.length > 0
                           ? qsTr("Retirées :") + "\n• " + confirmation.p.textesRetrait.join("\n• ") + "\n\n" : "")
                          + (confirmation.p.textesFusion.length > 0
                             ? qsTr("Fusionnées (fiche gardée) :") + "\n• " + confirmation.p.textesFusion.join("\n• ") : "")
                }
            }
        }
        onAccepted: dlgNettoyage.appliquer(confirmation.p)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            Layout.fillWidth: true
            visible: dlgNettoyage.bilan.length > 0
            wrapMode: Text.Wrap
            text: dlgNettoyage.bilan
            font.bold: true
        }
        Label {
            Layout.fillWidth: true
            visible: dlgNettoyage.rien
            wrapMode: Text.Wrap
            opacity: 0.7
            text: qsTr("Aucune fiche vide ni aucun doublon dans les carnets modifiables.")
        }
        ListView {
            id: vue
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: dlgNettoyage.lignes
            ScrollBar.vertical: ScrollBar {}
            delegate: ColumnLayout {
                id: ligne
                required property var modelData
                width: ListView.view.width - 12
                spacing: 0
                Label {
                    visible: ligne.modelData.genre === "carnet"
                    Layout.fillWidth: true
                    Layout.topMargin: 8
                    wrapMode: Text.Wrap
                    font.bold: true
                    text: ligne.modelData.texte
                }
                RowLayout {
                    visible: ligne.modelData.genre === "titre"
                    Layout.fillWidth: true
                    Layout.topMargin: 4
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: ligne.modelData.texte
                        opacity: 0.85
                    }
                    Button {
                        flat: true
                        enabled: ligne.modelData.actif === true
                        text: qsTr("Tout cocher")
                        onClicked: dlgNettoyage.cocherSection(ligne.modelData.section, true)
                    }
                    Button {
                        flat: true
                        enabled: ligne.modelData.actif === true
                        text: qsTr("Tout décocher")
                        onClicked: dlgNettoyage.cocherSection(ligne.modelData.section, false)
                    }
                }
                CheckBox {
                    visible: ligne.modelData.genre === "choix"
                    Layout.fillWidth: true
                    Layout.leftMargin: 12
                    enabled: ligne.modelData.actif === true
                    checked: dlgNettoyage.version >= 0 && dlgNettoyage.coches[ligne.modelData.cle] === true
                    text: ligne.modelData.texte || ""
                    onToggled: dlgNettoyage.cocher(ligne.modelData.cle, checked)
                    contentItem: Label {
                        leftPadding: parent.indicator.width + parent.spacing
                        text: parent.text
                        wrapMode: Text.Wrap
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }
}
