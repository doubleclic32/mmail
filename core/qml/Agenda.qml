// SPDX-License-Identifier: GPL-3.0-or-later
// Agenda : les agendas CalDAV des boîtes, en lecture.
//
// Vues jour, semaine et mois, à la manière d'Outlook : à gauche le mois et
// les agendas à cocher, à droite la vue. Le noyau rend les données toutes
// prêtes (`Boite.vueAgenda`) — occurrences calculées, chevauchements répartis
// en colonnes, textes en français — ; il ne reste ici qu'à les placer. Tout se
// lit dans l'index local : l'agenda reste consultable hors connexion, et une
// synchronisation en arrière-plan le tient à jour.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.settings

Pane {
    id: agenda
    padding: 0

    required property var boite
    property bool compact: false
    // Pour la sauvegarde de la configuration (cf. Main.qml).
    property alias reglagesAgenda: reglagesAgenda
    // « jour », « semaine » ou « mois » ; la date autour de laquelle la vue
    // est construite.
    property string genre: compact ? "jour" : "semaine"
    property date jour: new Date()
    property var donnees: ({})
    property var agendas: []
    property string erreurs: ""
    // Mois montré par le petit calendrier : il se feuillette sans déplacer la
    // vue.
    property date moisMini: new Date()
    readonly property real hauteurHeure: 46
    // Minutes écoulées depuis minuit : la ligne de l'heure courante.
    property int minutesMaintenant: 0

    // Écriture en cours : « saisie » (formulaire ouvert) ou « suppression ».
    property string ecritureEnCours: ""

    // Message passager, montré par la fenêtre principale.
    signal annoncer(string texte, bool erreur)

    // Mise en valeur, donnée par la fenêtre (cf. « accent » et « texteAccent »
    // dans Main.qml) : la couleur de sélection, hors apparence de marque.
    property color accent: palette.highlight
    property color texteAccent: palette.highlight

    readonly property color trait: Qt.rgba(palette.windowText.r, palette.windowText.g, palette.windowText.b, 0.13)
    readonly property color traitLeger: Qt.rgba(palette.windowText.r, palette.windowText.g, palette.windowText.b, 0.06)

    Settings {
        id: reglagesAgenda
        category: "agenda"
        property string genre: ""
        // Dernier agenda choisi pour un nouvel événement.
        property int agendaParDefaut: 0
    }

    function iso(d) {
        return Qt.formatDate(d, "yyyy-MM-dd")
    }

    function dateDe(texte) {
        var p = texte.split("-")
        return new Date(Number(p[0]), Number(p[1]) - 1, Number(p[2]))
    }

    function relire() {
        agendas = JSON.parse(boite.agendas())
        donnees = JSON.parse(boite.vueAgenda(genre, iso(jour)))
    }

    function decaler(sens) {
        var d = new Date(jour.getFullYear(), jour.getMonth(), jour.getDate())
        if (genre === "jour")
            d.setDate(d.getDate() + sens)
        else if (genre === "semaine")
            d.setDate(d.getDate() + 7 * sens)
        else {
            d.setDate(1)
            d.setMonth(d.getMonth() + sens)
        }
        jour = d
    }

    function allerAuJour(texte) {
        jour = dateDe(texte)
        genre = "jour"
    }

    function montrerFiche(indice) {
        if (!donnees.fiches || indice < 0 || indice >= donnees.fiches.length)
            return
        popupFiche.f = donnees.fiches[indice]
        popupFiche.open()
    }

    function deuxChiffres(n) {
        return (n < 10 ? "0" : "") + n
    }

    function isoJour(d) {
        return d.getFullYear() + "-" + deuxChiffres(d.getMonth() + 1) + "-" + deuxChiffres(d.getDate())
    }

    function isoHeure(d) {
        return isoJour(d) + "T" + deuxChiffres(d.getHours()) + ":" + deuxChiffres(d.getMinutes())
    }

    // « 2026-10-07… » → « 07/10/2026 ».
    function jjmmaaaa(texte) {
        if (!texte || texte.length < 10)
            return ""
        return texte.substr(8, 2) + "/" + texte.substr(5, 2) + "/" + texte.substr(0, 4)
    }

    // « 07/10/2026 » et « 09:30 » → Date, ou null si l'un ne se lit pas.
    function lireDate(jour, heure) {
        var j = /^\s*(\d{1,2})\/(\d{1,2})\/(\d{4})\s*$/.exec(jour)
        var h = /^\s*(\d{1,2}):(\d{2})\s*$/.exec(heure)
        if (!j || !h || Number(h[1]) > 23 || Number(h[2]) > 59)
            return null
        var d = new Date(Number(j[3]), Number(j[2]) - 1, Number(j[1]), Number(h[1]), Number(h[2]))
        return d.getDate() === Number(j[1]) && d.getMonth() === Number(j[2]) - 1 ? d : null
    }

    function agendasModifiables() {
        return agendas.filter(function(a) { return a.ecriture })
    }

    // Nouvel événement le jour `texte` (aaaa-mm-jj), à `minutes` depuis
    // minuit — -1 : l'heure pleine qui vient aujourd'hui, 9 h un autre jour.
    function nouvelEvenement(texte, minutes, journee) {
        var modifiables = agendasModifiables()
        if (modifiables.length === 0) {
            annoncer(qsTr("Aucun agenda n'accepte l'écriture."), true)
            return
        }
        var debut = dateDe(texte)
        if (minutes < 0) {
            var maintenant = new Date()
            minutes = isoJour(debut) === isoJour(maintenant) ? Math.min(23 * 60, (maintenant.getHours() + 1) * 60) : 9 * 60
        }
        debut.setHours(Math.floor(minutes / 60), minutes % 60)
        var fin = new Date(debut.getTime() + 60 * 60 * 1000)
        var defaut = modifiables.find(function(a) { return a.id === reglagesAgenda.agendaParDefaut }) || modifiables[0]
        popupSaisie.ouvrir({
            agenda: defaut.id, objet: 0, occurrence: "", titre: "", lieu: "", description: "",
            journee: journee, debut: journee ? isoJour(debut) : isoHeure(debut),
            fin: journee ? isoJour(debut) : isoHeure(fin), repetition: "", jusquAu: "", rappel: 15
        })
    }

    // Modifier ou supprimer : une série demande d'abord laquelle des deux.
    function demander(action, f) {
        popupFiche.close()
        if (action === "modifier" && !f.repete) {
            modifier(f, true)
            return
        }
        popupPortee.action = action
        popupPortee.f = f
        popupPortee.open()
    }

    function modifier(f, serie) {
        var texte = boite.saisieEvenement(f.objet, f.occurrence, serie)
        if (texte.length === 0) {
            annoncer(qsTr("Cet événement ne se lit pas."), true)
            return
        }
        popupSaisie.ouvrir(JSON.parse(texte))
    }

    function supprimer(f, serie) {
        ecritureEnCours = "suppression"
        boite.supprimerEvenement(f.objet, f.occurrence, serie)
    }

    // Réponse de la boîte à une réunion où elle est invitée ; le serveur en
    // avertit l'organisateur.
    function repondre(f, reponse) {
        popupFiche.close()
        ecritureEnCours = "reponse"
        boite.repondreEvenement(f.objet, reponse)
    }

    function libelleReponse(statut) {
        return { "ACCEPTED": qsTr("acceptée"), "DECLINED": qsTr("refusée"), "TENTATIVE": qsTr("provisoire"),
                 "NEEDS-ACTION": qsTr("en attente") }[statut] || statut
    }

    // Le serveur d'agenda de la boîte de cet agenda distribue-t-il les
    // invitations ?
    function invitationsPossibles(idAgenda) {
        var a = agendas.find(function(x) { return x.id === idAgenda })
        return a ? a.invitations === true : false
    }

    // Texte lisible sur une couleur : noir sur une couleur claire, blanc sinon.
    function encre(c) {
        return 0.299 * c.r + 0.587 * c.g + 0.114 * c.b > 0.6 ? "#000000" : "#ffffff"
    }

    function heureCourante() {
        var d = new Date()
        minutesMaintenant = d.getHours() * 60 + d.getMinutes()
    }

    onGenreChanged: {
        reglagesAgenda.genre = genre
        relire()
    }
    onJourChanged: {
        moisMini = jour
        relire()
    }
    onVisibleChanged: {
        if (visible) {
            relire()
            boite.synchroniserAgendas(false)
        }
    }
    Component.onCompleted: {
        if (reglagesAgenda.genre.length > 0)
            genre = reglagesAgenda.genre
        heureCourante()
        relire()
    }

    Timer {
        interval: 60 * 1000
        running: agenda.visible
        repeat: true
        onTriggered: agenda.heureCourante()
    }

    Connections {
        target: agenda.boite
        function onAgendasSynchronises(change, erreurs) {
            agenda.erreurs = erreurs
            if (change)
                agenda.relire()
        }
        function onEvenementEnregistre(ok, message) {
            var enCours = agenda.ecritureEnCours
            agenda.ecritureEnCours = ""
            if (enCours === "saisie" && popupSaisie.opened) {
                popupSaisie.enCours = false
                if (ok) {
                    popupSaisie.close()
                    agenda.annoncer(qsTr("Événement enregistré."), false)
                } else {
                    popupSaisie.erreur = message
                }
            } else if (enCours === "suppression") {
                agenda.annoncer(ok ? qsTr("Événement supprimé.") : qsTr("Suppression impossible : %1").arg(message), !ok)
            } else if (enCours === "reponse") {
                agenda.annoncer(ok ? qsTr("Réponse envoyée à l'organisateur.") : qsTr("Réponse impossible : %1").arg(message), !ok)
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // --- volet gauche : le mois, les agendas ---------------------------
        ColumnLayout {
            visible: !agenda.compact
            Layout.preferredWidth: 236
            Layout.maximumWidth: 236
            Layout.fillHeight: true
            Layout.margins: 8
            spacing: 4

            RowLayout {
                Layout.fillWidth: true
                ToolButton {
                    text: "‹"
                    onClicked: agenda.moisMini = new Date(agenda.moisMini.getFullYear(), agenda.moisMini.getMonth() - 1, 1)
                }
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    font.bold: true
                    text: {
                        var t = Qt.locale("fr_FR").standaloneMonthName(agenda.moisMini.getMonth(), Locale.LongFormat)
                        return t.charAt(0).toUpperCase() + t.slice(1) + " " + agenda.moisMini.getFullYear()
                    }
                }
                ToolButton {
                    text: "›"
                    onClicked: agenda.moisMini = new Date(agenda.moisMini.getFullYear(), agenda.moisMini.getMonth() + 1, 1)
                }
            }
            DayOfWeekRow {
                Layout.fillWidth: true
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property string shortName
                    text: shortName.charAt(0).toUpperCase()
                    horizontalAlignment: Text.AlignHCenter
                    opacity: 0.6
                    font.pixelSize: 11
                }
            }
            MonthGrid {
                id: miniMois
                Layout.fillWidth: true
                month: agenda.moisMini.getMonth()
                year: agenda.moisMini.getFullYear()
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property var model
                    readonly property bool choisi: model.year === agenda.jour.getFullYear()
                                                   && model.month === agenda.jour.getMonth()
                                                   && model.day === agenda.jour.getDate()
                    text: model.day
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                    font.bold: model.today
                    opacity: model.month === miniMois.month ? 1 : 0.4
                    color: model.today ? agenda.texteAccent : agenda.palette.windowText
                    background: Rectangle {
                        radius: height / 2
                        color: parent.choisi ? Qt.rgba(agenda.accent.r, agenda.accent.g,
                                                       agenda.accent.b, 0.22) : "transparent"
                    }
                }
                onClicked: function(date) { agenda.jour = date }
            }

            MenuSeparator { Layout.fillWidth: true }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: listeAgendas
            }

            Label {
                Layout.fillWidth: true
                visible: agenda.erreurs.length > 0
                text: agenda.erreurs
                wrapMode: Text.Wrap
                color: "#b00020"
                font.pixelSize: 11
            }
        }

        Rectangle {
            visible: !agenda.compact
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            color: agenda.trait
        }

        // --- vue -----------------------------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // Sur un écran étroit, le choix de la vue passe sur une seconde
            // ligne : le titre de la période garde la place de se lire.
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 6
                Layout.bottomMargin: agenda.compact ? 0 : 6
                spacing: 4
                Button {
                    visible: agenda.agendasModifiables().length > 0
                    text: agenda.compact ? "+" : qsTr("Nouvel événement")
                    font.bold: true
                    onClicked: agenda.nouvelEvenement(agenda.iso(agenda.jour), -1, false)
                    ToolTip.visible: hovered && agenda.compact
                    ToolTip.text: qsTr("Nouvel événement")
                }
                Button {
                    text: qsTr("Aujourd'hui")
                    onClicked: agenda.jour = new Date()
                }
                ToolButton {
                    text: "‹"
                    font.pixelSize: 18
                    onClicked: agenda.decaler(-1)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Précédent")
                }
                ToolButton {
                    text: "›"
                    font.pixelSize: 18
                    onClicked: agenda.decaler(1)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Suivant")
                }
                Label {
                    Layout.fillWidth: true
                    text: agenda.donnees.titre || ""
                    font.bold: true
                    font.pixelSize: 16
                    elide: Text.ElideRight
                }
                Loader {
                    active: !agenda.compact
                    visible: active
                    sourceComponent: choixGenre
                }
            }
            RowLayout {
                visible: agenda.compact
                Layout.fillWidth: true
                Layout.leftMargin: 6
                Layout.rightMargin: 6
                spacing: 4
                ToolButton {
                    text: qsTr("Agendas")
                    onClicked: popupAgendas.open()
                }
                Item { Layout.fillWidth: true }
                Loader {
                    active: agenda.compact
                    visible: active
                    sourceComponent: choixGenre
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: agenda.trait
            }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: agenda.genre === "mois" ? vueMois : vueGrille
            }

            Label {
                Layout.fillWidth: true
                Layout.margins: 6
                visible: agenda.agendas.length === 0
                wrapMode: Text.Wrap
                opacity: 0.75
                text: agenda.boite.agendaOccupe
                      ? qsTr("Recherche des agendas des boîtes…")
                      : qsTr("Aucun agenda pour l'instant. Ceux des boîtes connectées — Mailcow (SOGo) ou tout serveur CalDAV — apparaissent ici après leur synchronisation ; « Actualiser » la relance.")
            }
        }
    }

    Component {
        id: choixGenre
        RowLayout {
            spacing: 0
            ButtonGroup { id: groupeGenre }
            Repeater {
                model: [["jour", qsTr("Jour")], ["semaine", qsTr("Semaine")], ["mois", qsTr("Mois")]]
                delegate: ToolButton {
                    required property var modelData
                    text: modelData[1]
                    checkable: true
                    checked: agenda.genre === modelData[0]
                    ButtonGroup.group: groupeGenre
                    onClicked: agenda.genre = modelData[0]
                }
            }
        }
    }

    // --- agendas à cocher, groupés par boîte ---------------------------------
    Component {
        id: listeAgendas
        ListView {
            clip: true
            model: agenda.agendas
            spacing: 0
            boundsBehavior: Flickable.StopAtBounds
            // Le nom de la boîte au-dessus de son premier agenda.
            delegate: Column {
                id: ligneAgenda
                required property var modelData
                required property int index
                width: ListView.view.width
                Label {
                    visible: ligneAgenda.index === 0 || agenda.agendas[ligneAgenda.index - 1].boite !== ligneAgenda.modelData.boite
                    width: parent.width
                    text: ligneAgenda.modelData.boite
                    font.bold: true
                    font.pixelSize: 12
                    topPadding: 8
                    bottomPadding: 2
                    elide: Text.ElideRight
                }
                CheckDelegate {
                    width: parent.width
                    padding: 4
                    checked: ligneAgenda.modelData.affiche
                    onToggled: {
                        agenda.boite.afficherAgenda(ligneAgenda.modelData.id, checked)
                        agenda.relire()
                    }
                    contentItem: RowLayout {
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 12
                            Layout.preferredHeight: 12
                            radius: 3
                            color: ligneAgenda.modelData.couleur
                        }
                        Label {
                            Layout.fillWidth: true
                            text: ligneAgenda.modelData.nom
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    Popup {
        id: popupAgendas
        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 32, 360)
        height: Math.min(agenda.height - 64, 420)
        modal: true
        padding: 12
        contentItem: ColumnLayout {
            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: listeAgendas
            }
            Label {
                Layout.fillWidth: true
                visible: agenda.erreurs.length > 0
                text: agenda.erreurs
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Button {
                Layout.alignment: Qt.AlignRight
                text: qsTr("Fermer")
                onClicked: popupAgendas.close()
            }
        }
    }

    // --- vue mois --------------------------------------------------------------
    Component {
        id: vueMois
        ColumnLayout {
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: 7
                    delegate: Label {
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        horizontalAlignment: Text.AlignHCenter
                        padding: 4
                        opacity: 0.7
                        text: agenda.donnees.jours && agenda.donnees.jours.length > index ? agenda.donnees.jours[index].semaine : ""
                    }
                }
            }
            GridLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                columns: 7
                columnSpacing: 0
                rowSpacing: 0
                Repeater {
                    model: agenda.donnees.jours ? agenda.donnees.jours.length : 0
                    delegate: Rectangle {
                        id: caseMois
                        required property int index
                        readonly property var info: agenda.donnees.jours[index]
                        readonly property var entrees: agenda.donnees.entrees ? agenda.donnees.entrees[index] : []
                        // Lignes qui tiennent sous le numéro du jour.
                        readonly property int place: Math.max(1, Math.floor((height - 24) / 18))
                        readonly property int montrees: entrees.length > place ? place - 1 : entrees.length
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: 1
                        Layout.preferredHeight: 1
                        color: info.horsMois ? Qt.darker(agenda.palette.base, 1.04) : agenda.palette.base
                        border.width: 1
                        border.color: agenda.traitLeger

                        MouseArea {
                            anchors.fill: parent
                            onDoubleClicked: agenda.nouvelEvenement(caseMois.info.date, 9 * 60, false)
                        }
                        Rectangle {
                            x: parent.width - width - 4
                            y: 3
                            width: Math.max(20, numero.implicitWidth + 8)
                            height: 20
                            radius: 10
                            color: caseMois.info.aujourdhui ? agenda.accent : "transparent"
                            Label {
                                id: numero
                                anchors.centerIn: parent
                                text: caseMois.info.jour
                                font.bold: caseMois.info.aujourdhui
                                opacity: caseMois.info.horsMois ? 0.45 : 1
                                color: caseMois.info.aujourdhui ? agenda.palette.highlightedText : agenda.palette.windowText
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: agenda.allerAuJour(caseMois.info.date)
                            }
                        }
                        Column {
                            x: 3
                            y: 25
                            width: parent.width - 6
                            spacing: 1
                            Repeater {
                                model: caseMois.montrees
                                delegate: Rectangle {
                                    id: entree
                                    required property int index
                                    readonly property var e: caseMois.entrees[index]
                                    readonly property var f: agenda.donnees.fiches[e.fiche]
                                    readonly property color teinte: f.couleur
                                    width: parent.width
                                    height: 17
                                    radius: 3
                                    color: e.heure.length === 0 ? teinte : "transparent"
                                    Row {
                                        anchors.verticalCenter: parent.verticalCenter
                                        x: 3
                                        width: parent.width - 6
                                        spacing: 4
                                        Rectangle {
                                            visible: entree.e.heure.length > 0
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 7
                                            height: 7
                                            radius: 4
                                            color: entree.teinte
                                        }
                                        Label {
                                            width: parent.width - (entree.e.heure.length > 0 ? 11 : 0)
                                            // Sur un écran étroit, le titre seul : l'heure
                                            // prendrait toute la case.
                                            text: (entree.e.heure.length > 0 && !agenda.compact ? entree.e.heure + " " : "") + entree.f.titre
                                            font.pixelSize: 11
                                            font.strikeout: entree.f.annule
                                            elide: Text.ElideRight
                                            color: entree.e.heure.length === 0 ? agenda.encre(entree.teinte) : agenda.palette.windowText
                                        }
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: agenda.montrerFiche(entree.e.fiche)
                                    }
                                }
                            }
                            Label {
                                visible: caseMois.entrees.length > caseMois.montrees
                                readonly property int reste: caseMois.entrees.length - caseMois.montrees
                                text: reste > 1 ? qsTr("+ %1 autres").arg(reste) : qsTr("+ 1 autre")
                                font.pixelSize: 11
                                color: agenda.texteAccent
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: agenda.allerAuJour(caseMois.info.date)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- vues jour et semaine ----------------------------------------------------
    Component {
        id: vueGrille
        ColumnLayout {
            id: grille
            spacing: 0
            readonly property int nbJours: agenda.donnees.jours ? agenda.donnees.jours.length : 0
            // Colonne des heures, à gauche.
            readonly property real marge: 50
            readonly property real largeurJour: nbJours > 0 ? (width - marge - 12) / nbJours : 0
            readonly property real hh: agenda.hauteurHeure

            // En-têtes des jours.
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                Repeater {
                    model: grille.nbJours
                    delegate: Label {
                        required property int index
                        readonly property var info: agenda.donnees.jours[index]
                        x: grille.marge + index * grille.largeurJour
                        width: grille.largeurJour
                        height: parent.height
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        text: info.semaine + " " + info.jour
                        font.bold: info.aujourdhui
                        color: info.aujourdhui ? agenda.texteAccent : agenda.palette.windowText
                        MouseArea {
                            anchors.fill: parent
                            enabled: grille.nbJours > 1
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: agenda.allerAuJour(parent.info.date)
                        }
                    }
                }
            }

            // Bandeau des journées entières et des événements de plus d'un jour.
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: (agenda.donnees.lignes || 0) * 22 + ((agenda.donnees.lignes || 0) > 0 ? 4 : 0)
                visible: (agenda.donnees.lignes || 0) > 0
                Repeater {
                    model: agenda.donnees.bandes || []
                    delegate: Rectangle {
                        id: bande
                        required property var modelData
                        readonly property var f: agenda.donnees.fiches[modelData.fiche]
                        readonly property color teinte: f.couleur
                        x: grille.marge + modelData.de * grille.largeurJour + 1
                        y: 2 + modelData.ligne * 22
                        width: (modelData.a - modelData.de) * grille.largeurJour - 2
                        height: 20
                        radius: 3
                        color: teinte
                        Label {
                            anchors.fill: parent
                            anchors.leftMargin: 6
                            anchors.rightMargin: 4
                            verticalAlignment: Text.AlignVCenter
                            text: bande.f.titre
                            font.pixelSize: 12
                            font.strikeout: bande.f.annule
                            elide: Text.ElideRight
                            color: agenda.encre(bande.teinte)
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: agenda.montrerFiche(bande.modelData.fiche)
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: agenda.trait
            }

            Flickable {
                id: defilement
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: width
                contentHeight: 24 * grille.hh
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                // Au départ, la journée de travail : de 7 h 30, ou d'une
                // demi-heure avant le premier événement s'il commence plus tôt.
                // Période pour laquelle le défilement a été placé : une
                // synchronisation qui relit la même période ne le déplace pas.
                property string periodePlacee: ""
                function placer() {
                    var periode = agenda.genre + agenda.iso(agenda.jour)
                    if (periode === periodePlacee)
                        return
                    periodePlacee = periode
                    var depart = 7.5
                    var cases = agenda.donnees.cases || []
                    for (var i = 0; i < cases.length; ++i)
                        depart = Math.min(depart, cases[i].debut / 60 - 0.5)
                    contentY = Math.max(0, Math.min(contentHeight - height, depart * grille.hh))
                }
                Component.onCompleted: placer()
                Connections {
                    target: agenda
                    function onDonneesChanged() { defilement.placer() }
                }

                // Heures et demi-heures.
                Repeater {
                    model: 24
                    delegate: Item {
                        required property int index
                        y: index * grille.hh
                        width: defilement.width
                        height: grille.hh
                        Label {
                            visible: index > 0
                            x: 4
                            y: -height / 2
                            width: grille.marge - 10
                            horizontalAlignment: Text.AlignRight
                            text: (index < 10 ? "0" : "") + index + ":00"
                            font.pixelSize: 11
                            opacity: 0.65
                        }
                        Rectangle {
                            x: grille.marge
                            width: parent.width - grille.marge
                            height: 1
                            color: agenda.trait
                        }
                        Rectangle {
                            x: grille.marge
                            y: grille.hh / 2
                            width: parent.width - grille.marge
                            height: 1
                            color: agenda.traitLeger
                        }
                    }
                }
                // Séparations des jours, et le jour courant teinté.
                Repeater {
                    model: grille.nbJours
                    delegate: Rectangle {
                        required property int index
                        x: grille.marge + index * grille.largeurJour
                        width: grille.largeurJour
                        height: defilement.contentHeight
                        color: agenda.donnees.jours[index].aujourdhui
                               ? Qt.rgba(agenda.accent.r, agenda.accent.g, agenda.accent.b, 0.05)
                               : "transparent"
                        Rectangle {
                            width: 1
                            height: parent.height
                            color: agenda.trait
                        }
                    }
                }
                // Double clic sur un créneau libre : un nouvel événement à la
                // demi-heure.
                MouseArea {
                    width: defilement.contentWidth
                    height: defilement.contentHeight
                    onDoubleClicked: function(souris) {
                        var j = Math.floor((souris.x - grille.marge) / grille.largeurJour)
                        if (j < 0 || j >= grille.nbJours)
                            return
                        agenda.nouvelEvenement(agenda.donnees.jours[j].date, Math.floor(souris.y / grille.hh * 2) * 30, false)
                    }
                }
                // Événements.
                Repeater {
                    model: agenda.donnees.cases || []
                    delegate: Rectangle {
                        id: caseHeure
                        required property var modelData
                        readonly property var f: agenda.donnees.fiches[modelData.fiche]
                        readonly property color teinte: f.couleur
                        readonly property real largeurColonne: (grille.largeurJour - 4) / modelData.colonnes
                        x: grille.marge + modelData.jour * grille.largeurJour + 2 + modelData.colonne * largeurColonne
                        y: modelData.debut / 60 * grille.hh + 1
                        width: largeurColonne - 2
                        height: Math.max(16, (modelData.fin - modelData.debut) / 60 * grille.hh - 2)
                        radius: 4
                        clip: true
                        color: Qt.tint(agenda.palette.base, Qt.rgba(teinte.r, teinte.g, teinte.b, 0.28))
                        border.width: 1
                        border.color: Qt.rgba(teinte.r, teinte.g, teinte.b, 0.7)
                        Rectangle {
                            width: 4
                            height: parent.height
                            radius: 2
                            color: caseHeure.teinte
                        }
                        Column {
                            x: 8
                            y: 2
                            width: parent.width - 10
                            Label {
                                width: parent.width
                                text: caseHeure.f.titre
                                font.bold: true
                                font.pixelSize: 12
                                font.strikeout: caseHeure.f.annule
                                elide: Text.ElideRight
                                maximumLineCount: caseHeure.height > 50 ? 2 : 1
                                wrapMode: Text.Wrap
                            }
                            Label {
                                width: parent.width
                                visible: caseHeure.height > 34 && caseHeure.modelData.heures.length > 0
                                text: caseHeure.modelData.heures
                                font.pixelSize: 11
                                opacity: 0.8
                                elide: Text.ElideRight
                            }
                            Label {
                                width: parent.width
                                visible: caseHeure.height > 52 && caseHeure.f.lieu.length > 0
                                text: caseHeure.f.lieu
                                font.pixelSize: 11
                                opacity: 0.8
                                elide: Text.ElideRight
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: agenda.montrerFiche(caseHeure.modelData.fiche)
                        }
                    }
                }
                // L'heure courante, sur la colonne du jour.
                Repeater {
                    model: grille.nbJours
                    delegate: Rectangle {
                        required property int index
                        visible: agenda.donnees.jours[index].aujourdhui
                        x: grille.marge + index * grille.largeurJour
                        y: agenda.minutesMaintenant / 60 * grille.hh
                        width: grille.largeurJour
                        height: 2
                        color: "#d9534f"
                        Rectangle {
                            x: -4
                            y: -3
                            width: 8
                            height: 8
                            radius: 4
                            color: "#d9534f"
                        }
                    }
                }
            }
        }
    }

    // --- fiche d'un événement ------------------------------------------------------
    Popup {
        id: popupFiche
        property var f: ({})
        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 32, 480)
        modal: true
        focus: true
        padding: 16
        contentItem: ColumnLayout {
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Rectangle {
                    Layout.preferredWidth: 12
                    Layout.preferredHeight: 12
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 5
                    radius: 3
                    color: popupFiche.f.couleur || "transparent"
                }
                Label {
                    Layout.fillWidth: true
                    text: popupFiche.f.titre || ""
                    font.bold: true
                    font.pixelSize: 17
                    font.strikeout: popupFiche.f.annule || false
                    wrapMode: Text.Wrap
                }
            }
            Label {
                visible: popupFiche.f.annule || false
                text: qsTr("Annulé par l'organisateur")
                color: "#b00020"
            }
            Label {
                Layout.fillWidth: true
                text: popupFiche.f.quand || ""
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                visible: (popupFiche.f.lieu || "").length > 0
                text: qsTr("Lieu : %1").arg(popupFiche.f.lieu || "")
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                text: qsTr("Agenda : %1").arg(popupFiche.f.nomAgenda || "")
                opacity: 0.75
                wrapMode: Text.Wrap
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(texteFiche.implicitHeight, 260)
                visible: (popupFiche.f.description || "").length > 0
                TextArea {
                    id: texteFiche
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextArea.Wrap
                    text: popupFiche.f.description || ""
                    background: null
                }
            }
            Label {
                Layout.fillWidth: true
                visible: (popupFiche.f.participants || []).length > 0
                wrapMode: Text.Wrap
                font.pixelSize: 12
                text: {
                    var f = popupFiche.f
                    if (!f.participants || f.participants.length === 0)
                        return ""
                    var o = f.organisateur || {}
                    var lignes = [qsTr("Organisateur : %1").arg(o.nom || o.adresse || "")]
                    var liste = f.participants.slice(0, 12).map(function(p) {
                        return (p.nom || p.adresse) + " (" + agenda.libelleReponse(p.statut) + ")"
                    })
                    lignes.push(qsTr("Participants : %1").arg(liste.join(", "))
                                + (f.participants.length > 12 ? qsTr(", et %1 autres").arg(f.participants.length - 12) : ""))
                    return lignes.join("\n")
                }
            }
            RowLayout {
                Layout.fillWidth: true
                visible: (popupFiche.f.maReponse || "").length > 0
                spacing: 6
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Votre réponse : %1").arg(agenda.libelleReponse(popupFiche.f.maReponse || ""))
                }
                Repeater {
                    model: [["ACCEPTED", qsTr("Accepter")], ["TENTATIVE", qsTr("Provisoire")], ["DECLINED", qsTr("Refuser")]]
                    delegate: Button {
                        required property var modelData
                        text: modelData[1]
                        highlighted: popupFiche.f.maReponse === modelData[0]
                        enabled: agenda.invitationsPossibles(popupFiche.f.agenda)
                        onClicked: agenda.repondre(popupFiche.f, modelData[0])
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                visible: !popupFiche.f.modifiable && (popupFiche.f.motif || "").length > 0
                text: qsTr("Non modifiable ici : %1.").arg(popupFiche.f.motif || "")
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 12
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Button {
                    visible: popupFiche.f.modifiable || false
                    text: qsTr("Modifier")
                    onClicked: agenda.demander("modifier", popupFiche.f)
                }
                Button {
                    visible: popupFiche.f.modifiable || false
                    text: qsTr("Supprimer")
                    onClicked: agenda.demander("supprimer", popupFiche.f)
                }
                Button {
                    text: qsTr("Fermer")
                    onClicked: popupFiche.close()
                }
            }
        }
    }

    // --- portée d'une modification ou d'une suppression ----------------------------
    Popup {
        id: popupPortee
        property string action: ""
        property var f: ({})
        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 32, 460)
        modal: true
        focus: true
        padding: 16
        function choisir(serie) {
            close()
            if (action === "modifier")
                agenda.modifier(f, serie)
            else
                agenda.supprimer(f, serie)
        }
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font.bold: true
                text: popupPortee.action === "supprimer"
                      ? (popupPortee.f.repete ? qsTr("Supprimer « %1 » : cette occurrence, ou toute la série ?")
                                              : qsTr("Supprimer « %1 » ?")).arg(popupPortee.f.titre || "")
                      : qsTr("Modifier « %1 » : cette occurrence, ou toute la série ?").arg(popupPortee.f.titre || "")
            }
            Flow {
                Layout.fillWidth: true
                layoutDirection: Qt.RightToLeft
                spacing: 6
                Button {
                    text: qsTr("Annuler")
                    flat: true
                    onClicked: popupPortee.close()
                }
                Button {
                    visible: popupPortee.f.repete || false
                    text: qsTr("Toute la série")
                    onClicked: popupPortee.choisir(true)
                }
                Button {
                    visible: popupPortee.f.repete || false
                    text: qsTr("Cette occurrence")
                    onClicked: popupPortee.choisir(false)
                }
                Button {
                    visible: !(popupPortee.f.repete || false)
                    text: qsTr("Supprimer")
                    onClicked: popupPortee.choisir(true)
                }
            }
        }
    }

    // --- saisie d'un événement -----------------------------------------------------
    Popup {
        id: popupSaisie
        property var s: ({})
        property bool enCours: false
        property string erreur: ""
        // Début à l'ouverture ou au dernier changement : un nouveau début
        // décale la fin d'autant, comme dans Outlook.
        property var debutPrecedent: null
        readonly property var repetitions: [
            ["", qsTr("Ne se répète pas")], ["DAILY", qsTr("Chaque jour")],
            ["WEEKDAYS", qsTr("Chaque jour ouvré (lundi à vendredi)")], ["WEEKLY", qsTr("Chaque semaine")],
            ["MONTHLY", qsTr("Chaque mois")], ["YEARLY", qsTr("Chaque année")]
        ]
        readonly property var rappels: [
            [-1, qsTr("Aucun rappel")], [0, qsTr("À l'heure du début")], [5, qsTr("5 minutes avant")],
            [10, qsTr("10 minutes avant")], [15, qsTr("15 minutes avant")], [30, qsTr("30 minutes avant")],
            [60, qsTr("1 heure avant")], [120, qsTr("2 heures avant")], [1440, qsTr("1 jour avant")],
            [10080, qsTr("1 semaine avant")]
        ]
        property var choixRepetitions: repetitions
        property var choixRappels: rappels
        readonly property bool nouveau: !(s.objet > 0)
        readonly property bool occurrence: (s.occurrence || "").length > 0

        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 24, 580)
        height: Math.min(agenda.height - 24, implicitHeight)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        padding: 16

        // Participants proposés : le serveur de la boîte de l'agenda doit
        // distribuer les invitations, et une occurrence seule n'en change pas.
        readonly property bool reunionPossible: !occurrence && agenda.invitationsPossibles(
            nouveau ? (agenda.agendasModifiables()[choixAgenda.currentIndex] || {}).id : s.agenda)

        function ouvrir(saisie) {
            s = saisie
            champParticipants.text = (s.participants || []).join(", ")
            erreur = ""
            enCours = false
            champTitre.text = s.titre || ""
            champLieu.text = s.lieu || ""
            caseJournee.checked = s.journee
            champDebutJour.text = agenda.jjmmaaaa(s.debut)
            champFinJour.text = agenda.jjmmaaaa(s.fin)
            champDebutHeure.text = s.journee ? "09:00" : s.debut.substr(11, 5)
            champFinHeure.text = s.journee ? "10:00" : s.fin.substr(11, 5)
            var r = repetitions.slice()
            if (s.repetition === "AUTRE")
                r.push(["AUTRE", qsTr("Répétition personnalisée (conservée)")])
            choixRepetitions = r
            choixRepetition.currentIndex = Math.max(0, r.findIndex(function(x) { return x[0] === (s.repetition || "") }))
            champJusquAu.text = s.jusquAu ? agenda.jjmmaaaa(s.jusquAu) : ""
            var rp = rappels.slice()
            if (!rp.some(function(x) { return x[0] === s.rappel }))
                rp.push([s.rappel, qsTr("%1 minutes avant").arg(s.rappel)])
            choixRappels = rp
            choixRappel.currentIndex = Math.max(0, rp.findIndex(function(x) { return x[0] === s.rappel }))
            champDescription.text = s.description || ""
            var modifiables = agenda.agendasModifiables()
            choixAgenda.model = modifiables.map(function(a) { return a.nom + " — " + a.boite })
            choixAgenda.currentIndex = Math.max(0, modifiables.findIndex(function(a) { return a.id === s.agenda }))
            debutPrecedent = lireDebut()
            open()
            champTitre.forceActiveFocus()
        }

        function lireDebut() {
            return agenda.lireDate(champDebutJour.text, caseJournee.checked ? "00:00" : champDebutHeure.text)
        }

        function lireFin() {
            return agenda.lireDate(champFinJour.text, caseJournee.checked ? "00:00" : champFinHeure.text)
        }

        // Le début a changé : la fin suit, durée gardée.
        function debutChange() {
            var debut = lireDebut()
            var fin = lireFin()
            if (debut && fin && debutPrecedent) {
                var nouvelle = new Date(fin.getTime() + (debut.getTime() - debutPrecedent.getTime()))
                champFinJour.text = agenda.jjmmaaaa(agenda.isoJour(nouvelle))
                if (!caseJournee.checked)
                    champFinHeure.text = agenda.isoHeure(nouvelle).substr(11, 5)
            }
            if (debut)
                debutPrecedent = debut
        }

        function enregistrer() {
            var debut = lireDebut()
            var fin = lireFin()
            if (!debut || !fin) {
                erreur = qsTr("Date ou heure illisible : jj/mm/aaaa et hh:mm.")
                return
            }
            var jusqua = null
            if (champJusquAu.visible && champJusquAu.text.trim().length > 0) {
                jusqua = agenda.lireDate(champJusquAu.text, "00:00")
                if (!jusqua) {
                    erreur = qsTr("Dernier jour de la répétition illisible : jj/mm/aaaa.")
                    return
                }
            }
            var modifiables = agenda.agendasModifiables()
            var participants = null
            if (reunionPossible)
                participants = champParticipants.text.split(/[,;\n]/).map(function(t) { return t.trim() })
                                                     .filter(function(t) { return t.length > 0 })
            var saisie = {
                participants: participants,
                organisateur: s.organisateur || "",
                nomOrganisateur: s.nomOrganisateur || "",
                agenda: nouveau ? (modifiables.length > 0 ? modifiables[choixAgenda.currentIndex].id : 0) : s.agenda,
                objet: s.objet || 0,
                occurrence: s.occurrence || "",
                titre: champTitre.text,
                lieu: champLieu.text,
                description: champDescription.text,
                journee: caseJournee.checked,
                debut: caseJournee.checked ? agenda.isoJour(debut) : agenda.isoHeure(debut),
                fin: caseJournee.checked ? agenda.isoJour(fin) : agenda.isoHeure(fin),
                repetition: occurrence ? "" : choixRepetitions[choixRepetition.currentIndex][0],
                jusquAu: jusqua ? agenda.isoJour(jusqua) : "",
                rappel: choixRappels[choixRappel.currentIndex][0]
            }
            if (nouveau)
                reglagesAgenda.agendaParDefaut = saisie.agenda
            erreur = ""
            enCours = true
            agenda.ecritureEnCours = "saisie"
            agenda.boite.enregistrerEvenement(JSON.stringify(saisie))
        }

        contentItem: ColumnLayout {
            spacing: 10
            Label {
                Layout.fillWidth: true
                font.bold: true
                font.pixelSize: 17
                text: popupSaisie.nouveau ? qsTr("Nouvel événement")
                      : popupSaisie.occurrence ? qsTr("Modifier cette occurrence")
                      : (popupSaisie.s.repetition || "").length > 0 ? qsTr("Modifier la série") : qsTr("Modifier l'événement")
            }
            ScrollView {
                id: defileSaisie
                Layout.fillWidth: true
                Layout.fillHeight: true
                implicitHeight: grilleSaisie.implicitHeight
                contentWidth: availableWidth
                clip: true
                GridLayout {
                    id: grilleSaisie
                    width: defileSaisie.availableWidth
                    columns: agenda.compact ? 1 : 2
                    columnSpacing: 12
                    rowSpacing: 6

                    Label { text: qsTr("Titre") }
                    TextField {
                        id: champTitre
                        Layout.fillWidth: true
                        placeholderText: qsTr("Objet de l'événement")
                        onAccepted: popupSaisie.enregistrer()
                    }
                    Label { text: qsTr("Lieu") }
                    TextField {
                        id: champLieu
                        Layout.fillWidth: true
                    }
                    Item {
                        visible: !agenda.compact
                        implicitWidth: 1
                        implicitHeight: 1
                    }
                    CheckBox {
                        id: caseJournee
                        text: qsTr("Journée entière")
                    }
                    Label { text: qsTr("Début") }
                    RowLayout {
                        spacing: 4
                        TextField {
                            id: champDebutJour
                            Layout.preferredWidth: 120
                            placeholderText: qsTr("jj/mm/aaaa")
                            onEditingFinished: popupSaisie.debutChange()
                        }
                        ToolButton {
                            text: "…"
                            ToolTip.visible: hovered
                            ToolTip.text: qsTr("Choisir dans le calendrier")
                            onClicked: popupCalendrier.ouvrirPour(champDebutJour)
                        }
                        TextField {
                            id: champDebutHeure
                            visible: !caseJournee.checked
                            Layout.preferredWidth: 70
                            placeholderText: qsTr("hh:mm")
                            onEditingFinished: popupSaisie.debutChange()
                        }
                    }
                    Label { text: caseJournee.checked ? qsTr("Dernier jour") : qsTr("Fin") }
                    RowLayout {
                        spacing: 4
                        TextField {
                            id: champFinJour
                            Layout.preferredWidth: 120
                            placeholderText: qsTr("jj/mm/aaaa")
                        }
                        ToolButton {
                            text: "…"
                            ToolTip.visible: hovered
                            ToolTip.text: qsTr("Choisir dans le calendrier")
                            onClicked: popupCalendrier.ouvrirPour(champFinJour)
                        }
                        TextField {
                            id: champFinHeure
                            visible: !caseJournee.checked
                            Layout.preferredWidth: 70
                            placeholderText: qsTr("hh:mm")
                        }
                    }
                    Label {
                        visible: popupSaisie.nouveau
                        text: qsTr("Agenda")
                    }
                    ComboBox {
                        id: choixAgenda
                        visible: popupSaisie.nouveau
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: !popupSaisie.occurrence
                        text: qsTr("Répétition")
                    }
                    ComboBox {
                        id: choixRepetition
                        visible: !popupSaisie.occurrence
                        Layout.fillWidth: true
                        model: popupSaisie.choixRepetitions.map(function(x) { return x[1] })
                    }
                    Label {
                        visible: champJusquAu.parent.visible
                        text: qsTr("Jusqu'au")
                    }
                    RowLayout {
                        visible: !popupSaisie.occurrence && choixRepetition.currentIndex > 0
                                 && popupSaisie.choixRepetitions[choixRepetition.currentIndex][0] !== "AUTRE"
                        spacing: 4
                        TextField {
                            id: champJusquAu
                            Layout.preferredWidth: 120
                            placeholderText: qsTr("sans fin")
                        }
                        ToolButton {
                            text: "…"
                            ToolTip.visible: hovered
                            ToolTip.text: qsTr("Choisir dans le calendrier")
                            onClicked: popupCalendrier.ouvrirPour(champJusquAu)
                        }
                    }
                    Label {
                        visible: popupSaisie.reunionPossible
                        text: qsTr("Participants")
                    }
                    ColumnLayout {
                        visible: popupSaisie.reunionPossible
                        Layout.fillWidth: true
                        spacing: 0
                        TextField {
                            id: champParticipants
                            Layout.fillWidth: true
                            placeholderText: qsTr("adresses, séparées par des virgules")
                            // Propositions pour l'adresse en cours de saisie : la
                            // dernière de la liste.
                            property var propositions: []
                            onTextEdited: {
                                var dernier = text.split(/[,;]/).pop().trim()
                                propositions = dernier.length >= 2 ? JSON.parse(agenda.boite.adressesConnues(dernier)).slice(0, 6) : []
                            }
                            onActiveFocusChanged: if (!activeFocus) propositions = []
                        }
                        Repeater {
                            model: champParticipants.propositions
                            delegate: ItemDelegate {
                                required property var modelData
                                Layout.fillWidth: true
                                padding: 4
                                text: modelData.nom ? modelData.nom + " <" + modelData.adresse + ">" : modelData.adresse
                                onClicked: {
                                    var morceaux = champParticipants.text.split(/[,;]/)
                                    morceaux.pop()
                                    morceaux.push(" " + text)
                                    champParticipants.text = morceaux.map(function(m) { return m.trim() })
                                                                     .filter(function(m) { return m.length > 0 }).join(", ") + ", "
                                    champParticipants.propositions = []
                                    champParticipants.forceActiveFocus()
                                }
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: champParticipants.text.trim().length > 0
                            wrapMode: Text.Wrap
                            font.pixelSize: 11
                            opacity: 0.7
                            text: qsTr("Le serveur envoie les invitations, et leurs mises à jour, à l'enregistrement.")
                        }
                    }
                    Label { text: qsTr("Rappel") }
                    ComboBox {
                        id: choixRappel
                        Layout.fillWidth: true
                        model: popupSaisie.choixRappels.map(function(x) { return x[1] })
                    }
                    Label {
                        Layout.alignment: Qt.AlignTop
                        text: qsTr("Description")
                    }
                    TextArea {
                        id: champDescription
                        Layout.fillWidth: true
                        Layout.preferredHeight: 110
                        wrapMode: TextArea.Wrap
                        selectByMouse: true
                        // Certains styles n'en dessinent pas le cadre : sans lui,
                        // la zone ne se voit pas.
                        background: Rectangle {
                            color: agenda.palette.base
                            border.width: 1
                            border.color: champDescription.activeFocus ? agenda.accent : agenda.palette.mid
                            radius: 2
                        }
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                visible: popupSaisie.erreur.length > 0
                text: popupSaisie.erreur
                color: "#b00020"
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.fillWidth: true
                BusyIndicator {
                    running: popupSaisie.enCours
                    visible: running
                    Layout.preferredHeight: 28
                    Layout.preferredWidth: 28
                }
                Item { Layout.fillWidth: true }
                Button {
                    text: qsTr("Annuler")
                    flat: true
                    onClicked: popupSaisie.close()
                }
                Button {
                    text: qsTr("Enregistrer")
                    enabled: !popupSaisie.enCours
                    highlighted: true
                    onClicked: popupSaisie.enregistrer()
                }
            }
        }
    }

    // Petit calendrier pour choisir une date dans le formulaire.
    Popup {
        id: popupCalendrier
        property var cible: null
        property date mois: new Date()
        padding: 8
        focus: true
        function ouvrirPour(champ) {
            cible = champ
            mois = agenda.lireDate(champ.text, "00:00") || new Date()
            var point = champ.mapToItem(agenda, 0, champ.height)
            x = Math.max(0, Math.min(agenda.width - 260, point.x))
            y = Math.max(0, Math.min(agenda.height - 260, point.y))
            open()
        }
        function choisir(annee, moisChoisi, jourChoisi) {
            cible.text = agenda.deuxChiffres(jourChoisi) + "/" + agenda.deuxChiffres(moisChoisi + 1) + "/" + annee
            if (cible === champDebutJour)
                popupSaisie.debutChange()
            close()
        }
        contentItem: ColumnLayout {
            RowLayout {
                Layout.fillWidth: true
                ToolButton {
                    text: "‹"
                    onClicked: popupCalendrier.mois = new Date(popupCalendrier.mois.getFullYear(), popupCalendrier.mois.getMonth() - 1, 1)
                }
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    font.bold: true
                    text: {
                        var t = Qt.locale("fr_FR").standaloneMonthName(popupCalendrier.mois.getMonth(), Locale.LongFormat)
                        return t.charAt(0).toUpperCase() + t.slice(1) + " " + popupCalendrier.mois.getFullYear()
                    }
                }
                ToolButton {
                    text: "›"
                    onClicked: popupCalendrier.mois = new Date(popupCalendrier.mois.getFullYear(), popupCalendrier.mois.getMonth() + 1, 1)
                }
            }
            DayOfWeekRow {
                Layout.fillWidth: true
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property string shortName
                    text: shortName.charAt(0).toUpperCase()
                    horizontalAlignment: Text.AlignHCenter
                    opacity: 0.6
                    font.pixelSize: 11
                }
            }
            MonthGrid {
                id: grilleCalendrier
                Layout.preferredWidth: 240
                month: popupCalendrier.mois.getMonth()
                year: popupCalendrier.mois.getFullYear()
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property var model
                    text: model.day
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    opacity: model.month === grilleCalendrier.month ? 1 : 0.4
                    font.bold: model.today
                    MouseArea {
                        anchors.fill: parent
                        onClicked: popupCalendrier.choisir(parent.model.year, parent.model.month, parent.model.day)
                    }
                }
            }
        }
    }
}
