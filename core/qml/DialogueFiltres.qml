// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Filtres du courrier » (0.6.0 ; décision 11, tranchée par Manu le
// 18/10/2026) : des règles appliquées par MMail lui-même aux messages qui
// arrivent dans la boîte de réception — sur tous les comptes —, ou à toute la
// boîte sur demande. La liste part avec la sauvegarde de la configuration.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml, où « DialogueFiltres { id: dlgFiltres } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: dlgFiltres
    title: qsTr("Filtres du courrier")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(760, fenetre.width - 16)
    height: Math.min(680, fenetre.height - 24)
    padding: 10

    property var liste: []
    property int choisi: -1
    property var cibles: []
    property string message: ""
    readonly property var filtre: choisi >= 0 && choisi < liste.length ? liste[choisi] : null
    readonly property var adresses: fenetre.comptesConnus.map(function(c) { return c.adresse })

    readonly property var champs: ["expediteur", "objet"]
    readonly property var nomsChamps: [qsTr("Expéditeur"), qsTr("Objet")]
    readonly property var operateurs: ["contient", "est", "commence", "finit"]
    readonly property var nomsOperateurs: [qsTr("contient"), qsTr("est"), qsTr("commence par"), qsTr("se termine par")]
    readonly property var destinations: ["", "dossier", "corbeille", "indesirables"]
    readonly property var nomsDestinations: [qsTr("le laisser où il est"), qsTr("le déplacer vers un dossier"),
                                             qsTr("le mettre à la corbeille"), qsTr("le classer indésirable")]

    /// Ouvre la liste ; `nouveau` (facultatif) : un filtre à ajouter, déjà
    /// rempli — « Créer un filtre… » sur un message.
    function ouvrir(nouveau) {
        cibles = JSON.parse(boite.dossiersCibles())
        var l = []
        try { l = JSON.parse(boite.filtres() || "[]") } catch (e) { l = [] }
        if (nouveau)
            l.push(nouveau)
        liste = l
        choisi = nouveau ? l.length - 1 : (l.length > 0 ? 0 : -1)
        message = ""
        open()
    }
    function vierge() {
        return { nom: qsTr("Nouveau filtre"), actif: true, compte: "", tout: true,
                 conditions: [{ champ: "expediteur", operateur: "contient", valeur: "" }],
                 lu: false, suivi: false, destination: "", dossier: "", arreter: false }
    }
    function modifier(cle, valeur) {
        if (!filtre)
            return
        var l = liste.slice()
        var f = Object.assign({}, l[choisi])
        f[cle] = valeur
        l[choisi] = f
        liste = l
    }
    function modifierCondition(i, cle, valeur) {
        var conditions = filtre.conditions.slice()
        var c = Object.assign({}, conditions[i])
        c[cle] = valeur
        conditions[i] = c
        modifier("conditions", conditions)
    }
    function deplacer(sens) {
        var j = choisi + sens
        if (choisi < 0 || j < 0 || j >= liste.length)
            return
        var l = liste.slice()
        var f = l[choisi]
        l[choisi] = l[j]
        l[j] = f
        liste = l
        choisi = j
    }
    function resume(f) {
        var actions = []
        if (f.lu)
            actions.push(qsTr("lu"))
        if (f.suivi)
            actions.push(qsTr("suivi"))
        if (f.destination === "dossier")
            actions.push("→ " + (f.dossier || "?"))
        else if (f.destination === "corbeille")
            actions.push("→ " + qsTr("corbeille"))
        else if (f.destination === "indesirables")
            actions.push("→ " + qsTr("indésirables"))
        return (f.compte || qsTr("tous les comptes")) + " · " + actions.join(", ")
               + (f.actif ? "" : " · " + qsTr("désactivé"))
    }
    function dossiersDuCompte(adresse) {
        return cibles.filter(function(c) { return c.adresse === adresse })
    }
    function enregistrer() {
        dlgFiltres.forceActiveFocus()
        if (boite.poserFiltres(JSON.stringify(liste))) {
            message = qsTr("Filtres enregistrés.")
            return true
        }
        message = boite.erreur
        return false
    }
    function appliquer() {
        if (!enregistrer())
            return
        var touches = 0
        for (var i = 0; i < fenetre.comptesConnus.length; ++i)
            touches += boite.appliquerFiltres(fenetre.comptesConnus[i].compte)
        message = touches > 0
                ? qsTr("Filtres appliqués : %1.").arg(fenetre.accord(touches, qsTr("message touché"), qsTr("messages touchés")))
                : qsTr("Aucun message des boîtes de réception ne répond aux filtres.")
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        // ------------------------------------------------- la liste
        Frame {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(150, Math.max(64, vueFiltres.contentHeight + 12))
            padding: 2
            ListView {
                id: vueFiltres
                anchors.fill: parent
                clip: true
                model: dlgFiltres.liste
                currentIndex: dlgFiltres.choisi
                ScrollBar.vertical: ScrollBar {}
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    highlighted: index === dlgFiltres.choisi
                    onClicked: dlgFiltres.choisi = index
                    contentItem: ColumnLayout {
                        spacing: 0
                        Label {
                            text: modelData.nom || qsTr("(sans nom)")
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                            opacity: modelData.actif ? 1 : 0.55
                        }
                        Label {
                            text: dlgFiltres.resume(modelData)
                            elide: Text.ElideRight
                            font.pointSize: dlgFiltres.font.pointSize * 0.9
                            opacity: 0.75
                            Layout.fillWidth: true
                        }
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: dlgFiltres.liste.length === 0
                    text: qsTr("Aucun filtre.")
                    opacity: 0.6
                }
            }
        }
        Flow {
            Layout.fillWidth: true
            spacing: 4
            Button {
                text: qsTr("Ajouter")
                onClicked: {
                    dlgFiltres.liste = dlgFiltres.liste.concat([dlgFiltres.vierge()])
                    dlgFiltres.choisi = dlgFiltres.liste.length - 1
                }
            }
            Button {
                text: qsTr("Supprimer")
                enabled: dlgFiltres.filtre !== null
                onClicked: {
                    var l = dlgFiltres.liste.slice()
                    l.splice(dlgFiltres.choisi, 1)
                    dlgFiltres.liste = l
                    dlgFiltres.choisi = Math.min(dlgFiltres.choisi, l.length - 1)
                }
            }
            Button {
                text: qsTr("Monter")
                enabled: dlgFiltres.choisi > 0
                onClicked: dlgFiltres.deplacer(-1)
            }
            Button {
                text: qsTr("Descendre")
                enabled: dlgFiltres.choisi >= 0 && dlgFiltres.choisi < dlgFiltres.liste.length - 1
                onClicked: dlgFiltres.deplacer(1)
            }
        }

        // ------------------------------------------------- le filtre choisi
        ScrollView {
            id: defilement
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: dlgFiltres.filtre !== null
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: defilement.availableWidth
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    TextField {
                        id: champNom
                        Layout.fillWidth: true
                        placeholderText: qsTr("Nom du filtre")
                        text: dlgFiltres.filtre ? dlgFiltres.filtre.nom : ""
                        onEditingFinished: dlgFiltres.modifier("nom", text)
                    }
                    CheckBox {
                        text: qsTr("Actif")
                        checked: dlgFiltres.filtre ? dlgFiltres.filtre.actif : false
                        onToggled: dlgFiltres.modifier("actif", checked)
                    }
                }
                Label { text: qsTr("Pour les messages qui arrivent dans la boîte de réception de") }
                ComboBox {
                    Layout.fillWidth: true
                    model: [qsTr("tous les comptes")].concat(dlgFiltres.adresses)
                    currentIndex: dlgFiltres.filtre ? dlgFiltres.adresses.indexOf(dlgFiltres.filtre.compte) + 1 : 0
                    onActivated: function(i) { dlgFiltres.modifier("compte", i === 0 ? "" : dlgFiltres.adresses[i - 1]) }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label { text: qsTr("et qui remplissent") }
                    ComboBox {
                        Layout.fillWidth: true
                        model: [qsTr("toutes les conditions"), qsTr("une condition au moins")]
                        currentIndex: dlgFiltres.filtre && !dlgFiltres.filtre.tout ? 1 : 0
                        onActivated: function(i) { dlgFiltres.modifier("tout", i === 0) }
                    }
                }
                Repeater {
                    model: dlgFiltres.filtre ? dlgFiltres.filtre.conditions : []
                    delegate: GridLayout {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        columns: fenetre.compact ? 2 : 4
                        ComboBox {
                            model: dlgFiltres.nomsChamps
                            currentIndex: Math.max(0, dlgFiltres.champs.indexOf(modelData.champ))
                            onActivated: function(i) { dlgFiltres.modifierCondition(index, "champ", dlgFiltres.champs[i]) }
                            Layout.preferredWidth: 140
                        }
                        ComboBox {
                            model: dlgFiltres.nomsOperateurs
                            currentIndex: Math.max(0, dlgFiltres.operateurs.indexOf(modelData.operateur))
                            onActivated: function(i) { dlgFiltres.modifierCondition(index, "operateur", dlgFiltres.operateurs[i]) }
                            Layout.preferredWidth: 160
                            Layout.fillWidth: fenetre.compact
                        }
                        TextField {
                            Layout.fillWidth: true
                            placeholderText: qsTr("texte")
                            text: modelData.valeur
                            onEditingFinished: dlgFiltres.modifierCondition(index, "valeur", text)
                        }
                        Button {
                            text: qsTr("Retirer")
                            enabled: dlgFiltres.filtre && dlgFiltres.filtre.conditions.length > 1
                            onClicked: {
                                var conditions = dlgFiltres.filtre.conditions.slice()
                                conditions.splice(index, 1)
                                dlgFiltres.modifier("conditions", conditions)
                            }
                        }
                    }
                }
                Button {
                    text: qsTr("Ajouter une condition")
                    onClicked: dlgFiltres.modifier("conditions", dlgFiltres.filtre.conditions.concat(
                                                       [{ champ: "objet", operateur: "contient", valeur: "" }]))
                }
                Label { text: qsTr("Alors :") ; font.bold: true }
                CheckBox {
                    text: qsTr("le marquer comme lu")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.lu : false
                    onToggled: dlgFiltres.modifier("lu", checked)
                }
                CheckBox {
                    text: qsTr("le suivre (drapeau)")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.suivi : false
                    onToggled: dlgFiltres.modifier("suivi", checked)
                }
                ComboBox {
                    Layout.fillWidth: true
                    model: dlgFiltres.nomsDestinations
                    currentIndex: dlgFiltres.filtre ? Math.max(0, dlgFiltres.destinations.indexOf(dlgFiltres.filtre.destination)) : 0
                    onActivated: function(i) { dlgFiltres.modifier("destination", dlgFiltres.destinations[i]) }
                }
                ComboBox {
                    id: choixDossier
                    readonly property var dossiers: dlgFiltres.filtre ? dlgFiltres.dossiersDuCompte(dlgFiltres.filtre.compte) : []
                    visible: dlgFiltres.filtre !== null && dlgFiltres.filtre.destination === "dossier" && dlgFiltres.filtre.compte !== ""
                    Layout.fillWidth: true
                    model: dossiers.map(function(d) { return fenetre.libelleLigne(d) + "  (" + d.chemin + ")" })
                    currentIndex: dlgFiltres.filtre ? dossiers.map(function(d) { return d.chemin }).indexOf(dlgFiltres.filtre.dossier) : -1
                    displayText: currentIndex < 0 ? qsTr("Choisir le dossier…") : currentText
                    onActivated: function(i) { dlgFiltres.modifier("dossier", dossiers[i].chemin) }
                }
                Label {
                    visible: dlgFiltres.filtre !== null && dlgFiltres.filtre.destination === "dossier" && dlgFiltres.filtre.compte === ""
                    text: qsTr("Pour déplacer vers un dossier, choisissez d'abord le compte.")
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    opacity: 0.75
                }
                CheckBox {
                    text: qsTr("ne pas passer aux filtres suivants")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.arreter : false
                    onToggled: dlgFiltres.modifier("arreter", checked)
                }
            }
        }

        // ------------------------------------------------- en bas
        Label {
            Layout.fillWidth: true
            visible: dlgFiltres.message.length > 0
            text: dlgFiltres.message
            wrapMode: Text.Wrap
            font.bold: true
        }
        Label {
            Layout.fillWidth: true
            text: qsTr("Les filtres agissent sur les messages qui arrivent pendant qu'un MMail est ouvert. Pour un tri permanent sur une boîte du serveur de MMEDIA, le webmail a aussi ses filtres.")
            wrapMode: Text.Wrap
            font.pointSize: dlgFiltres.font.pointSize * 0.9
            opacity: 0.7
        }
        Flow {
            Layout.fillWidth: true
            spacing: 4
            Button {
                text: qsTr("Appliquer aux boîtes de réception")
                enabled: dlgFiltres.liste.length > 0
                onClicked: dlgFiltres.appliquer()
            }
            Button {
                text: qsTr("Enregistrer")
                highlighted: true
                onClicked: dlgFiltres.enregistrer()
            }
            Button {
                text: qsTr("Fermer")
                onClicked: dlgFiltres.close()
            }
        }
    }
}
