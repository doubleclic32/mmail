// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Nouveau contact » / « Modifier le contact » (0.6.20) : la fiche
// d'un carnet d'adresses du serveur (SOGo pour Mailcow), à la manière
// d'Outlook — nom, organisation, adresses de courriel et numéros typés,
// adresse postale, note. Seul ce que l'on change est écrit (cf. `fiche` dans
// le noyau) : photo, anniversaire et ce qu'un autre logiciel a mis restent.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueFiche { id: dlgFiche } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgFiche
    // La fiche telle que montrée à l'ouverture (ou après un conflit) : le
    // noyau n'écrit que ce qui l'en distingue.
    property var origine: ({})
    property int ficheId: 0
    property string etag: ""
    property var carnets: []
    property string motif: ""
    // Vrai entre « Enregistrer » ou « Supprimer » et la réponse du serveur.
    property bool attente: false
    property bool suppression: false
    // Faux tant que le nom affiché suit prénom et nom.
    property bool nomAfficheLibre: false
    readonly property var genresCourriel: [
        { cle: "travail", nom: qsTr("Travail") }, { cle: "domicile", nom: qsTr("Domicile") }, { cle: "autre", nom: qsTr("Autre") }
    ]
    readonly property var genresTelephone: [
        { cle: "portable", nom: qsTr("Portable") }, { cle: "travail", nom: qsTr("Travail") },
        { cle: "domicile", nom: qsTr("Domicile") }, { cle: "fax", nom: qsTr("Fax") }, { cle: "autre", nom: qsTr("Autre") }
    ]

    /// Nouveau contact, prérempli de `fiche` (un correspondant, par
    /// exemple), dans le carnet par défaut.
    function ouvrirNouveau(fiche) {
        carnets = JSON.parse(boite.carnetsModifiables())
        if (carnets.length === 0) {
            messageEtat.texte = qsTr("Aucun carnet d'adresses modifiable : il en faut un sur le serveur d'une boîte connectée (le webmail, pour les boîtes de MMEDIA).")
            return
        }
        var defaut = 0
        for (var i = 0; i < carnets.length; ++i)
            if (carnets[i].defaut)
                defaut = i
        choixCarnet.currentIndex = defaut
        remplir(fiche || {}, {})
        open()
        champPrenom.forceActiveFocus()
    }

    /// La fiche `id` de l'index, à modifier.
    function ouvrirFiche(id) {
        var texte = boite.ficheContact(id)
        if (texte.length === 0) {
            messageEtat.texte = qsTr("Ce contact n'est plus dans le carnet.")
            return
        }
        var f = JSON.parse(texte)
        if (f.relire) {
            boite.synchroniserAgendas(true)
            messageEtat.texte = qsTr("La fiche est en cours de lecture depuis le serveur : rouvrez-la dans un instant.")
            return
        }
        carnets = []
        remplir(f, f)
        open()
        champPrenom.forceActiveFocus()
    }

    function remplir(f, o) {
        origine = o
        ficheId = f.id || 0
        etag = f.etag || ""
        motif = ""
        attente = false
        champPrenom.text = f.prenom || ""
        champNom.text = f.nom || ""
        champNomAffiche.text = f.nomAffiche || ""
        nomAfficheLibre = champNomAffiche.text.length > 0 && champNomAffiche.text !== compose()
        champOrganisation.text = f.organisation || ""
        champFonction.text = f.fonction || ""
        champNote.text = f.note || ""
        var a = f.adresse || {}
        choixGenreAdresse.currentIndex = Math.max(0, ["travail", "domicile", "autre"].indexOf(a.genre || "travail"))
        champRue.text = a.rue || ""
        champCodePostal.text = a.codePostal || ""
        champVille.text = a.ville || ""
        champPays.text = a.pays || ""
        remplirListe(modeleCourriels, f.courriels || [], "travail")
        remplirListe(modeleTelephones, f.telephones || [], "portable")
    }

    function remplirListe(modele, liste, genre) {
        modele.clear()
        for (var i = 0; i < liste.length; ++i)
            modele.append({ genre: liste[i].genre || genre, valeur: liste[i].valeur || "",
                            cle: liste[i].cle === null || liste[i].cle === undefined ? -1 : liste[i].cle })
        // Toujours une ligne à remplir.
        if (modele.count === 0)
            modele.append({ genre: genre, valeur: "", cle: -1 })
    }

    function compose() {
        return (champPrenom.text.trim() + " " + champNom.text.trim()).trim()
    }

    function suivreNom() {
        if (!nomAfficheLibre)
            champNomAffiche.text = compose()
    }

    function liste(modele) {
        var l = []
        for (var i = 0; i < modele.count; ++i) {
            var e = modele.get(i)
            if (e.valeur.trim().length > 0 || e.cle >= 0)
                l.push({ genre: e.genre, valeur: e.valeur.trim(), cle: e.cle >= 0 ? e.cle : null })
        }
        return l
    }

    /// Le formulaire, tel que le noyau l'attend (cf. `fiche::Fiche`).
    function saisie() {
        return {
            id: ficheId,
            carnet: ficheId > 0 ? (origine.carnet || 0) : (carnets.length > 0 ? carnets[choixCarnet.currentIndex].id : 0),
            etag: etag,
            nomAffiche: champNomAffiche.text.trim(),
            prenom: champPrenom.text.trim(),
            nom: champNom.text.trim(),
            organisation: champOrganisation.text.trim(),
            fonction: champFonction.text.trim(),
            courriels: liste(modeleCourriels),
            telephones: liste(modeleTelephones),
            adresse: { genre: ["travail", "domicile", "autre"][choixGenreAdresse.currentIndex],
                       rue: champRue.text.trim(), codePostal: champCodePostal.text.trim(),
                       ville: champVille.text.trim(), pays: champPays.text.trim() },
            note: champNote.text.trim()
        }
    }

    function valider() {
        var s = saisie()
        var rien = [s.nomAffiche, s.prenom, s.nom, s.organisation, s.fonction, s.note, s.adresse.rue,
                    s.adresse.ville, s.adresse.codePostal, s.adresse.pays].join("").length === 0
                && s.courriels.filter(function(c) { return c.valeur.length > 0 }).length === 0
                && s.telephones.filter(function(c) { return c.valeur.length > 0 }).length === 0
        if (rien) {
            motif = qsTr("Donnez au moins un nom ou une coordonnée.")
            return
        }
        for (var i = 0; i < s.courriels.length; ++i) {
            var v = s.courriels[i].valeur
            if (v.length > 0 && (v.indexOf("@") <= 0 || v.indexOf("@") === v.length - 1 || /\s/.test(v))) {
                motif = qsTr("Adresse de courriel invalide : %1").arg(v)
                return
            }
        }
        motif = ""
        attente = true
        suppression = false
        boite.enregistrerContact(JSON.stringify(s), JSON.stringify(origine))
    }

    function supprimer() {
        attente = true
        suppression = true
        boite.supprimerContacts(JSON.stringify([ficheId]))
    }

    /// Scénarios d'essai : une saisie au clavier dans un champ, réactions
    /// comprises (le nom affiché qui suit prénom et nom).
    function essaiSaisir(champ, texte) {
        var champs = { prenom: champPrenom, nom: champNom, nomAffiche: champNomAffiche, organisation: champOrganisation,
                       fonction: champFonction, note: champNote, ville: champVille }
        champs[champ].text = texte
        if (champ === "prenom" || champ === "nom")
            suivreNom()
        else if (champ === "nomAffiche")
            nomAfficheLibre = texte.length > 0
    }

    function essaiAjouter(liste, genre, valeur) {
        (liste === "courriels" ? modeleCourriels : modeleTelephones).append({ genre: genre, valeur: valeur, cle: -1 })
    }

    /// Réponse du noyau à « Enregistrer » ou « Supprimer ».
    function issue(ok, message, conflit) {
        if (!attente)
            return
        attente = false
        if (ok) {
            close()
            messageEtat.texte = message
            return
        }
        if (!conflit || ficheId === 0) {
            motif = message
            return
        }
        if (suppression) {
            // Rien n'est supprimé : la fiche du serveur, à revoir.
            var f = JSON.parse(boite.ficheContact(ficheId) || "{}")
            if (f.id) {
                remplir(f, f)
                motif = qsTr("Ce contact a été modifié ailleurs entre-temps : il n'est pas supprimé. La fiche montre sa version actuelle ; « Supprimer… » le supprime.")
            } else {
                close()
                messageEtat.texte = qsTr("Ce contact a déjà été supprimé ailleurs.")
            }
            return
        }
        // Changée ailleurs entre-temps : la version du serveur, plus ce que
        // la saisie changeait ; rien n'a été écrit.
        var r = JSON.parse(boite.reprendreContact(JSON.stringify(saisie()), JSON.stringify(origine)))
        remplir(r.fiche, r.origine)
        if (r.disparue) {
            motif = qsTr("Ce contact a été supprimé ailleurs entre-temps. « Enregistrer » le recrée avec cette fiche ; « Annuler » le laisse supprimé.")
            carnets = JSON.parse(boite.carnetsModifiables())
            for (var i = 0; i < carnets.length; ++i)
                if (carnets[i].id === r.fiche.carnet)
                    choixCarnet.currentIndex = i
        } else {
            motif = qsTr("Ce contact a été modifié ailleurs entre-temps : la fiche montre maintenant la version du serveur, avec vos changements.")
                    + (r.conflits.length > 0
                       ? " " + qsTr("Changé des deux côtés, votre saisie l'emporte : %1.").arg(r.conflits.join(", ")) : "")
                    + " " + qsTr("Vérifiez, puis enregistrez de nouveau.")
        }
    }

    Connections {
        target: boite
        function onContactsEcrits(ok, message, conflit) { dlgFiche.issue(ok, message, conflit) }
    }

    title: ficheId > 0 ? qsTr("Modifier le contact") : qsTr("Nouveau contact")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(620, fenetre.width - 16)
    height: Math.min(720, fenetre.height - 24)
    padding: 10
    closePolicy: Popup.CloseOnEscape
    // « Enregistrer » ne ferme pas d'office : un refus laisse corriger.
    footer: DialogButtonBox {
        Button {
            text: qsTr("Supprimer…")
            visible: dlgFiche.ficheId > 0
            enabled: !dlgFiche.attente
            DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
            onClicked: confirmationSuppression.open()
        }
        Button {
            text: dlgFiche.attente ? qsTr("Enregistrement…") : qsTr("Enregistrer")
            enabled: !dlgFiche.attente
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgFiche.valider()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }

    Dialog {
        id: confirmationSuppression
        title: qsTr("Supprimer ce contact ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: confirmationSuppression.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Supprimer « %1 » du carnet d'adresses du serveur ? Une copie de la fiche est gardée sur ce poste.")
                      .arg(champNomAffiche.text || dlgFiche.compose())
            }
        }
        onAccepted: dlgFiche.supprimer()
    }

    // Une ligne d'adresse de courriel ou de téléphone : genre, valeur, retrait.
    component LigneCoordonnee: RowLayout {
        id: ligne
        required property var modele
        required property var genres
        required property int index
        required property string genre
        required property string valeur
        property string indication: ""
        Layout.fillWidth: true
        spacing: 6
        ComboBox {
            model: ligne.genres
            textRole: "nom"
            implicitContentWidthPolicy: ComboBox.WidestText
            currentIndex: Math.max(0, ligne.genres.map(function(g) { return g.cle }).indexOf(ligne.genre))
            onActivated: function(i) { ligne.modele.setProperty(ligne.index, "genre", ligne.genres[i].cle) }
        }
        TextField {
            MenuSaisie {}
            Layout.fillWidth: true
            text: ligne.valeur
            placeholderText: ligne.indication
            onTextEdited: ligne.modele.setProperty(ligne.index, "valeur", text)
        }
        ToolButton {
            text: "×"
            font.pixelSize: 18
            Accessible.name: qsTr("Retirer")
            ToolTip.visible: hovered
            ToolTip.delay: 600
            ToolTip.text: qsTr("Retirer cette ligne")
            onClicked: ligne.modele.remove(ligne.index)
        }
    }

    ListModel { id: modeleCourriels }
    ListModel { id: modeleTelephones }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: dlgFiche.availableWidth - 14
            spacing: 6
            RowLayout {
                visible: dlgFiche.ficheId === 0 && dlgFiche.carnets.length > 1
                Layout.fillWidth: true
                Label { text: qsTr("Carnet :") }
                ComboBox {
                    id: choixCarnet
                    Layout.fillWidth: true
                    model: dlgFiche.carnets.map(function(c) { return c.nom + " — " + c.boite })
                }
            }
            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 8
                rowSpacing: 4
                Label { text: qsTr("Prénom :") }
                TextField {
                    id: champPrenom
                    MenuSaisie {}
                    Layout.fillWidth: true
                    onTextEdited: dlgFiche.suivreNom()
                }
                Label { text: qsTr("Nom :") }
                TextField {
                    id: champNom
                    MenuSaisie {}
                    Layout.fillWidth: true
                    onTextEdited: dlgFiche.suivreNom()
                }
                Label { text: qsTr("Nom affiché :") }
                TextField {
                    id: champNomAffiche
                    MenuSaisie {}
                    Layout.fillWidth: true
                    placeholderText: qsTr("Prénom et nom, par défaut")
                    onTextEdited: dlgFiche.nomAfficheLibre = text.length > 0
                }
                Label { text: qsTr("Organisation :") }
                TextField {
                    id: champOrganisation
                    MenuSaisie {}
                    Layout.fillWidth: true
                }
                Label { text: qsTr("Fonction :") }
                TextField {
                    id: champFonction
                    MenuSaisie {}
                    Layout.fillWidth: true
                }
            }
            Label {
                text: qsTr("Adresses de courriel")
                font.bold: true
                Layout.topMargin: 6
            }
            Repeater {
                model: modeleCourriels
                delegate: LigneCoordonnee {
                    modele: modeleCourriels
                    genres: dlgFiche.genresCourriel
                    indication: qsTr("nom@exemple.fr")
                }
            }
            Button {
                text: qsTr("Ajouter une adresse")
                flat: true
                onClicked: modeleCourriels.append({ genre: "travail", valeur: "", cle: -1 })
            }
            Label {
                text: qsTr("Téléphones")
                font.bold: true
            }
            Repeater {
                model: modeleTelephones
                delegate: LigneCoordonnee {
                    modele: modeleTelephones
                    genres: dlgFiche.genresTelephone
                }
            }
            Button {
                text: qsTr("Ajouter un numéro")
                flat: true
                onClicked: modeleTelephones.append({ genre: "travail", valeur: "", cle: -1 })
            }
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Adresse postale")
                    font.bold: true
                }
                ComboBox {
                    id: choixGenreAdresse
                    model: [qsTr("Travail"), qsTr("Domicile"), qsTr("Autre")]
                }
            }
            TextArea {
                id: champRue
                MenuSaisie {}
                Layout.fillWidth: true
                placeholderText: qsTr("Rue")
                wrapMode: TextEdit.Wrap
                background: Rectangle { color: fenetre.palette.base; border.color: fenetre.palette.mid }
            }
            RowLayout {
                Layout.fillWidth: true
                TextField {
                    id: champCodePostal
                    MenuSaisie {}
                    Layout.preferredWidth: 90
                    placeholderText: qsTr("Code postal")
                }
                TextField {
                    id: champVille
                    MenuSaisie {}
                    Layout.fillWidth: true
                    placeholderText: qsTr("Ville")
                }
                TextField {
                    id: champPays
                    MenuSaisie {}
                    Layout.preferredWidth: 120
                    placeholderText: qsTr("Pays")
                }
            }
            Label {
                text: qsTr("Note")
                font.bold: true
            }
            TextArea {
                id: champNote
                MenuSaisie {}
                Layout.fillWidth: true
                Layout.minimumHeight: 60
                wrapMode: TextEdit.Wrap
                background: Rectangle { color: fenetre.palette.base; border.color: fenetre.palette.mid }
            }
            Label {
                Layout.fillWidth: true
                visible: dlgFiche.motif.length > 0
                wrapMode: Text.Wrap
                color: fenetre.couleurAlerte
                text: dlgFiche.motif
            }
        }
    }
}
