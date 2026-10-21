// SPDX-License-Identifier: GPL-3.0-or-later
// Rédaction d’un message : fenêtre à part sur le bureau, volet plein écran en vue compacte.
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où le Component « composantRedaction » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Page {
    id: redac
    // Une fenêtre à part ne reçoit ni la palette ni la police de la
    // fenêtre principale : l'apparence choisie les lui transmet.
    palette: fenetre.palette
    font: fenetre.font

    property string jeton: ""
    property var conteneur: null
    property int brouillonUid: 0
    property string origineChemin: ""
    property int origineUid: 0
    property string origineMode: ""
    property string enReponseA: ""
    property var references: []
    // Fichiers joints : [{chemin, nom, taille}].
    property var pieces: []
    property bool modifie: false
    property bool occupe: false
    property bool chargement: false
    property bool fermetureConfirmee: false
    property bool fermerApresEnregistrement: false
    property bool afficherCopies: false
    property bool miseEnForme: reglagesRedaction.miseEnForme
    // Lus par les scénarios d'essai (cf. Scenarios.qml, « majuscule »).
    readonly property alias corps: corpsRedaction
    readonly property alias mefCorps: mef
    property string brouillonChemin: ""
    // Rédaction gardée sur le poste (noyau, garde.rs) : son
    // identifiant et le dernier état écrit ; empreinte du contenu au
    // dernier enregistrement sur le serveur.
    property string idGarde: ""
    property string etatGarde: ""
    property string empreinteEnregistree: ""
    property bool enregistrementAuto: false
    // Options d'envoi (menu « Options ») : importance 1 / 0 / -1,
    // accusés, heure d'envoi (secondes Unix, 0 : tout de suite).
    property int importance: 0
    property bool accuseRemise: false
    property bool confirmationLecture: false
    property real envoiDiffere: 0
    readonly property string resumeOptions: {
        var t = []
        if (importance > 0) t.push(qsTr("Importance haute"))
        if (importance < 0) t.push(qsTr("Importance basse"))
        if (accuseRemise) t.push(qsTr("Accusé de réception"))
        if (confirmationLecture) t.push(qsTr("Confirmation de lecture"))
        if (envoiDiffere > 0) t.push(qsTr("Envoi %1").arg(fenetre.heureEnvoi(envoiDiffere)))
        return t.join("  ·  ")
    }
    property string note: ""
    property bool noteErreur: false
    readonly property string titre: champObjet.text.length > 0 ? champObjet.text : qsTr("Nouveau message")

    /// Le champ « De » sur l'adresse du compte `compte`.
    function choisirCompte(compte) {
        var liste = fenetre.expediteursConnus
        for (var i = 0; i < liste.length; ++i) {
            if (liste[i].compte === compte) {
                choixCompte.currentIndex = i
                return
            }
        }
    }
    /// Le champ « De » sur `adresse`, si c'est l'une de celles du compte
    /// choisi ; sinon il ne bouge pas.
    function choisirExpediteur(adresse) {
        var c = compteChoisi()
        if (!c || !adresse)
            return
        var liste = fenetre.expediteursConnus
        for (var i = 0; i < liste.length; ++i) {
            if (liste[i].compte === c.compte && liste[i].adresse.toLowerCase() === adresse.toLowerCase()) {
                choixCompte.currentIndex = i
                return
            }
        }
    }
    /// L'adresse choisie dans « De » : { compte, adresse, compteAdresse }.
    function compteChoisi() {
        return fenetre.expediteursConnus[choixCompte.currentIndex] || null
    }

    function identiteChoisie() {
        return fenetre.identiteExpediteur(compteChoisi())
    }

    function pret() {
        chargement = true
        var i = identiteChoisie()
        if (i.nouveaux && i.html.length > 0) {
            // Signature avec images : elle exige la mise en forme.
            miseEnForme = true
            mef.poserTexte("\n\n")
            mef.insererHtml(2, i.html)
        } else {
            mef.poserTexte(i.nouveaux && i.signature.length > 0 ? "\n\n" + i.signature : "")
        }
        corpsRedaction.cursorPosition = 0
        chargement = false
        occupe = false
        modifie = false
        champA.forceActiveFocus()
    }

    function objetActuel() {
        return champObjet.text
    }

    /// HTML de la rédaction, tel qu'il partirait (scénarios d'essai).
    function htmlActuel() {
        return mef.html()
    }

    function insererSignature() {
        var i = identiteChoisie()
        if (i.html.length > 0) {
            miseEnForme = true
            mef.insererHtml(corpsRedaction.cursorPosition, i.html)
            return
        }
        if (i.signature.length === 0) {
            signaler(qsTr("Aucune signature pour ce compte : menu « Comptes », « Nom et signature… »."), true)
            return
        }
        mef.inserer(corpsRedaction.cursorPosition, i.signature)
    }

    /// Mise en forme coupée : le texte est ramené au brut, puces et
    /// numéros compris, pour que ce qui s'affiche soit ce qui part.
    function basculerMiseEnForme(active) {
        miseEnForme = active
        if (!active && mef.enrichi())
            mef.poserTexte(mef.texte())
    }

    function attendre(texte) {
        chargement = true
        occupe = true
        signaler(texte, false)
    }

    function signaler(texte, erreur) {
        note = texte
        noteErreur = erreur
    }

    /// Pré-remplit la rédaction (réponse, transfert, brouillon repris).
    function remplir(p) {
        chargement = true
        // L'adresse d'abord : la signature se prend d'elle.
        choisirExpediteur(p.de || "")
        champA.text = p.a || ""
        champCc.text = p.cc || ""
        champCci.text = p.cci || ""
        champObjet.text = p.objet || ""
        if (p.html && p.html.length > 0) {
            miseEnForme = true
            mef.poserHtml(p.html)
            // Réponse ou transfert d'un message HTML : la signature entre les
            // deux lignes vides et le texte cité, comme en texte brut.
            var iH = identiteChoisie()
            if (p.mode !== "brouillon" && iH.reponses) {
                if (iH.html.length > 0)
                    mef.insererHtml(2, iH.html + "<p><br></p><p><br></p>")
                else if (iH.signature.length > 0)
                    mef.inserer(2, iH.signature + "\n\n")
            }
        } else {
            var i = identiteChoisie()
            var avecSignature = p.mode !== "brouillon" && i.reponses
            if (avecSignature && i.html.length > 0) {
                // La signature entre les deux lignes vides et le texte cité.
                miseEnForme = true
                mef.poserTexte("\n\n" + (p.texte || ""))
                mef.insererHtml(2, i.html)
            } else {
                var signature = avecSignature && i.signature.length > 0 ? "\n\n" + i.signature : ""
                mef.poserTexte(signature + (p.texte || ""))
            }
        }
        brouillonUid = p.brouillonUid || 0
        brouillonChemin = p.brouillonChemin || ""
        importance = p.importance || 0
        accuseRemise = p.accuseRemise === true
        confirmationLecture = p.confirmationLecture === true
        envoiDiffere = p.envoiDiffere || 0
        origineChemin = p.origineChemin || ""
        origineUid = p.origineUid || 0
        origineMode = p.origineMode || ""
        enReponseA = p.enReponseA || ""
        references = p.references || []
        var liste = []
        var chemins = p.pieces || []
        for (var i = 0; i < chemins.length; ++i) {
            var d = boite.decrireFichier(chemins[i])
            if (d.length > 0)
                liste.push(JSON.parse(d))
        }
        pieces = liste
        afficherCopies = champCc.text.length > 0 || champCci.text.length > 0
        chargement = false
        occupe = false
        modifie = false
        signaler("", false)
        if (champA.text.length === 0) {
            champA.forceActiveFocus()
        } else {
            corpsRedaction.forceActiveFocus()
            corpsRedaction.cursorPosition = 0
        }
    }

    function contenu() {
        var c = compteChoisi()
        return {
            jeton: jeton, de: c ? c.adresse : "", nom: identiteChoisie().nom,
            a: champA.text, cc: champCc.text, cci: champCci.text,
            objet: champObjet.text, texte: mef.texte(), html: miseEnForme ? mef.html() : "",
            pieces: pieces.map(function(f) { return f.chemin }),
            enReponseA: enReponseA, references: references, brouillonUid: brouillonUid,
            brouillonChemin: brouillonChemin,
            origineChemin: origineChemin, origineUid: origineUid, origineMode: origineMode,
            importance: importance, accuseRemise: accuseRemise,
            confirmationLecture: confirmationLecture, envoiDiffere: Math.round(envoiDiffere)
        }
    }

    function ajouterFichiers(urls) {
        var liste = pieces.slice()
        for (var i = 0; i < urls.length; ++i) {
            var d = boite.decrireFichier(urls[i].toString())
            if (d.length === 0)
                continue
            var f = JSON.parse(d)
            if (!liste.some(function(x) { return x.chemin === f.chemin }))
                liste.push(f)
        }
        if (liste.length !== pieces.length) {
            pieces = liste
            modifie = true
        }
    }

    /// Images dans le corps — collées, ou glissées sur le corps (demande
    /// de Manu, 08/10). Elles exigent la mise en forme ; l'envoi les
    /// intègre au message (`cid:`), copiées parmi les images collées du
    /// profil. Larges, elles s'affichent sur 800 pixels.
    function insererImages(urls) {
        if (urls.length === 0)
            return false
        miseEnForme = true
        for (var i = 0; i < urls.length; ++i)
            mef.insererImage(urls[i], 800)
        modifie = true
        return true
    }

    /// Ctrl+V : une image seule du presse-papier (capture d'écran) ou des
    /// images copiées dans l'explorateur entrent dans le corps ; d'autres
    /// fichiers copiés sont joints. Rien de tel : le collage ordinaire.
    function collerImage() {
        var url = pressePapier.enregistrerImage(boite.dossierImagesCollees())
        if (url.length > 0)
            return insererImages([url])
        var fichiers = pressePapier.fichiers()
        if (fichiers.length === 0)
            return false
        var images = [], autres = []
        for (var i = 0; i < fichiers.length; ++i) {
            var copie = boite.importerImage(fichiers[i])
            if (copie.length > 0)
                images.push(copie)
            else
                autres.push(fichiers[i])
        }
        insererImages(images)
        if (autres.length > 0)
            ajouterFichiers(autres)
        return true
    }

    /// Majuscule en début de phrase (0.6.12, demande de Fabienne) : la
    /// minuscule frappée au début d'une phrase passe en capitale (cf.
    /// MiseEnForme.debutDePhrase) ; Ctrl+Z aussitôt après la rend minuscule,
    /// comme dans Outlook. Pas sous Android, où le clavier s'en charge. Rend
    /// vrai si la touche est traitée.
    property int positionMajuscule: -1
    function majuscule(ev) {
        if (Qt.platform.os === "android" || !reglages.majusculeAuto)
            return false
        var c = corpsRedaction
        if (ev.matches(StandardKey.Undo)) {
            var p = positionMajuscule
            positionMajuscule = -1
            if (p < 0 || c.cursorPosition !== p + 1 || c.selectionStart !== c.selectionEnd)
                return false
            mef.remplacerCaractere(p, c.getText(p, p + 1).toLowerCase())
            c.cursorPosition = p + 1
            return true
        }
        positionMajuscule = -1
        if (ev.text.length !== 1 || (ev.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)))
            return false
        var capitale = ev.text.toUpperCase()
        if (capitale === ev.text || capitale.length !== 1 || c.selectionStart !== c.selectionEnd
                || !mef.debutDePhrase(c.cursorPosition))
            return false
        var position = c.cursorPosition
        mef.inserer(position, capitale)
        c.cursorPosition = position + 1
        positionMajuscule = position
        return true
    }

    /// Ouvre une pièce jointe avant l'envoi, pour la vérifier (0.6.9, demande
    /// de Gwenaëlle) : comme à la lecture, avec le logiciel retenu pour son
    /// type, à défaut celui du système. Un programme ou un script ne s'ouvre
    /// pas.
    function ouvrirPiece(p) {
        if (p.risquee) {
            signaler(qsTr("Programme ou script : %1 ne s'ouvre pas depuis MMail.").arg(p.nom), true)
            return
        }
        var programme = Qt.platform.os === "android" ? "" : fenetre.logicielPour(p.nom)
        if (programme.length > 0 && lanceur.lancer(programme, p.chemin))
            signaler(qsTr("Ouverture de %1 avec %2…").arg(p.nom).arg(lanceur.nom(programme)), false)
        else if (Qt.openUrlExternally(lanceur.adresse(p.chemin)))
            signaler(qsTr("Ouverture de %1…").arg(p.nom), false)
        else
            signaler(qsTr("Aucun logiciel n'a pu ouvrir %1.").arg(p.nom), true)
    }

    function retirerPiece(i) {
        var liste = pieces.slice()
        liste.splice(i, 1)
        pieces = liste
        modifie = true
    }

    function envoyer(sansObjet) {
        if (occupe)
            return
        var c = compteChoisi()
        if (!c) {
            signaler(qsTr("Aucun compte pour envoyer ce message."), true)
            return
        }
        if ((champA.text + champCc.text + champCci.text).trim().length === 0) {
            signaler(qsTr("Indiquez au moins un destinataire."), true)
            champA.forceActiveFocus()
            return
        }
        if (champObjet.text.trim().length === 0 && !sansObjet) {
            dlgSansObjet.open()
            return
        }
        if (envoiDiffere > 0 && envoiDiffere * 1000 <= Date.now())
            envoiDiffere = 0
        occupe = true
        signaler(envoiDiffere > 0 ? qsTr("Mise en attente…") : qsTr("Envoi…"), false)
        if (!boite.envoyerMessage(c.compte, JSON.stringify(contenu())))
            echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Envoi impossible."))
    }

    /// `auto` : enregistrement périodique, et non demandé.
    function enregistrer(auto) {
        if (occupe)
            return
        var c = compteChoisi()
        if (!c)
            return
        occupe = true
        enregistrementAuto = auto === true
        empreinteEnregistree = empreinte()
        signaler(qsTr("Enregistrement du brouillon…"), false)
        if (!boite.enregistrerBrouillon(c.compte, JSON.stringify(contenu())))
            echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Enregistrement impossible."))
    }

    function brouillonEnregistre(uid) {
        brouillonUid = uid
        occupe = false
        enregistrementAuto = false
        // Ce qui a été tapé pendant l'enregistrement reste à enregistrer.
        modifie = empreinte() !== empreinteEnregistree
        if (!modifie)
            oublierGarde()
        signaler(qsTr("Brouillon enregistré à %1.").arg(Qt.formatTime(new Date(), "hh:mm")), false)
        if (fermerApresEnregistrement) {
            // Du texte tapé pendant l'enregistrement serait perdu à la
            // fermeture : il part d'abord, la fenêtre se ferme ensuite.
            if (modifie)
                enregistrer()
            else
                conteneur.fermerRedaction()
        }
    }

    function echouer(message) {
        occupe = false
        chargement = false
        fermerApresEnregistrement = false
        if (enregistrementAuto) {
            // Hors ligne, serveur injoignable : la rédaction reste
            // gardée sur le poste, ce n'est pas une erreur à traiter.
            enregistrementAuto = false
            garder()
            signaler(qsTr("Brouillon non enregistré sur le serveur (%1) ; il reste gardé sur ce poste.").arg(message), false)
            return
        }
        signaler(message, true)
    }

    /// Ce qui distingue deux états de la rédaction.
    function empreinte() {
        var c = contenu()
        delete c.jeton
        delete c.brouillonUid
        delete c.brouillonChemin
        return JSON.stringify(c)
    }

    /// Écrit l'état de la rédaction sur le poste, s'il a changé depuis
    /// la dernière écriture.
    function garder() {
        if (!modifie || chargement || idGarde.length === 0)
            return
        var c = compteChoisi()
        var etat = JSON.stringify({ compte: c ? c.compte : 0, miseEnForme: miseEnForme, contenu: contenu() })
        if (etat !== etatGarde && boite.garderRedaction(idGarde, etat))
            etatGarde = etat
    }

    /// La rédaction a trouvé une issue : rien à reprendre.
    function oublierGarde() {
        if (idGarde.length > 0)
            boite.oublierRedactionGardee(idGarde)
        etatGarde = ""
    }

    // Sur le poste toutes les 10 s, tant que la rédaction a changé.
    Timer {
        interval: 10000
        repeat: true
        running: true
        onTriggered: redac.garder()
    }
    // Sur le serveur : 30 s après la première frappe, de sorte que le
    // brouillon paraisse dans son dossier pendant la rédaction, puis
    // une minute après chaque reprise de la frappe. La minuterie part
    // avec la modification et s'arrête une fois le brouillon à jour ;
    // un enregistrement manqué (hors ligne) se retente au tour suivant.
    Timer {
        id: minuterieBrouillon
        interval: redac.brouillonUid > 0 ? 60000 : 30000
        repeat: true
        running: redac.modifie && !redac.chargement
        onTriggered: if (!redac.occupe) redac.enregistrer(true)
    }

    // ---- adresses proposées à la saisie
    // Temporisées : chaque frappe interrogeait l'index, sur le fil de
    // l'interface ; on attend que la saisie marque une pause.
    Timer {
        id: minuterieProposer
        interval: 150
        property var champ: null
        onTriggered: if (champ) redac.proposerMaintenant(champ)
    }
    function proposer(champ) {
        minuterieProposer.champ = champ
        minuterieProposer.restart()
    }
    function proposerMaintenant(champ) {
        var avant = champ.text.substring(0, champ.cursorPosition)
        var coupure = Math.max(avant.lastIndexOf(","), avant.lastIndexOf(";"))
        var morceau = avant.substring(coupure + 1).trim()
        if (morceau.length < 2) {
            suggestions.close()
            return
        }
        var liste = JSON.parse(boite.adressesConnues(morceau))
        if (liste.length === 0) {
            suggestions.close()
            return
        }
        suggestions.champ = champ
        suggestions.liste = liste
        vueSuggestions.currentIndex = 0
        if (!suggestions.opened)
            suggestions.open()
    }

    function affichable(s) {
        if (!s.nom || s.nom.toLowerCase() === s.adresse.toLowerCase())
            return s.adresse
        var nom = s.nom.replace(/"/g, "")
        return /[,;<>]/.test(nom) ? "\"" + nom + "\" <" + s.adresse + ">" : nom + " <" + s.adresse + ">"
    }

    function accepterSuggestion(i) {
        var champ = suggestions.champ
        var s = suggestions.liste[i]
        suggestions.close()
        if (!champ || !s)
            return
        var avant = champ.text.substring(0, champ.cursorPosition)
        var apres = champ.text.substring(champ.cursorPosition).replace(/^[^,;]*[,;]?\s*/, "")
        var coupure = Math.max(avant.lastIndexOf(","), avant.lastIndexOf(";"))
        var tete = avant.substring(0, coupure + 1)
        var nouveau = (tete.length > 0 ? tete + " " : "") + affichable(s) + "; "
        champ.text = nouveau + apres
        champ.cursorPosition = nouveau.length
        modifie = true
    }

    /// Flèches, Entrée, Tab et Échap pilotent la liste quand elle est ouverte.
    function toucheAdresse(ev) {
        if (!suggestions.opened)
            return
        if (ev.key === Qt.Key_Down) {
            vueSuggestions.incrementCurrentIndex()
        } else if (ev.key === Qt.Key_Up) {
            vueSuggestions.decrementCurrentIndex()
        } else if (ev.key === Qt.Key_Return || ev.key === Qt.Key_Enter || ev.key === Qt.Key_Tab) {
            accepterSuggestion(vueSuggestions.currentIndex)
        } else if (ev.key === Qt.Key_Escape) {
            suggestions.close()
        } else {
            return
        }
        ev.accepted = true
    }

    function demanderFermeture() {
        if (!modifie || fermetureConfirmee) {
            conteneur.fermerRedaction()
            return
        }
        dlgFermer.open()
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 6
            anchors.rightMargin: 6
            ToolButton {
                text: redac.envoiDiffere > 0 ? qsTr("Programmer") : qsTr("Envoyer")
                font.bold: true
                enabled: !redac.occupe
                onClicked: redac.envoyer(false)
            }
            ToolButton {
                text: qsTr("Enregistrer")
                enabled: !redac.occupe
                onClicked: redac.enregistrer()
            }
            ToolButton {
                text: qsTr("Joindre…")
                enabled: !redac.occupe
                onClicked: choixFichiers.open()
            }
            ToolButton {
                text: qsTr("Cc / Cci")
                checkable: true
                checked: redac.afficherCopies
                onToggled: redac.afficherCopies = checked
            }
            ToolButton {
                id: boutonOptions
                text: qsTr("Options ▾")
                onClicked: menuOptions.popup(boutonOptions, 0, boutonOptions.height)
            }
            Label {
                text: redac.note
                color: redac.noteErreur ? "#b00020" : fenetre.palette.windowText
                opacity: redac.noteErreur ? 1 : 0.75
                elide: Text.ElideRight
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
            }
            ToolButton {
                text: qsTr("Fermer")
                onClicked: redac.demanderFermeture()
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        GridLayout {
            columns: 2
            columnSpacing: 8
            rowSpacing: 4
            Layout.fillWidth: true
            enabled: !redac.chargement

            Label { text: qsTr("De :") }
            ComboBox {
                id: choixCompte
                model: fenetre.expediteursConnus
                displayText: currentIndex >= 0 && fenetre.expediteursConnus[currentIndex]
                             ? fenetre.expediteursConnus[currentIndex].adresse : ""
                delegate: ItemDelegate {
                    required property var modelData
                    width: ListView.view ? ListView.view.width : implicitWidth
                    text: modelData.adresse
                }
                Layout.fillWidth: true
                onActivated: redac.modifie = true
            }
            Label { text: qsTr("À :") }
            TextField {
                id: champA
                onTextEdited: {
                    redac.modifie = true
                    redac.proposer(champA)
                }
                Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                onActiveFocusChanged: if (!activeFocus && suggestions.champ === champA) suggestions.close()
                placeholderText: qsTr("nom@exemple.fr ; autre@exemple.fr")
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
            }
            Label { text: qsTr("Cc :"); visible: redac.afficherCopies }
            TextField {
                id: champCc
                onTextEdited: {
                    redac.modifie = true
                    redac.proposer(champCc)
                }
                Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                onActiveFocusChanged: if (!activeFocus && suggestions.champ === champCc) suggestions.close()
                visible: redac.afficherCopies
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
            }
            Label { text: qsTr("Cci :"); visible: redac.afficherCopies }
            TextField {
                id: champCci
                onTextEdited: {
                    redac.modifie = true
                    redac.proposer(champCci)
                }
                Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                onActiveFocusChanged: if (!activeFocus && suggestions.champ === champCci) suggestions.close()
                visible: redac.afficherCopies
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
            }
            Label { text: qsTr("Objet :") }
            TextField {
                id: champObjet
                Layout.fillWidth: true
                onTextEdited: redac.modifie = true
            }
        }

        // Options d'envoi choisies, rappelées en clair.
        Label {
            Layout.fillWidth: true
            visible: redac.resumeOptions.length > 0
            text: redac.resumeOptions
            color: redac.importance > 0 ? fenetre.couleurAlerte : fenetre.palette.windowText
            elide: Text.ElideRight
        }

        // Mise en forme : les boutons ne prennent pas le focus, pour que
        // la sélection du texte reste celle sur laquelle ils agissent.
        RowLayout {
            Layout.fillWidth: true
            spacing: 2
            CheckBox {
                text: qsTr("Mise en forme")
                checked: redac.miseEnForme
                focusPolicy: Qt.NoFocus
                onToggled: redac.basculerMiseEnForme(checked)
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("G")
                font.bold: true
                checkable: true
                checked: mef.gras
                focusPolicy: Qt.NoFocus
                onClicked: mef.basculerGras()
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Gras (Ctrl+B)")
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("I")
                font.italic: true
                checkable: true
                checked: mef.italique
                focusPolicy: Qt.NoFocus
                onClicked: mef.basculerItalique()
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Italique (Ctrl+I)")
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("S")
                font.underline: true
                checkable: true
                checked: mef.souligne
                focusPolicy: Qt.NoFocus
                onClicked: mef.basculerSouligne()
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Souligné (Ctrl+U)")
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("• Liste")
                checkable: true
                checked: mef.puces
                focusPolicy: Qt.NoFocus
                onClicked: mef.basculerListe(false)
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("1. Liste")
                checkable: true
                checked: mef.numeros
                focusPolicy: Qt.NoFocus
                onClicked: mef.basculerListe(true)
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("Lien…")
                focusPolicy: Qt.NoFocus
                onClicked: dlgAdresseLien.open()
            }
            ToolButton {
                visible: redac.miseEnForme && !fenetre.tactile
                text: qsTr("Effacer la mise en forme")
                focusPolicy: Qt.NoFocus
                onClicked: mef.effacerMiseEnForme()
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                text: qsTr("Signature")
                focusPolicy: Qt.NoFocus
                onClicked: redac.insererSignature()
            }
        }

        // Fichiers joints, chacun retirable.
        Flow {
            Layout.fillWidth: true
            spacing: 6
            visible: redac.pieces.length > 0
            Repeater {
                model: redac.pieces
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    radius: 4
                    color: fenetre.palette.base
                    border.color: fenetre.palette.mid
                    width: lignePiece.implicitWidth + 8
                    height: lignePiece.implicitHeight + 4
                    RowLayout {
                        id: lignePiece
                        anchors.centerIn: parent
                        spacing: 2
                        // Un clic l'ouvre, pour vérifier la pièce avant l'envoi.
                        ToolButton {
                            text: modelData.nom + "  (" + fenetre.tailleLisible(modelData.taille) + ")"
                            Layout.leftMargin: 2
                            onClicked: redac.ouvrirPiece(modelData)
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                            ToolTip.text: modelData.risquee
                                          ? qsTr("Programme ou script : il ne s'ouvre pas depuis MMail.")
                                          : qsTr("Ouvrir, pour vérifier la pièce avant l'envoi")
                        }
                        ToolButton {
                            text: "×"
                            enabled: !redac.occupe
                            onClicked: redac.retirerPiece(index)
                        }
                    }
                }
            }
        }

        ScrollView {
            id: defilementCorps
            Layout.fillWidth: true
            Layout.fillHeight: true
            ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            rightPadding: ScrollBar.vertical.width
            // Au doigt (retour de Manu du 18/10 : « l'affichage bouge, impossible
            // de sélectionner correctement un mot ») : tant qu'un choix de texte
            // est en cours, le corps ne défile plus — tirer les poignées d'Android
            // étend le choix au lieu de faire défiler.
            Binding {
                target: defilementCorps.contentItem
                property: "interactive"
                value: !(fenetre.tactile && corpsRedaction.selectedText.length > 0)
            }
            TextArea {
                id: corpsRedaction
                textFormat: TextEdit.RichText
                wrapMode: TextEdit.Wrap
                // Au doigt, glisser fait défiler : le choix du texte passe par
                // l'appui long et les poignées, pas par un glissé qui disputait
                // le défilement.
                selectByMouse: !fenetre.tactile
                persistentSelection: true
                font.pointSize: fenetre.tailleColonne("message")
                enabled: !redac.chargement
                placeholderText: qsTr("Votre message")
                background: Rectangle { color: fenetre.palette.base }
                onTextChanged: if (!redac.chargement) redac.modifie = true
                Keys.onPressed: function(ev) {
                    if (ev.matches(StandardKey.Paste) && redac.collerImage())
                        ev.accepted = true
                    else if (redac.majuscule(ev))
                        ev.accepted = true
                }
                // Au doigt, l'appui long choisit le mot touché ; Android pose
                // ensuite ses poignées pour étendre le choix (retour de Manu du
                // 18/10 : le texte ne se sélectionnait pas).
                TapHandler {
                    enabled: fenetre.tactile
                    acceptedDevices: PointerDevice.TouchScreen
                    onLongPressed: {
                        if (corpsRedaction.selectedText.length > 0)
                            return
                        corpsRedaction.forceActiveFocus()
                        corpsRedaction.cursorPosition = corpsRedaction.positionAt(point.position.x, point.position.y)
                        corpsRedaction.selectWord()
                    }
                }
            }
            // Zoom au pincement, celui du corps des messages (retour de Manu
            // du 18/10) — comme le volet de lecture.
            PinchHandler {
                target: null
                property real depart: 1
                onActiveChanged: if (active) depart = reglages.zoomMessage
                onActiveScaleChanged: if (active) fenetre.poserZoom("message", depart * activeScale)
            }
        }

        // Vue tactile (décision 32, point 6) : la mise en forme au bas de la
        // rédaction — donc juste au-dessus du clavier, qui réduit la fenêtre —,
        // en boutons classiques, lisibles (retour de Manu du 18/10, plutôt
        // que des pastilles) ; enfoncés quand l'option est active.
        Flow {
            visible: fenetre.tactile && redac.miseEnForme
            Layout.fillWidth: true
            Layout.topMargin: 4
            Layout.bottomMargin: 4
            spacing: 4
            Button { text: "G"; font.bold: true; font.pointSize: fenetre.tailleBase * 1.15
                     checkable: true; checked: mef.gras; highlighted: checked; focusPolicy: Qt.NoFocus
                     Accessible.name: qsTr("Gras"); onClicked: mef.basculerGras() }
            Button { text: "I"; font.italic: true; font.pointSize: fenetre.tailleBase * 1.15
                     checkable: true; checked: mef.italique; highlighted: checked; focusPolicy: Qt.NoFocus
                     Accessible.name: qsTr("Italique"); onClicked: mef.basculerItalique() }
            Button { text: "S"; font.underline: true; font.pointSize: fenetre.tailleBase * 1.15
                     checkable: true; checked: mef.souligne; highlighted: checked; focusPolicy: Qt.NoFocus
                     Accessible.name: qsTr("Souligné"); onClicked: mef.basculerSouligne() }
            Button { text: qsTr("• Liste"); font.pointSize: fenetre.tailleBase * 1.15
                     checkable: true; checked: mef.puces; highlighted: checked; focusPolicy: Qt.NoFocus
                     Accessible.name: qsTr("Liste à puces"); onClicked: mef.basculerListe(false) }
            Button { text: qsTr("Lien…"); font.pointSize: fenetre.tailleBase * 1.15; focusPolicy: Qt.NoFocus
                     onClicked: dlgAdresseLien.open() }
            Button { text: qsTr("Image…"); font.pointSize: fenetre.tailleBase * 1.15; focusPolicy: Qt.NoFocus
                     Accessible.name: qsTr("Insérer une image"); onClicked: choixImages.open() }
        }
    }

    // Images choisies pour le corps (bouton « Image… ») : copiées parmi les
    // images collées du profil, comme une image glissée ou collée.
    FileDialog {
        id: choixImages
        title: qsTr("Insérer une image")
        fileMode: FileDialog.OpenFiles
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.webp *.bmp)")]
        onAccepted: {
            var images = []
            for (var i = 0; i < selectedFiles.length; ++i) {
                var copie = boite.importerImage(selectedFiles[i].toString())
                if (copie.length > 0)
                    images.push(copie)
            }
            if (images.length > 0)
                redac.insererImages(images)
        }
    }

    // Fichiers glissés depuis l'explorateur : joints — sauf les images
    // déposées sur le corps, qui y entrent à l'endroit du dépôt.
    DropArea {
        id: depotRedaction
        anchors.fill: parent
        onEntered: function(glisse) { glisse.accepted = glisse.hasUrls }
        onDropped: function(depose) {
            if (!depose.hasUrls)
                return
            var p = corpsRedaction.mapFromItem(depotRedaction, depose.x, depose.y)
            var surCorps = corpsRedaction.visible && p.x >= 0 && p.y >= 0
                    && p.x <= corpsRedaction.width && p.y <= corpsRedaction.height
            var images = [], autres = []
            for (var i = 0; i < depose.urls.length; ++i) {
                var copie = surCorps ? boite.importerImage(depose.urls[i].toString()) : ""
                if (copie.length > 0)
                    images.push(copie)
                else
                    autres.push(depose.urls[i])
            }
            if (images.length > 0) {
                corpsRedaction.cursorPosition = corpsRedaction.positionAt(p.x, p.y)
                redac.insererImages(images)
            }
            if (autres.length > 0)
                redac.ajouterFichiers(autres)
            depose.accept(Qt.CopyAction)
        }
    }

    Menu {
        id: menuOptions
        onAboutToShow: width = fenetre.largeurMenu(menuOptions)
        MenuItem {
            text: qsTr("Importance haute")
            checkable: true
            checked: redac.importance > 0
            onTriggered: { redac.importance = checked ? 1 : 0; redac.modifie = true }
        }
        MenuItem {
            text: qsTr("Importance basse")
            checkable: true
            checked: redac.importance < 0
            onTriggered: { redac.importance = checked ? -1 : 0; redac.modifie = true }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Demander un accusé de réception")
            checkable: true
            checked: redac.accuseRemise
            onTriggered: { redac.accuseRemise = checked; redac.modifie = true }
        }
        MenuItem {
            text: qsTr("Demander une confirmation de lecture")
            checkable: true
            checked: redac.confirmationLecture
            onTriggered: { redac.confirmationLecture = checked; redac.modifie = true }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Différer l'envoi…")
            onTriggered: dlgDiffere.open()
        }
        MenuItem {
            visible: redac.envoiDiffere > 0
            height: visible ? implicitHeight : 0
            text: qsTr("Envoyer sans attendre")
            onTriggered: { redac.envoiDiffere = 0; redac.modifie = true }
        }
    }

    // Heure d'envoi : quelques choix courants, ou une date et une heure.
    Dialog {
        id: dlgDiffere
        title: qsTr("Différer l'envoi")
        modal: true
        anchors.centerIn: parent
        width: Math.min(460, redac.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        function proposer(d) {
            champJourDiffere.text = Qt.formatDate(d, "dd/MM/yyyy")
            champHeureDiffere.text = Qt.formatTime(d, "HH:mm")
            noteDiffere.text = ""
        }
        function lire() {
            var j = champJourDiffere.text.trim().match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/)
            var h = champHeureDiffere.text.trim().match(/^(\d{1,2})[:h](\d{2})$/)
            if (!j || !h)
                return 0
            var d = new Date(Number(j[3]), Number(j[2]) - 1, Number(j[1]), Number(h[1]), Number(h[2]))
            return isNaN(d.getTime()) ? 0 : d.getTime() / 1000
        }
        onOpened: {
            var d = redac.envoiDiffere > 0 ? new Date(redac.envoiDiffere * 1000)
                                           : new Date(Date.now() + 3600 * 1000)
            proposer(d)
        }
        onAccepted: {
            var t = lire()
            if (t * 1000 <= Date.now() + 30000) {
                redac.signaler(qsTr("Heure d'envoi invalide ou déjà passée."), true)
                return
            }
            redac.envoiDiffere = t
            redac.modifie = true
        }
        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Flow {
                Layout.fillWidth: true
                spacing: 6
                Button {
                    text: qsTr("Dans une heure")
                    onClicked: dlgDiffere.proposer(new Date(Date.now() + 3600 * 1000))
                }
                Button {
                    text: qsTr("Demain 8 h")
                    onClicked: {
                        var d = new Date()
                        d.setDate(d.getDate() + 1)
                        d.setHours(8, 0, 0, 0)
                        dlgDiffere.proposer(d)
                    }
                }
                Button {
                    text: qsTr("Lundi 8 h")
                    onClicked: {
                        var d = new Date()
                        d.setDate(d.getDate() + ((8 - d.getDay()) % 7 || 7))
                        d.setHours(8, 0, 0, 0)
                        dlgDiffere.proposer(d)
                    }
                }
            }
            GridLayout {
                columns: 2
                Label { text: qsTr("Jour :") }
                TextField { id: champJourDiffere; placeholderText: "30/09/2026"; Layout.fillWidth: true }
                Label { text: qsTr("Heure :") }
                TextField { id: champHeureDiffere; placeholderText: "08:00"; Layout.fillWidth: true }
            }
            Label {
                id: noteDiffere
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: ""
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: qsTr("Le message attend sur le serveur, dans « Envoi différé ». Il part à l'heure dite si MMail est ouvert à ce moment-là, sur ce poste ou un autre ; sinon, à la prochaine ouverture.")
            }
        }
    }

    MiseEnForme {
        id: mef
        document: corpsRedaction.textDocument
        curseur: corpsRedaction.cursorPosition
        debut: corpsRedaction.selectionStart
        fin: corpsRedaction.selectionEnd
    }

    Popup {
        id: suggestions
        property var champ: null
        property var liste: []
        x: champ ? champ.mapToItem(redac, 0, 0).x : 0
        y: champ ? champ.mapToItem(redac, 0, champ.height).y : 0
        width: champ ? champ.width : 200
        padding: 1
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        contentItem: ListView {
            id: vueSuggestions
            implicitHeight: contentHeight
            clip: true
            model: suggestions.liste
            delegate: ItemDelegate {
                required property var modelData
                required property int index
                width: ListView.view.width
                highlighted: ListView.isCurrentItem
                // Sans focus : le champ garderait sinon la main… et
                // fermerait la liste avant que le clic ne compte.
                focusPolicy: Qt.NoFocus
                text: modelData.nom.length > 0 ? modelData.nom + "  —  " + modelData.adresse : modelData.adresse
                onClicked: redac.accepterSuggestion(index)
            }
        }
    }

    Dialog {
        id: dlgAdresseLien
        title: qsTr("Insérer un lien")
        modal: true
        anchors.centerIn: parent
        width: Math.min(520, redac.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: {
            champAdresseLien.text = "https://"
            champAdresseLien.forceActiveFocus()
            champAdresseLien.cursorPosition = champAdresseLien.text.length
        }
        onAccepted: mef.poserLien(champAdresseLien.text)
        ColumnLayout {
            anchors.fill: parent
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Adresse du lien, posée sur le texte sélectionné — ou insérée telle quelle s'il n'y en a pas :")
            }
            TextField {
                id: champAdresseLien
                Layout.fillWidth: true
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                onAccepted: dlgAdresseLien.accept()
            }
        }
    }

    Shortcut {
        sequences: [StandardKey.Bold]
        enabled: redac.miseEnForme
        onActivated: mef.basculerGras()
    }
    Shortcut {
        sequences: [StandardKey.Italic]
        enabled: redac.miseEnForme
        onActivated: mef.basculerItalique()
    }
    Shortcut {
        sequences: [StandardKey.Underline]
        enabled: redac.miseEnForme
        onActivated: mef.basculerSouligne()
    }

    FileDialog {
        id: choixFichiers
        title: qsTr("Joindre des fichiers")
        fileMode: FileDialog.OpenFiles
        onAccepted: redac.ajouterFichiers(selectedFiles)
    }

    Dialog {
        id: dlgSansObjet
        title: qsTr("Envoyer sans objet ?")
        modal: true
        anchors.centerIn: parent
        width: Math.min(420, redac.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: dlgSansObjet.availableWidth
            wrapMode: Text.Wrap
            text: qsTr("Ce message n'a pas d'objet.")
        }
        onAccepted: redac.envoyer(true)
    }

    Dialog {
        id: dlgFermer
        title: qsTr("Fermer ce message ?")
        modal: true
        anchors.centerIn: parent
        width: Math.min(520, redac.width - 24)
        Label {
            width: dlgFermer.availableWidth
            wrapMode: Text.Wrap
            text: qsTr("Le message a été modifié depuis son dernier enregistrement.")
        }
        footer: DialogButtonBox {
            Button {
                text: qsTr("Enregistrer le brouillon")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
            Button {
                text: qsTr("Abandonner")
                DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
            }
            Button {
                text: qsTr("Continuer")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }
        onAccepted: {
            redac.fermerApresEnregistrement = true
            redac.enregistrer()
        }
        onDiscarded: {
            dlgFermer.close()
            redac.conteneur.fermerRedaction()
        }
    }

    Shortcut {
        sequences: ["Ctrl+Return", "Ctrl+Enter"]
        onActivated: redac.envoyer(false)
    }
    Shortcut {
        sequences: [StandardKey.Save]
        onActivated: redac.enregistrer()
    }
}
