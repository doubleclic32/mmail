// SPDX-License-Identifier: GPL-3.0-or-later
// Fenêtre principale de MMail : arborescence, liste des messages, message ;
// et l'agenda (cf. Agenda.qml), à la place des trois volets.
//
// Trois volets côte à côte, dans l'ordre de lecture d'Outlook, qui sert de
// référence de conception (décision 1) : l'arborescence tient un seul volet à
// gauche, tous comptes confondus (décision 2), la rubrique Favoris au-dessus
// (décision 3). Un message se déplace vers n'importe quel dossier de n'importe
// quelle boîte, par glisser-déposer ou par menu (décisions 5 et 12).
//
// Sur un écran étroit — un téléphone —, un seul volet à la fois, et le retour
// arrière remonte d'un cran.
//
// Écrit pour Qt 6.4, comme MMdedit : ce qui s'y compile passe aussi sur le
// Qt 6.8 de l'intégration continue.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

ApplicationWindow {
    id: fenetre

    width: 1280
    height: 800
    visible: true
    title: boite.dossierCourant.length > 0
           ? "MMail — " + libelleCourant()
           : "MMail"

    Socle { id: socle }
    // Envois différés : chaque minute, les messages arrivés à échéance partent.
    Timer {
        interval: 60000
        running: true
        repeat: true
        onTriggered: boite.envoyerDifferes()
    }
    Boite { id: boite }
    // Configuration automatique du serveur, et lien de configuration.
    Configuration {
        id: configuration
        onServeurDecouvert: function(adresse, hote) { fenetre.serveurDecouvert(adresse, hote) }
        onLienLu: function(contenu) { fenetre.appliquerLien(contenu) }
        onLienEchoue: function(message) {
            var lien = fenetre.lienEnCours
            fenetre.lienEnCours = ""
            dlgLien.ouvrir(lien, qsTr("Ce lien n'a pas pu être lu — %1").arg(message))
        }
    }
    // Mise à jour assistée (demande de Manu du 10/10) : une version plus
    // récente se propose — l'installer maintenant, à la fermeture de MMail, ou
    // jamais. Le paquet est téléchargé et contrôlé par le noyau, installé par le
    // programme une fois MMail fermé (cf. cpp/installeur.h).
    MiseAJour {
        id: miseAJour
        onDisponible: function(version, page) { fenetre.versionDisponible(version, page) }
        onProgression: function(version, pourcent) {
            fenetre.progressionMiseAJour = pourcent
            if (fenetre.essai && fenetre.essai.scenario.indexOf("mise-a-jour") === 0 && pourcent >= 30
                    && !fenetre.essai.bandeauTelechargement) {
                fenetre.essai.bandeauTelechargement = true
                fenetre.capturerBandeauVersion("telechargement")
            }
        }
        onPrete: function(version, fichier) { fenetre.miseAJourPrete(version, fichier) }
        onEchec: function(version, message) { fenetre.miseAJourEchouee(version, message) }
    }
    Installeur { id: installeur }
    // Choix fait pour une version : « maintenant », « fermeture » ou « jamais ».
    // Une version plus récente se propose de nouveau.
    Settings {
        id: reglagesMiseAJour
        category: "miseAJour"
        property string version: ""
        property string choix: ""
    }
    // Avis de nouvelle version : un quart de minute après le démarrage, puis une
    // fois par jour. Coupé pour la machine par « AvisVersion » à 0.
    Timer {
        interval: 15000
        running: typeof avisVersion !== "undefined" && avisVersion
                 && !(typeof modeControle !== "undefined" && modeControle)
        repeat: true
        onTriggered: {
            if (interval === 15000)
                fenetre.nettoyerMisesAJour()
            interval = 24 * 3600 * 1000
            miseAJour.verifier()
        }
    }
    // Sauvegarde automatique : chaque minute, ce qui a changé part ; rien ne
    // part si rien n'a changé. Pas pendant une épreuve, sauf la sienne.
    Timer {
        interval: 60000
        repeat: true
        running: reglagesSauvegarde.automatique
                 && !(typeof modeControle !== "undefined" && modeControle)
                 && (!fenetre.essai || fenetre.essai.scenario === "sauvegarde")
        onTriggered: fenetre.sauvegarderConfiguration(false)
    }
    PressePapier { id: pressePapier }
    Lanceur { id: lanceur }
    Pastille { id: pastille }
    // Non-lus des boîtes de réception de tous les comptes : la pastille de
    // l'icône de la barre des tâches, le bouton « Toutes les BàL ».
    property int nonLusReceptions: 0
    onNonLusReceptionsChanged: pastille.poser(fenetre, nonLusReceptions)

    // Logiciel choisi pour ouvrir les pièces jointes, par extension (décision
    // 9) : { "pdf": "C:/…/Acrobat.exe" }, en JSON. Sur ce poste seulement.
    Settings {
        id: reglagesPieces
        category: "pieces"
        property string logiciels: "{}"
    }
    // « Ouvrir avec… » d'une seule fois : { extension, programme }, repris
    // quand le noyau a écrit la pièce.
    property var ouvertureAvec: null
    ReglePressePapier { id: reglePresse }

    Settings {
        id: reglagesEdition
        category: "edition"
        // Comme dans MMdedit : toute sélection faite dans un message part au
        // presse-papier, sauf ce qu'un autre logiciel vient d'y déposer.
        property bool copieAuto: true
    }
    readonly property string noyau: socle.noyau
    // Comptes du profil, pour le menu « Comptes » : { compte, adresse, hote }.
    property var comptesConnus: []
    // Rédactions gardées sur le poste par une session précédente.
    property var redactionsRetrouvees: []
    // Lien de configuration en cours de lecture, rendu au dialogue s'il échoue.
    property string lienEnCours: ""
    // Rôle SPECIAL-USE du dossier ouvert : « Drafts » change le double clic.
    property string roleCourant: ""
    // Dossier dont les messages se reprennent plutôt qu'on n'y répond :
    // brouillons, envois différés.
    readonly property bool dossierDeReprise: roleCourant === "Drafts" || roleCourant === "Differe"
    // Importance du message affiché, et adresse qui demande une confirmation
    // de lecture (vide si aucune, ou déjà répondu).
    property int importanceCourante: 0
    property string confirmationDemandee: ""
    // Rédactions ouvertes : jeton → contenu de la fenêtre de rédaction.
    property int compteurRedactions: 0
    property var redactions: ({})

    // Un seul volet à la fois sous cette largeur : 0 arborescence, 1 liste,
    // 2 message.
    readonly property bool compact: width < 760
    property int vue: 0
    // « courrier » ou « agenda » : ce qu'occupe la fenêtre sous la barre
    // d'outils.
    property string module: "courrier"

    // Sélection de la liste : uid → vrai. Réaffectée à chaque changement pour
    // que les liaisons qui la lisent soient réévaluées.
    property var selection: ({})
    property int ancre: -1
    property int uidCourant: 0
    // Invitation, réponse ou annulation portée par le message affiché (cf.
    // `Boite.invitationRecue`) ; et la réponse en cours d'envoi.
    property var invitation: ({})
    property string reponseInvitation: ""
    onUidCourantChanged: invitation = ({})
    property bool sourceVisible: false
    // Messages retirés de la liste en attendant la fin de leur déplacement :
    // l'interface reflète le tri tout de suite, le serveur suit (décision 16).
    property var enDeplacement: ({})
    // Recherche dans le dossier ouvert : le texte cherché (vide hors recherche),
    // et les UID que le serveur a trouvés dans le texte des messages.
    property string recherche: ""
    property var uidsServeur: []
    // Recherche dans toutes les boîtes : ses résultats remplissent la liste,
    // chacun avec son dossier. Le noyau suit le dossier du résultat choisi —
    // lecture, réponse, déplacement visent le bon —, et la sélection y est
    // simple : deux boîtes peuvent donner le même UID. On revient au dossier
    // d'avant en sortant de la recherche.
    property bool rechercheGlobale: false
    // Tri des résultats d'une recherche (demande de Manu, 11/10) : fait sur le
    // poste, jamais demandé au serveur — un tri IMAP sur un gros dossier
    // continue de tourner après qu'on l'a abandonné (constaté le 05/10).
    property string triRecherche: "recents"
    readonly property var trisRecherche: [
        { cle: "recents", libelle: qsTr("Plus récents") },
        { cle: "anciens", libelle: qsTr("Plus anciens") },
        { cle: "expediteur", libelle: qsTr("Expéditeur") },
        { cle: "objet", libelle: qsTr("Objet") },
        { cle: "taille", libelle: qsTr("Plus gros") }
    ].concat(rechercheGlobale ? [{ cle: "dossier", libelle: qsTr("Dossier") }] : [])
    property string cleChoisie: ""
    property var dossierAvantRecherche: null
    // Boîte de réception unifiée (« toutes les BàL », demande de Manu du
    // 08/10) : les réceptions de tous les comptes en une liste, sur le même
    // modèle — chaque message avec son compte, le noyau sur le dossier du
    // message choisi, sélection simple. Une recherche lancée depuis elle y
    // ramène quand on la quitte.
    property bool vueUnifiee: false
    property bool unifieeAvantRecherche: false
    readonly property bool listeMulti: rechercheGlobale || vueUnifiee
    // Version plus récente publiée, et sa page ; vide sinon.
    property string versionNouvelle: ""
    property string pageVersion: ""
    // Bandeau de mise à jour : "" (caché), "proposee", "telechargement",
    // "prete" ou "echec".
    property string etatMiseAJour: ""
    property int progressionMiseAJour: 0
    property string fichierMiseAJour: ""
    property string erreurMiseAJour: ""
    // Paquet que cette copie sait installer seule (« setup », « appimage ») ;
    // vide : la page de la version s'ouvre, comme avant la 0.5.8.
    readonly property string genreMiseAJour: installeur.genre()
    property int deplacementsEnVol: 0
    // Cumul des messages effacés par « Vider les corbeilles » (un compte par
    // signal), remis à zéro à chaque lancement.
    property int messagesVides: 0
    // Pièces jointes du message affiché, telles que le noyau les rend.
    property var pieces: []
    // Le message affiché est du HTML assaini par le noyau, rendu en texte
    // riche ; `htmlAffiche` le garde tel quel, avant la mise à l'échelle des
    // polices par le zoom de la colonne.
    property bool corpsHtml: false
    property string htmlAffiche: ""
    // Images distantes du message affiché laissées de côté.
    property int imagesBloquees: 0
    // Lien survolé dans le message : son adresse s'affiche dans la barre
    // d'information, comme dans un navigateur — un lien trompeur s'y voit.
    property string lienSurvole: ""
    // Ce que l'on glisse en ce moment (dossier ou messages) : l'arborescence,
    // où sont toutes les cibles de dépôt, défile quand il approche de son bord.
    property var glisse: null
    // Rang du message affiché dans la liste : le retrouver par son UID
    // parcourait toute la liste à chaque changement de message.
    property int indexCourant: -1
    // Rédaction qui a reçu le focus en dernier : cible de « Joindre au
    // message en cours ».
    property string redactionActive: ""
    // Rédaction qui attend une pièce jointe en cours d'écriture (« nouveau »
    // pour en ouvrir une).
    property string pieceCible: ""
    // Fond de la rubrique Favoris : un cran plus soutenu que celui des
    // comptes, pour qu'elle s'en détache (retour de Manu, 01/10).
    readonly property color fondFavoris: palette.base.hslLightness > 0.5
            ? Qt.tint(Qt.darker(palette.base, 1.07), Qt.rgba(accent.r, accent.g, accent.b, 0.10))
            : Qt.tint(palette.base, Qt.rgba(1, 1, 1, 0.08))

    // Rédaction : identité et signature de chaque compte, en JSON
    // (adresse → {nom, signature, nouveaux, reponses}) ; mise en forme par défaut.
    Settings {
        id: reglagesRedaction
        category: "redaction"
        property string identites: "{}"
        property bool miseEnForme: true
    }

    // Sauvegarde de la configuration (0.5.10) : elle part chiffrée chez le
    // service de M-Media (cf. le module `sauvegarde` du noyau) ; sur un poste
    // neuf, le premier compte ajouté la retrouve.
    Settings {
        id: reglagesSauvegarde
        category: "sauvegarde"
        // Numéro de ce poste chez le service, tiré à la première sauvegarde.
        property string poste: ""
        // Empreinte et date (ms) de la dernière sauvegarde faite.
        property string empreinte: ""
        property real date: 0
        // Profil parti de zéro : le premier compte connecté cherche une
        // sauvegarde à reprendre, et rien ne se sauvegarde avant.
        property bool aChercher: false
        property bool automatique: true
    }
    property bool rechercheSauvegarde: false
    property bool rechercheManuelle: false
    property bool sauvegardeDemandee: false
    // Sauvegardes proposées à la restauration : cf. `sauvegardesTrouvees`.
    property var sauvegardesProposees: []

    Settings {
        id: reglages
        category: "interface"
        property bool afficherMasques: false
        property bool memoriser: true
        property real largeurArborescence: 260
        property real largeurListe: 420
        // Zoom propre à chaque colonne, 1 = taille de l'apparence.
        property real zoomArborescence: 1
        property real zoomListe: 1
        property real zoomMessage: 1
        // Dernières recherches, la plus récente d'abord (JSON).
        property string historiqueRecherches: "[]"
    }

    // ---------------------------------------------------------- apparences
    //
    // Les apparences de MMdedit, reprises à l'identique : c'est la
    // palette et la police de la fenêtre qui changent, le style des contrôles
    // restant Fusion, seul à honorer une palette partout (cf. cpp/main.cpp).
    // « systeme » ne pose aucune surcharge : la palette reste celle du bureau.
    property string apparence: "moderne"

    Settings {
        id: reglagesApparence
        category: "apparence"
        property alias theme: fenetre.apparence
    }

    readonly property var jeuxApparence: ({
        "classique": {
            libelle: qsTr("Classique (Windows 9x)"),
            fond: "#d4d0c8", texte: "#000000", base: "#ffffff",
            bouton: "#d4d0c8", surbrillance: "#000080", texteSurbrillance: "#ffffff",
            police: ["MS Shell Dlg 2", "Tahoma", "Microsoft Sans Serif", "Arial"], taille: 9,
            mono: ["Courier New", "Liberation Mono", "DejaVu Sans Mono"]
        },
        "moderne": {
            libelle: qsTr("Moderne (M-Media)"),
            fond: "#f6f7f8", texte: "#231f20", base: "#ffffff",
            bouton: "#ffffff", surbrillance: "#21abe3", texteSurbrillance: "#ffffff",
            police: ["Segoe UI", "Noto Sans", "DejaVu Sans", "Arial"], taille: 10,
            mono: ["Cascadia Mono", "Consolas", "DejaVu Sans Mono", "Courier New"]
        },
        "systeme": { libelle: qsTr("Système (aspect natif)") },
        // Charte du site mmedia.fr : sélection en cyan pâle, texte nuit dessus.
        // Le texte qui prenait la couleur de sélection (objets non lus, jour
        // courant, lien survolé) prend « texteAccent », le cyan foncé, lisible
        // sur fond blanc (5,5:1) là où le cyan du site n'atteint pas 2,6:1.
        // Au-delà de la palette, les repères de marque (cf. « marque ») :
        // bandeau de l'en-tête, accent cyan des filets et des pastilles,
        // rubriques en bleu pétrole, alerte au rouge du logo, titres en
        // capitales étroites.
        "mmedia": {
            libelle: qsTr("MMEDIA"),
            fond: "#f1f2f2", texte: "#231f20", base: "#ffffff",
            bouton: "#ffffff", surbrillance: "#d6eefa", texteSurbrillance: "#0b0d2e",
            police: ["Segoe UI", "Noto Sans", "DejaVu Sans", "Arial"], taille: 10,
            mono: ["Cascadia Mono", "Consolas", "DejaVu Sans Mono", "Courier New"],
            bandeau: "#0b0d2e", texteBandeau: "#ffffff", accent: "#21abe3",
            texteAccent: "#0e6f9e", rubrique: "#276e8b", alerte: "#e61e29",
            titres: ["Saira Condensed", "Segoe UI", "Noto Sans", "DejaVu Sans", "Arial"]
        }
    })
    readonly property var nomsApparence: ["classique", "moderne", "systeme", "mmedia"]

    // Nul pour « systeme » : c'est ce qui coupe les surcharges ci-dessous.
    readonly property var jeu: apparence === "systeme" || !jeuxApparence[apparence]
                               ? null : jeuxApparence[apparence]

    // Repères de marque : nul hors d'une apparence qui porte un bandeau. Les
    // couleurs qui suivent reprennent alors la couleur de sélection et le
    // rouge d'importance d'avant, si bien que les autres apparences restent
    // telles quelles.
    readonly property var marque: jeu && jeu.bandeau ? jeu : null
    // Filets, cadres, pastilles : ce qui signale sans être du texte.
    readonly property color accent: marque ? marque.accent : palette.highlight
    // Texte mis en valeur : objets non lus, jour courant, lien survolé.
    readonly property color texteAccent: marque ? marque.texteAccent : palette.highlight
    readonly property color couleurRubrique: marque ? marque.rubrique : palette.highlight
    readonly property color couleurAlerte: marque ? marque.alerte : "#c42b1c"
    readonly property string policeTitres: marque ? premierePolice(marque.titres) : font.family
    // Les titres en capitales étroites prennent un cran de plus ; la police de
    // repli, plus large, en reste à la taille des autres apparences.
    readonly property bool titresEtroits: marque !== null && policeTitres === marque.titres[0]

    // Surcharge qui ne vaut que sous une apparence de marque : hors d'elle,
    // la propriété retrouve sa valeur d'origine, celle du style.
    component SiMarque: Binding {
        when: fenetre.marque !== null
        restoreMode: Binding.RestoreBindingOrValue
    }

    // Bouton de l'en-tête. Sous une apparence de marque, une pastille sur le
    // bandeau, comme les onglets du site : pleine en cyan pour le module
    // affiché et l'action principale, cernée pour les autres boutons à
    // bascule, nue pour le reste ; les non-lus en pastille rouge. Ailleurs,
    // le bouton d'outil du style, intact : son fond et sa couleur ne sont
    // recouverts que le temps de l'apparence.
    component BoutonEntete: ToolButton {
        id: be
        // Action principale : pleine même sans être choisie.
        property bool plein: false
        property bool cerne: false
        property int compteur: 0
        readonly property bool actif: be.checked || be.plein
        readonly property bool avecCompteur: fenetre.marque !== null && be.compteur > 0

        Rectangle {
            visible: fenetre.marque !== null
            z: -1
            anchors.fill: parent
            anchors.topMargin: 3
            anchors.bottomMargin: 3
            radius: height / 2
            color: be.actif ? fenetre.accent
                 : be.hovered || be.down ? Qt.rgba(1, 1, 1, 0.14) : "transparent"
            border.width: be.cerne && !be.actif ? 1 : 0
            border.color: Qt.rgba(1, 1, 1, 0.35)
        }
        CompteurNonLus {
            id: compteurEntete
            visible: be.avecCompteur
            nombre: be.compteur
            taille: fenetre.tailleBase
            anchors.right: parent.right
            anchors.rightMargin: be.marge - 4
            anchors.verticalCenter: parent.verticalCenter
        }
        SiMarque { target: be.background; property: "visible"; value: false }
        SiMarque {
            target: be.contentItem
            property: "color"
            value: fenetre.marque ? (be.actif ? fenetre.marque.bandeau : fenetre.marque.texteBandeau) : "black"
        }
        // Plus serrées sur un écran étroit, où l'en-tête manque déjà de place.
        readonly property real marge: fenetre.compact ? 8 : 14
        SiMarque { target: be; property: "leftPadding"; value: be.marge }
        SiMarque {
            target: be
            property: "rightPadding"
            value: be.avecCompteur ? compteurEntete.implicitWidth + be.marge + 4 : be.marge
        }
    }

    // Bouton d'action du volet de lecture. Sous une apparence de marque, une
    // pastille blanche cernée de bleu pétrole, pleine au survol ; ailleurs,
    // le bouton du style.
    component BoutonAction: Button {
        id: ba
        Rectangle {
            visible: fenetre.marque !== null
            z: -1
            anchors.fill: parent
            radius: height / 2
            color: ba.down ? Qt.darker(fenetre.couleurRubrique, 1.2)
                 : ba.hovered ? fenetre.couleurRubrique : fenetre.palette.base
            border.width: 1
            border.color: fenetre.couleurRubrique
        }
        SiMarque { target: ba.background; property: "visible"; value: false }
        SiMarque {
            target: ba.contentItem
            property: "color"
            value: ba.hovered || ba.down ? "#ffffff" : fenetre.palette.buttonText
        }
        SiMarque { target: ba; property: "leftPadding"; value: 14 }
        SiMarque { target: ba; property: "rightPadding"; value: 14 }
    }

    // Nombre de non-lus en pastille rouge, chiffre blanc.
    component CompteurNonLus: Rectangle {
        id: cn
        property int nombre: 0
        property real taille: fenetre.tailleArborescence
        radius: height / 2
        color: fenetre.couleurAlerte
        implicitHeight: chiffreCompteur.implicitHeight + 2
        implicitWidth: Math.max(implicitHeight, chiffreCompteur.implicitWidth + 10)
        width: implicitWidth
        height: implicitHeight
        Label {
            id: chiffreCompteur
            anchors.centerIn: parent
            text: cn.nombre
            color: "#ffffff"
            font.bold: true
            font.pointSize: cn.taille * 0.88
        }
    }

    // Surcharges conditionnelles plutôt qu'affectations directes : quand
    // « when » redevient faux, RestoreBindingOrValue rend la valeur d'origine —
    // c'est ce qui permet de revenir à l'apparence du bureau sans redémarrer.
    Instantiator {
        model: [
            { propriete: "palette.window", clef: "fond" },
            { propriete: "palette.base", clef: "base" },
            { propriete: "palette.text", clef: "texte" },
            { propriete: "palette.windowText", clef: "texte" },
            { propriete: "palette.button", clef: "bouton" },
            { propriete: "palette.buttonText", clef: "texte" },
            { propriete: "palette.highlight", clef: "surbrillance" },
            { propriete: "palette.highlightedText", clef: "texteSurbrillance" },
            { propriete: "font.family", clef: "police" },
            { propriete: "font.pointSize", clef: "taille" }
        ]
        delegate: Binding {
            required property var modelData
            target: fenetre
            property: modelData.propriete
            value: fenetre.jeu ? fenetre.valeurJeu(modelData.clef) : ""
            when: fenetre.jeu !== null
            restoreMode: Binding.RestoreBindingOrValue
        }
    }

    // Taille de police de l'apparence, en points, avant zoom. Celle du système
    // est relevée au démarrage : la fenêtre la perd dès qu'une apparence
    // impose la sienne.
    readonly property real tailleSysteme: Qt.application.font.pointSize > 0
                                          ? Qt.application.font.pointSize : 10
    readonly property real tailleBase: jeu ? jeu.taille : tailleSysteme
    readonly property real tailleArborescence: tailleBase * reglages.zoomArborescence
    readonly property real tailleListe: tailleBase * reglages.zoomListe
    readonly property real tailleMessage: tailleBase * reglages.zoomMessage
    readonly property string policeMono: premierePolice(jeu ? jeu.mono
                                                            : ["Consolas", "DejaVu Sans Mono", "Courier New"])

    // Polices installées, relevées une fois : une police demandée qui manque
    // cède la place à la suivante de sa liste, plutôt qu'au choix arbitraire
    // de Qt. « MS Shell Dlg 2 », par exemple, n'est qu'un alias de Windows que
    // la base de polices de Qt 6.8 ne connaît pas.
    readonly property var famillesInstallees: Qt.fontFamilies()

    function premierePolice(liste) {
        if (typeof liste === "string")
            return liste
        for (var i = 0; i < liste.length; ++i)
            if (famillesInstallees.indexOf(liste[i]) >= 0)
                return liste[i]
        return liste.length > 0 ? liste[liste.length - 1] : ""
    }

    function valeurJeu(clef) {
        var v = jeu[clef]
        return Array.isArray(v) ? premierePolice(v) : v
    }

    // --------------------------------------------------------------- zoom
    //
    // Chaque colonne a son zoom : Ctrl + molette sur elle, ou Ctrl + / Ctrl − /
    // Ctrl 0 sur la dernière colonne survolée ; au doigt, le pincement.
    property string colonneActive: "liste"
    readonly property real zoomMinimum: 0.6
    readonly property real zoomMaximum: 3

    function zoomDe(colonne) {
        if (colonne === "arborescence") return reglages.zoomArborescence
        if (colonne === "message") return reglages.zoomMessage
        return reglages.zoomListe
    }

    function poserZoom(colonne, valeur) {
        var z = Math.max(zoomMinimum, Math.min(zoomMaximum, Math.round(valeur * 100) / 100))
        if (colonne === "arborescence") reglages.zoomArborescence = z
        else if (colonne === "message") reglages.zoomMessage = z
        else reglages.zoomListe = z
        messageEtat.texte = qsTr("Zoom : %1 %").arg(Math.round(z * 100))
        return z
    }

    function zoomer(colonne, facteur) {
        return poserZoom(colonne, zoomDe(colonne) * facteur)
    }

    Connections {
        target: boite
        function onRevisionChanged() {
            fenetre.rafraichirArborescence()
            fenetre.nonLusReceptions = boite.nonLusReceptions()
            if (fenetre.vueUnifiee && fenetre.recherche.length === 0)
                fenetre.poserBoitesUnifiees()
            if (!boite.occupe)
                fenetre.derniereSynchro = new Date()
        }
        function onDossierCourantChanged() {
            fenetre.roleCourant = boite.roleCourant()
            fenetre.majInfoDossier()
        }
    }

    // ------------------------------------------------------------- en-tête
    header: ToolBar {
        id: entete
        // Sous une apparence de marque : bandeau nuit, filet cyan dessous qui
        // s'éteint vers la droite, comme l'en-tête du site.
        Rectangle {
            parent: entete
            visible: fenetre.marque !== null
            z: -1
            anchors.fill: parent
            color: fenetre.marque ? fenetre.marque.bandeau : "transparent"
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 3
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: fenetre.accent }
                    GradientStop { position: 0.7; color: fenetre.accent }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
        }
        SiMarque { target: entete.background; property: "visible"; value: false }
        SiMarque { target: entete; property: "topPadding"; value: 4 }
        SiMarque { target: entete; property: "bottomPadding"; value: 7 }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 6
            anchors.rightMargin: 6
            spacing: 6

            // Le signe et le nom du logiciel, en tête du bandeau.
            Image {
                visible: fenetre.marque !== null && !fenetre.compact && status === Image.Ready
                source: Qt.platform.os === "android" ? "" : "qrc:/assets/mmail.png"
                sourceSize.height: 28
                fillMode: Image.PreserveAspectFit
                smooth: true
                Layout.preferredHeight: 28
                Layout.preferredWidth: 28
                Layout.leftMargin: 4
            }
            Label {
                visible: fenetre.marque !== null && !fenetre.compact
                text: "MMail"
                color: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                font.family: fenetre.policeTitres
                font.weight: Font.DemiBold
                font.pointSize: fenetre.tailleBase * 1.5
                Layout.rightMargin: 12
            }

            BoutonEntete {
                visible: fenetre.compact && fenetre.vue > 0 && fenetre.module === "courrier"
                text: "‹"
                font.pixelSize: 22
                onClicked: fenetre.vue = fenetre.vue - 1
            }
            // Courrier ou agenda : deux boutons, un seul qui bascule sur un
            // écran étroit.
            BoutonEntete {
                visible: fenetre.compact
                text: fenetre.module === "agenda" ? qsTr("Courrier") : qsTr("Agenda")
                onClicked: fenetre.module = fenetre.module === "agenda" ? "courrier" : "agenda"
            }
            ButtonGroup { id: groupeModule }
            BoutonEntete {
                visible: !fenetre.compact
                cerne: true
                text: qsTr("Courrier")
                checkable: true
                checked: fenetre.module === "courrier"
                ButtonGroup.group: groupeModule
                onClicked: fenetre.module = "courrier"
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Courrier (Ctrl+1)")
            }
            BoutonEntete {
                visible: !fenetre.compact
                cerne: true
                text: qsTr("Agenda")
                checkable: true
                checked: fenetre.module === "agenda"
                ButtonGroup.group: groupeModule
                onClicked: fenetre.module = "agenda"
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Agenda (Ctrl+2)")
            }
            ToolSeparator { visible: !fenetre.compact && fenetre.marque === null }
            Item {
                visible: !fenetre.compact && fenetre.marque !== null
                Layout.preferredWidth: 8
            }
            BoutonEntete {
                plein: true
                text: fenetre.marque ? qsTr("+  Nouveau message") : qsTr("Nouveau message")
                font.bold: true
                onClicked: fenetre.rediger("nouveau")
            }
            // Boîte de réception unifiée : les réceptions de tous les comptes,
            // avec leurs non-lus (emplacement choisi par Manu le 10/10) — en
            // pastille rouge sous une apparence de marque.
            BoutonEntete {
                cerne: true
                compteur: fenetre.nonLusReceptions
                text: (fenetre.compact ? qsTr("BàL") : qsTr("Toutes les BàL"))
                      + (fenetre.marque === null && fenetre.nonLusReceptions > 0
                         ? "  (" + fenetre.nonLusReceptions + ")" : "")
                checkable: true
                checked: fenetre.vueUnifiee
                font.bold: fenetre.nonLusReceptions > 0
                onClicked: {
                    checked = Qt.binding(function() { return fenetre.vueUnifiee })
                    fenetre.ouvrirBoitesUnifiees()
                }
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: qsTr("Les boîtes de réception de tous les comptes, en une liste")
            }
            BoutonEntete {
                id: boutonComptes
                text: qsTr("Comptes")
                visible: !fenetre.compact || fenetre.vue === 0
                onClicked: menuComptes.popup(boutonComptes, 0, boutonComptes.height)
            }
            BoutonEntete {
                text: qsTr("Actualiser")
                onClicked: fenetre.module === "agenda" ? boite.synchroniserAgendas(true) : boite.actualiser()
            }
            BoutonEntete {
                visible: (!fenetre.compact || fenetre.vue === 0) && fenetre.module === "courrier"
                checkable: true
                checked: reglages.afficherMasques
                text: qsTr("Dossiers masqués")
                onToggled: {
                    reglages.afficherMasques = checked
                    fenetre.rafraichirArborescence()
                }
            }
            BoutonEntete {
                id: boutonAffichage
                text: qsTr("Affichage")
                onClicked: menuAffichage.popup(boutonAffichage, 0, boutonAffichage.height)
            }
            BoutonEntete {
                id: boutonAide
                text: "?"
                font.bold: true
                onClicked: menuAide.popup(boutonAide, 0, boutonAide.height)
            }
            Label {
                text: fenetre.compact && fenetre.vue > 0 && fenetre.module === "courrier" ? fenetre.libelleCourant() : ""
                color: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                elide: Text.ElideRight
                font.bold: true
                Layout.fillWidth: true
            }
            BusyIndicator {
                id: indicateurActivite
                running: boite.occupe || boite.agendaOccupe
                visible: running
                Layout.preferredHeight: 26
                Layout.preferredWidth: 26
                // Le style le dessine dans la teinte sombre : blanc sur le bandeau.
                SiMarque {
                    target: indicateurActivite
                    property: "palette.dark"
                    value: fenetre.marque ? fenetre.marque.texteBandeau : "black"
                }
            }
        }
    }

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

    // ------------------------------------------------------------- volets
    Agenda {
        id: vueAgenda
        anchors.fill: parent
        anchors.topMargin: bandeauVersion.height
        visible: fenetre.module === "agenda"
        boite: boite
        compact: fenetre.compact
        accent: fenetre.accent
        texteAccent: fenetre.texteAccent
        onAnnoncer: function(texte, erreur) { fenetre.annoncer(texte, erreur) }
    }

    // Rappels de l'agenda : contrôlés toutes les 30 secondes, quel que soit
    // le module affiché ; la fenêtre clignote dans la barre des tâches.
    Timer {
        interval: 30 * 1000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: fenetre.verifierRappels()
    }

    // Scénario « agenda-ecriture » : demain à 10 h, un événement créé,
    // retrouvé dans la vue, renommé, puis supprimé — chaque étape après la
    // relecture de l'agenda qui suit l'écriture.
    function etapeEcriture() {
        var e = fenetre.essai
        var demain = new Date()
        demain.setDate(demain.getDate() + 1)
        var jour = Qt.formatDate(demain, "yyyy-MM-dd")
        var trouver = function(titre) {
            var v = JSON.parse(boite.vueAgenda("jour", jour))
            return (v.fiches || []).find(function(f) { return f.titre === titre })
        }
        e.etape = e.etape || 0
        if (e.etape === 0) {
            var modifiables = JSON.parse(boite.agendas()).filter(function(a) { return a.ecriture })
            console.log("scenario: agendas modifiables", modifiables.length)
            if (modifiables.length === 0)
                return
            e.etape = 1
            boite.enregistrerEvenement(JSON.stringify({
                agenda: modifiables[0].id, titre: "Essai d'écriture MMail", lieu: "Salle 2",
                journee: false, debut: jour + "T10:00", fin: jour + "T11:00", rappel: 15
            }))
        } else if (e.etape === 1) {
            var f = trouver("Essai d'écriture MMail")
            console.log("scenario: créé", !!f, f ? f.quand + " | " + f.lieu : "")
            if (!f)
                return
            var s = JSON.parse(boite.saisieEvenement(f.objet, f.occurrence, true))
            console.log("scenario: formulaire", s.debut, s.fin, "rappel", s.rappel)
            s.titre = "Essai d'écriture MMail (modifié)"
            s.debut = jour + "T15:00"
            s.fin = jour + "T16:30"
            e.etape = 2
            boite.enregistrerEvenement(JSON.stringify(s))
        } else if (e.etape === 2) {
            var g = trouver("Essai d'écriture MMail (modifié)")
            console.log("scenario: modifié", !!g, g ? g.quand : "")
            if (!g)
                return
            e.etape = 3
            boite.supprimerEvenement(g.objet, g.occurrence, true)
        } else if (e.etape === 3) {
            e.etape = 4
            console.log("scenario: supprimé", !trouver("Essai d'écriture MMail (modifié)"))
        }
    }

    // « accepte », « refuse »… : le verbe d'une réponse à une invitation.
    function verbeReponse(statut) {
        return { "ACCEPTED": qsTr("accepte"), "DECLINED": qsTr("refuse"), "TENTATIVE": qsTr("accepte provisoirement"),
                 "DELEGATED": qsTr("délègue") }[statut] || qsTr("répond à")
    }

    function libelleReponse(statut) {
        return { "ACCEPTED": qsTr("acceptée"), "DECLINED": qsTr("refusée"), "TENTATIVE": qsTr("provisoire"),
                 "NEEDS-ACTION": qsTr("en attente") }[statut] || statut
    }

    // Ce que le bandeau dit de l'invitation au regard de l'agenda.
    function etatInvitation() {
        var i = fenetre.invitation
        var inv = i.invitation || {}
        if (!inv.methode)
            return ""
        if (i.planification === false && inv.methode === "REQUEST")
            return qsTr("Le serveur d'agenda de cette boîte ne transmet pas les réponses : répondez depuis le webmail.")
        if (inv.methode === "REPLY")
            return i.inscrite ? qsTr("Réponse inscrite dans l'agenda.")
                 : i.dansAgenda ? qsTr("Réponse déjà inscrite dans l'agenda.")
                 : qsTr("Cette réunion n'est pas dans l'agenda de cette boîte.")
        if (inv.methode === "CANCEL")
            return i.dansAgenda ? "" : qsTr("Elle n'est pas, ou plus, dans l'agenda.")
        var texte = ""
        if (i.dansAgenda && i.maReponse && i.maReponse !== "NEEDS-ACTION")
            texte = qsTr("Votre réponse : %1.").arg(libelleReponse(i.maReponse))
        else if (i.dansAgenda)
            texte = qsTr("Dans l'agenda, en attente de votre réponse.")
        if (i.sequenceAgenda > inv.sequence)
            texte += (texte.length > 0 ? " " : "") + qsTr("Une version plus récente de cette réunion est déjà dans l'agenda.")
        return texte
    }

    function donnerSuite(action) {
        var i = fenetre.invitation
        fenetre.reponseInvitation = action
        if (action === "RETIRER")
            boite.retirerAnnulee(i.compte, i.ical)
        else
            boite.repondreInvitation(i.compte, i.ical, action)
    }

    function verifierRappels() {
        var liste = JSON.parse(boite.rappelsEchus())
        popupRappels.rappels = liste
        if (liste.length === 0) {
            popupRappels.close()
            return
        }
        if (!popupRappels.opened) {
            popupRappels.open()
            fenetre.alert(0)
        }
    }

    Popup {
        id: popupRappels
        property var rappels: []
        anchors.centerIn: Overlay.overlay
        width: Math.min(fenetre.width - 32, 480)
        height: Math.min(fenetre.height - 48, implicitHeight)
        modal: false
        focus: true
        closePolicy: Popup.NoAutoClose
        padding: 16
        contentItem: ColumnLayout {
            spacing: 10
            Label {
                text: popupRappels.rappels.length > 1 ? qsTr("%1 rappels").arg(popupRappels.rappels.length) : qsTr("Rappel")
                font.bold: true
                font.pixelSize: 17
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                implicitHeight: Math.min(contentHeight, 320)
                clip: true
                spacing: 8
                model: popupRappels.rappels
                delegate: RowLayout {
                    id: ligneRappel
                    required property var modelData
                    width: ListView.view.width
                    spacing: 8
                    Rectangle {
                        Layout.preferredWidth: 10
                        Layout.preferredHeight: 10
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: 5
                        radius: 3
                        color: ligneRappel.modelData.couleur || fenetre.accent
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Label {
                            Layout.fillWidth: true
                            text: ligneRappel.modelData.titre
                            font.bold: true
                            wrapMode: Text.Wrap
                        }
                        Label {
                            Layout.fillWidth: true
                            text: ligneRappel.modelData.quand
                                  + (ligneRappel.modelData.lieu.length > 0 ? " — " + ligneRappel.modelData.lieu : "")
                            wrapMode: Text.Wrap
                            opacity: 0.8
                        }
                    }
                    Button {
                        text: qsTr("Ouvrir")
                        flat: true
                        onClicked: {
                            fenetre.module = "agenda"
                            vueAgenda.allerAuJour(ligneRappel.modelData.date)
                        }
                    }
                    Button {
                        text: qsTr("Ignorer")
                        onClicked: {
                            boite.rappelVu(ligneRappel.modelData.cle)
                            fenetre.verifierRappels()
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Répéter dans") }
                ComboBox {
                    id: delaiRappel
                    model: [qsTr("5 minutes"), qsTr("10 minutes"), qsTr("15 minutes"), qsTr("30 minutes"), qsTr("1 heure")]
                    readonly property var minutes: [5, 10, 15, 30, 60]
                }
                Button {
                    text: qsTr("Répéter")
                    onClicked: {
                        for (var i = 0; i < popupRappels.rappels.length; ++i)
                            boite.repousserRappel(popupRappels.rappels[i].cle, delaiRappel.minutes[delaiRappel.currentIndex])
                        fenetre.verifierRappels()
                    }
                }
                Item { Layout.fillWidth: true }
                Button {
                    visible: popupRappels.rappels.length > 1
                    text: qsTr("Tout ignorer")
                    onClicked: {
                        for (var i = 0; i < popupRappels.rappels.length; ++i)
                            boite.rappelVu(popupRappels.rappels[i].cle)
                        fenetre.verifierRappels()
                    }
                }
            }
        }
    }

    // Agendas tenus à jour en arrière-plan, quel que soit le module affiché :
    // un quart d'heure, et peu après la connexion d'un compte.
    Timer {
        id: minuteurAgendas
        interval: 15 * 60 * 1000
        repeat: true
        running: true
        onTriggered: {
            interval = 15 * 60 * 1000
            boite.synchroniserAgendas(false)
        }
    }

    SplitView {
        id: volets
        anchors.fill: parent
        anchors.topMargin: bandeauVersion.height
        orientation: Qt.Horizontal
        visible: fenetre.module === "courrier"

        // --- arborescence -------------------------------------------------
        // Deux listes : la rubrique Favoris, fixée en haut — elle reste à
        // l'écran quand les comptes défilent (demande de Manu du 01/10) —,
        // puis les comptes, seuls à défiler.
        ColumnLayout {
            id: colonneArborescence
            visible: !fenetre.compact || fenetre.vue === 0
            SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurArborescence
            SplitView.minimumWidth: 160
            SplitView.fillWidth: fenetre.compact
            spacing: 0
            onWidthChanged: if (!fenetre.compact && visible) reglages.largeurArborescence = width

            // Les Favoris ne défilent qu'au-delà de 40 % de la hauteur.
            ScrollView {
                id: voletFavoris
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(vueFavoris.contentHeight, colonneArborescence.height * 0.4)
                ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
                clip: true
                contentWidth: availableWidth
                font.pointSize: fenetre.tailleArborescence
                background: Rectangle { color: fenetre.fondFavoris }

                ListView {
                    id: vueFavoris
                    model: ListModel { id: modeleFavoris }
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: ligneArborescence

                    HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "arborescence" }
                    WheelHandler {
                        acceptedModifiers: Qt.ControlModifier
                        onWheel: function(roue) {
                            fenetre.zoomer("arborescence", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                        }
                    }
                }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: fenetre.palette.windowText
                opacity: 0.25
            }

            ScrollView {
                id: voletArborescence
                Layout.fillWidth: true
                Layout.fillHeight: true
                // Ascenseurs toujours visibles dès qu'il y a de quoi défiler, et non
                // seulement pendant le défilement (demande Manu du 2026-09-29). Leur
                // place est réservée : posés par-dessus, ils masquaient les compteurs
                // de non-lus. La réserve suit `size`, pas `visible` : un ascenseur
                // qui n'a rien à faire défiler reste `visible`, seulement transparent.
                ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
                clip: true
                contentWidth: availableWidth
                font.pointSize: fenetre.tailleArborescence
                background: Rectangle { color: fenetre.palette.base }

                // Les gestionnaires vivent dans la liste, pas dans le ScrollView :
                // celui-ci ne reconnaît sa liste comme contenu défilant que si elle
                // est son seul enfant — sinon elle prend une largeur nulle.
                ListView {
                    id: vueArborescence
                    model: ListModel { id: modeleArborescence }
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: ligneArborescence
                    // Toutes les lignes restent construites : un dossier qu'on glisse
                    // vers les Favoris fait défiler la liste, et sa ligne d'origine,
                    // qui porte le glisser, ne doit pas être détruite en sortant de
                    // l'écran — le glisser serait annulé.
                    // Seulement pendant un glisser : hors de là, garder toutes
                    // les lignes construites coûtait mémoire et reconstructions
                    // pour rien.
                    cacheBuffer: fenetre.glisse !== null ? 100000 : 400

                    // Défilement au bord pendant un glisser-déposer (retour de Manu,
                    // 01/10 : les Favoris, en haut, étaient hors d'atteinte).
                    Timer {
                        interval: 30
                        repeat: true
                        running: fenetre.glisse !== null
                        onTriggered: {
                            var g = fenetre.glisse
                            if (!g)
                                return
                            var p = g.mapToItem(vueArborescence, g.width / 2, g.height / 2)
                            if (p.x < 0 || p.x > vueArborescence.width)
                                return
                            var bord = 40
                            var pas = 0
                            // Plus vite à mesure qu'on s'approche du bord, ou qu'on le passe.
                            if (p.y < bord && p.y > -2 * bord)
                                pas = -Math.max(4, (bord - p.y) / 2)
                            else if (p.y > vueArborescence.height - bord && p.y < vueArborescence.height + 2 * bord)
                                pas = Math.max(4, (p.y - vueArborescence.height + bord) / 2)
                            if (pas === 0)
                                return
                            var haut = vueArborescence.originY
                            var bas = Math.max(haut, haut + vueArborescence.contentHeight - vueArborescence.height)
                            var y = Math.min(bas, Math.max(haut, vueArborescence.contentY + pas))
                            if (y === vueArborescence.contentY)
                                return
                            vueArborescence.contentY = y
                            // Le contenu a bougé sous un pointeur immobile : la cible
                            // du dépôt est réévaluée.
                            g.y += 0.5
                            g.y -= 0.5
                        }
                    }

                    // Un blanc et un trait au-dessus de chaque compte.
                    section.property: "groupe"
                    section.delegate: Item {
                        required property string section
                        width: vueArborescence.width
                        height: 16
                        Rectangle {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.leftMargin: 6
                            anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            height: 2
                            color: fenetre.palette.windowText
                            opacity: 0.3
                        }
                    }

                    HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "arborescence" }

                    WheelHandler {
                        acceptedModifiers: Qt.ControlModifier
                        onWheel: function(roue) {
                            fenetre.zoomer("arborescence", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                        }
                    }
                    PinchHandler {
                        target: null
                        property real depart: 1
                        onActiveChanged: if (active) depart = reglages.zoomArborescence
                        onActiveScaleChanged: if (active) fenetre.poserZoom("arborescence", depart * activeScale)
                    }
                }
            }
        }

        // --- liste des messages -------------------------------------------
        ColumnLayout {
            id: colonneListe
            visible: !fenetre.compact || fenetre.vue === 1
            SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurListe
            SplitView.minimumWidth: 240
            SplitView.fillWidth: fenetre.compact
            spacing: 0
            onWidthChanged: if (!fenetre.compact && visible) reglages.largeurListe = width

            // Recherche, comme dans Outlook : dans le dossier ouvert (objet,
            // expéditeur, puis texte entier par le serveur), ou dans toutes les
            // boîtes (index local).
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 4
                spacing: 4
                TextField {
                    id: champRecherche
                    Layout.fillWidth: true
                    placeholderText: qsTr("Rechercher (Ctrl+E)")
                    selectByMouse: true
                    // Mis en valeur (demande de Manu, 08/10) : une loupe, un
                    // cadre de la couleur d'accent sur un fond teinté ; le cadre
                    // s'épaissit quand le champ a la main.
                    leftPadding: loupe.width + 13
                    background: Rectangle {
                        implicitHeight: 30
                        radius: fenetre.marque ? height / 2 : 4
                        color: champRecherche.activeFocus ? fenetre.palette.base
                               : Qt.rgba(fenetre.accent.r, fenetre.accent.g, fenetre.accent.b, 0.08)
                        border.width: champRecherche.activeFocus ? 2 : 1
                        border.color: fenetre.accent
                    }
                    Canvas {
                        id: loupe
                        anchors.left: parent.left
                        anchors.leftMargin: 7
                        anchors.verticalCenter: parent.verticalCenter
                        height: Math.round(parent.height * 0.5)
                        width: height
                        property color encre: fenetre.accent
                        onEncreChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onPaint: fenetre.dessinerLoupe(getContext("2d"), width, height, encre)
                    }
                    // La frappe ne cherche plus : elle propose les dernières
                    // recherches, que l'on choisit ; Entrée lance la recherche
                    // (demande de Manu, 09/10).
                    onActiveFocusChanged: if (activeFocus) fenetre.proposerHistorique()
                    onTextEdited: fenetre.proposerHistorique()
                    onAccepted: {
                        if (panneauHistorique.opened && vueHistorique.currentIndex >= 0)
                            fenetre.lancerRecherche(panneauHistorique.entrees[vueHistorique.currentIndex])
                        else
                            fenetre.lancerRecherche()
                    }
                    Keys.onDownPressed: function(ev) {
                        if (panneauHistorique.opened)
                            vueHistorique.incrementCurrentIndex()
                        else
                            fenetre.proposerHistorique()
                    }
                    Keys.onUpPressed: if (panneauHistorique.opened) vueHistorique.decrementCurrentIndex()
                    Keys.onEscapePressed: {
                        if (panneauHistorique.opened)
                            panneauHistorique.close()
                        else
                            fenetre.quitterRecherche()
                    }
                    // Dernières recherches sous le champ : dix visibles au plus,
                    // un ascenseur au-delà.
                    Popup {
                        id: panneauHistorique
                        property var entrees: []
                        y: champRecherche.height + 2
                        width: champRecherche.width
                        padding: 1
                        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                        onAboutToShow: vueHistorique.currentIndex = -1
                        background: Rectangle {
                            color: fenetre.palette.base
                            border.color: fenetre.accent
                            radius: 3
                        }
                        contentItem: ListView {
                            id: vueHistorique
                            implicitHeight: Math.min(panneauHistorique.entrees.length, 10) * 30
                            clip: true
                            model: panneauHistorique.entrees
                            currentIndex: -1
                            ScrollBar.vertical: ScrollBar {
                                policy: panneauHistorique.entrees.length > 10 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                            }
                            delegate: ItemDelegate {
                                width: ListView.view.width
                                height: 30
                                text: modelData
                                highlighted: ListView.isCurrentItem
                                onClicked: fenetre.lancerRecherche(modelData)
                            }
                        }
                    }
                }
                ComboBox {
                    id: porteeRecherche
                    readonly property bool partout: currentIndex === 1
                    model: [qsTr("Dossier actif"), qsTr("Toutes les boîtes")]
                    implicitContentWidthPolicy: ComboBox.WidestText
                    // Le champ s'étire, la portée garde sa largeur : « Toutes les
                    // boîtes » était tronqué.
                    Layout.minimumWidth: Math.max(implicitWidth, 150)
                    onActivated: if (champRecherche.text.trim().length > 0) fenetre.lancerRecherche()
                }
                ToolButton {
                    text: "✕"
                    visible: champRecherche.text.length > 0 || fenetre.recherche.length > 0
                    onClicked: fenetre.quitterRecherche()
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Effacer la recherche (Échap)")
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 8
                Layout.rightMargin: 4
                Layout.bottomMargin: 4
                spacing: 6
                visible: fenetre.recherche.length > 0
                Label { text: qsTr("Trier par") }
                ComboBox {
                    id: choixTri
                    model: fenetre.trisRecherche
                    textRole: "libelle"
                    implicitContentWidthPolicy: ComboBox.WidestText
                    function suivre() {
                        var i = fenetre.trisRecherche.findIndex(function(t) { return t.cle === fenetre.triRecherche })
                        if (i < 0) {
                            // « Dossier » n'a plus cours hors de toutes les boîtes.
                            fenetre.triRecherche = "recents"
                            i = 0
                        }
                        currentIndex = i
                    }
                    onModelChanged: suivre()
                    Component.onCompleted: suivre()
                    onActivated: fenetre.trierRecherche(fenetre.trisRecherche[currentIndex].cle)
                }
                Item { Layout.fillWidth: true }
            }

            ScrollView {
                id: voletListe
                Layout.fillWidth: true
                Layout.fillHeight: true
                ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
                clip: true
                contentWidth: availableWidth
                font.pointSize: fenetre.tailleListe
                background: Rectangle { color: fenetre.palette.base }

                ListView {
                    id: vueMessages
                    model: ListModel {
                        id: modeleMessages
                        // La vue ne connaît que les champs présents à son premier
                        // remplissage : ceux des résultats d'une recherche dans
                        // toutes les boîtes (dossier de chaque message) sont donc
                        // déclarés d'avance, puis la ligne est retirée.
                        ListElement {
                            uid: 0; h: 0; expediteur: ""; adresse: ""; sujet: ""; date: ""; taille: 0
                            lu: false; repondu: false; reponduTous: false; transfere: false; pieces: false
                            suivi: false; importance: 0
                            compte: 0; chemin: ""; dossier: ""; role: ""; cheminAffiche: ""
                        }
                        Component.onCompleted: clear()
                    }
                    // Les lignes qui sortent de l'écran servent à celles qui entrent,
                    // au lieu d'être détruites et recréées à chaque défilement.
                    reuseItems: true
                    boundsBehavior: Flickable.StopAtBounds
                    currentIndex: -1
                    focus: true
                    keyNavigationEnabled: false
                    delegate: ligneMessage

                    Keys.onUpPressed: function(touche) {
                        fenetre.deplacerCurseur(-1, touche.modifiers & Qt.ShiftModifier)
                    }
                    Keys.onDownPressed: function(touche) {
                        fenetre.deplacerCurseur(1, touche.modifiers & Qt.ShiftModifier)
                    }

                    HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "liste" }

                    WheelHandler {
                        acceptedModifiers: Qt.ControlModifier
                        onWheel: function(roue) {
                            fenetre.zoomer("liste", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                        }
                    }
                    PinchHandler {
                        target: null
                        property real depart: 1
                        onActiveChanged: if (active) depart = reglages.zoomListe
                        onActiveScaleChanged: if (active) fenetre.poserZoom("liste", depart * activeScale)
                    }

                    Label {
                        anchors.centerIn: parent
                        visible: modeleMessages.count === 0
                        opacity: 0.6
                        text: boite.dossierCourant.length === 0
                              ? qsTr("Choisissez un dossier.")
                              : qsTr("Aucun message.")
                    }
                }
            }

        }

        // --- message ------------------------------------------------------
        ColumnLayout {
            visible: !fenetre.compact || fenetre.vue === 2
            SplitView.fillWidth: true
            spacing: 0

            HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "message" }

            ToolBar {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0
                font.pointSize: fenetre.tailleMessage
                leftPadding: 8
                rightPadding: 8
                // Les actions sur leur ligne, au-dessus de l'objet, comme dans
                // le volet de lecture d'Outlook : à côté, elles tronquaient
                // l'objet et l'expéditeur. Elles passent à la ligne si la
                // colonne est étroite.
                ColumnLayout {
                    width: parent.width
                    spacing: 0
                    // De vrais boutons, encadrés et espacés, en trois groupes —
                    // répondre, ranger, voir la source — : des boutons plats de
                    // barre d'outils se lisaient mal (retour de Manu, 01/10).
                    Flow {
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        Layout.bottomMargin: 6
                        spacing: 6
                        // Un brouillon se reprend ; un message reçu reçoit une réponse.
                        BoutonAction {
                            visible: fenetre.dossierDeReprise
                            text: qsTr("Reprendre")
                            onClicked: fenetre.rediger("brouillon")
                        }
                        BoutonAction {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Répondre")
                            onClicked: fenetre.rediger("repondre")
                        }
                        BoutonAction {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Répondre à tous")
                            onClicked: fenetre.rediger("repondre_tous")
                        }
                        BoutonAction {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Transférer")
                            onClicked: fenetre.rediger("transferer")
                        }
                        ToolSeparator { height: boutonDeplacer.height }
                        BoutonAction {
                            id: boutonDeplacer
                            text: qsTr("Déplacer…")
                            onClicked: fenetre.ouvrirDeplacer()
                        }
                        BoutonAction {
                            visible: !fenetre.dossierDeReprise
                            text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
                            onClicked: fenetre.signalerIndesirable()
                            ToolTip.visible: hovered
                            ToolTip.delay: 600
                            ToolTip.text: fenetre.roleCourant === "Junk"
                                          ? qsTr("Remettre en boîte de réception ; le filtre du serveur apprend que le message est légitime (Ctrl+Alt+J).")
                                          : qsTr("Classer en courrier indésirable ; le filtre du serveur l'apprend (Ctrl+Alt+J).")
                        }
                        BoutonAction {
                            text: qsTr("Supprimer")
                            onClicked: fenetre.supprimerSelection()
                        }
                        ToolSeparator { height: boutonDeplacer.height }
                        // Décision 6 : voir le message brut, en-têtes compris, en un geste.
                        BoutonAction {
                            text: fenetre.sourceVisible ? qsTr("Message") : qsTr("Source")
                            onClicked: fenetre.basculerSource()
                        }
                    }
                    // L'objet, en titre dans la police des titres sous une
                    // apparence de marque.
                    Label {
                        id: sujetAffiche
                        font.bold: true
                        font.family: fenetre.marque ? fenetre.policeTitres : fenetre.font.family
                        font.pointSize: fenetre.tailleMessage * (fenetre.titresEtroits ? 1.4
                                                                 : fenetre.marque ? 1.2 : 1)
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        Layout.topMargin: fenetre.marque ? 2 : 0
                    }
                    Label {
                        id: auteurAffiche
                        opacity: 0.8
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: fenetre.uidCourant > 0 && fenetre.importanceCourante !== 0
                        text: fenetre.importanceCourante > 0 ? qsTr("Importance haute") : qsTr("Importance basse")
                        color: fenetre.importanceCourante > 0 ? fenetre.couleurAlerte : "#1a4480"
                        font.bold: true
                    }
                }
            }

            // L'expéditeur demande une confirmation de lecture : on la propose,
            // sans l'envoyer d'office — comme Outlook.
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.confirmationDemandee.length > 0
                padding: 4
                background: Rectangle { color: "#fff8e1" }
                RowLayout {
                    width: parent.width
                    Label {
                        text: qsTr("L'expéditeur demande une confirmation de lecture (à %1).").arg(fenetre.confirmationDemandee)
                        color: "#1c1f24"
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Envoyer")
                        onClicked: {
                            boite.repondreConfirmation(fenetre.uidCourant, true)
                            fenetre.confirmationDemandee = ""
                            messageEtat.texte = qsTr("Confirmation de lecture envoyée.")
                        }
                    }
                    Button {
                        text: qsTr("Ignorer")
                        onClicked: {
                            boite.repondreConfirmation(fenetre.uidCourant, false)
                            fenetre.confirmationDemandee = ""
                        }
                    }
                }
            }

            // Invitation, réponse ou annulation portée par le message : le
            // bandeau d'Outlook, avec de quoi y donner suite. Le serveur
            // d'agenda (SOGo) envoie lui-même la réponse à l'organisateur.
            Pane {
                id: bandeauInvitation
                readonly property var inv: fenetre.invitation.invitation || ({})
                readonly property string methode: inv.methode || ""
                readonly property var organisateur: inv.organisateur || ({})
                readonly property var repondant: (inv.participants || [])[0] || ({})
                readonly property bool active: fenetre.invitation.planification !== false
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && methode.length > 0
                font.pointSize: fenetre.tailleMessage
                padding: 8
                background: Rectangle {
                    color: Qt.tint(fenetre.palette.base, "#263a7bd5")
                    border.color: "#3a7bd5"
                    border.width: 1
                    radius: 3
                }
                ColumnLayout {
                    width: parent.width
                    spacing: 3
                    Label {
                        Layout.fillWidth: true
                        font.bold: true
                        wrapMode: Text.Wrap
                        text: bandeauInvitation.methode === "CANCEL" ? qsTr("Réunion annulée : %1").arg(bandeauInvitation.inv.titre || "")
                              : bandeauInvitation.methode === "REPLY"
                                ? qsTr("Réponse de %1 : %2 « %3 »").arg(bandeauInvitation.repondant.nom || bandeauInvitation.repondant.adresse || "")
                                      .arg(fenetre.verbeReponse(bandeauInvitation.repondant.statut)).arg(bandeauInvitation.inv.titre || "")
                              : qsTr("Invitation : %1").arg(bandeauInvitation.inv.titre || "")
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: (bandeauInvitation.inv.quand || "") + (bandeauInvitation.inv.repete ? qsTr(" — se répète") : "")
                              + ((bandeauInvitation.inv.lieu || "").length > 0 ? " — " + bandeauInvitation.inv.lieu : "")
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: bandeauInvitation.methode !== "REPLY" && (bandeauInvitation.organisateur.adresse || "").length > 0
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        text: qsTr("Organisateur : %1").arg(bandeauInvitation.organisateur.nom
                              ? bandeauInvitation.organisateur.nom + " <" + bandeauInvitation.organisateur.adresse + ">"
                              : bandeauInvitation.organisateur.adresse || "")
                              + ((bandeauInvitation.inv.participants || []).length > 0
                                 ? qsTr(" — %1").arg(fenetre.accord(bandeauInvitation.inv.participants.length, qsTr("participant"), qsTr("participants"))) : "")
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.8
                        text: fenetre.etatInvitation()
                        visible: text.length > 0
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        visible: bandeauInvitation.methode === "REQUEST" || (bandeauInvitation.methode === "CANCEL" && fenetre.invitation.dansAgenda)
                        Repeater {
                            model: bandeauInvitation.methode === "REQUEST"
                                   ? [["ACCEPTED", qsTr("Accepter")], ["TENTATIVE", qsTr("Provisoire")], ["DECLINED", qsTr("Refuser")]]
                                   : [["RETIRER", qsTr("Retirer de l'agenda")]]
                            delegate: Button {
                                required property var modelData
                                text: modelData[1]
                                enabled: bandeauInvitation.active && fenetre.reponseInvitation.length === 0
                                highlighted: fenetre.invitation.maReponse === modelData[0]
                                onClicked: fenetre.donnerSuite(modelData[0])
                            }
                        }
                        BusyIndicator {
                            running: fenetre.reponseInvitation.length > 0
                            visible: running
                            height: 28
                            width: 28
                        }
                    }
                }
            }

            // Pièces jointes du message affiché, mises en valeur sous l'en-tête :
            // une ligne par pièce — type, nom, taille, « Ouvrir », « Enregistrer
            // sous… » (forme choisie par Manu le 09/10 sur maquettes). Clic droit,
            // ou appui long : ouvrir avec un autre logiciel, joindre à un message.
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.pieces.length > 0
                font.pointSize: fenetre.tailleMessage
                padding: 6
                background: Rectangle { color: fenetre.palette.window }
                ColumnLayout {
                    width: parent.width
                    spacing: 2
                    Label {
                        text: qsTr("Pièces jointes (%1)").arg(fenetre.pieces.length)
                        font.bold: true
                    }
                    Repeater {
                        model: fenetre.pieces
                        delegate: RowLayout {
                            id: lignePiece
                            required property var modelData
                            readonly property var genre: fenetre.genrePiece(modelData.nom)
                            readonly property string logiciel: Qt.platform.os === "android" ? ""
                                                               : fenetre.logicielPour(modelData.nom)
                            Layout.fillWidth: true
                            spacing: 8
                            TapHandler {
                                acceptedButtons: Qt.RightButton
                                onTapped: menuPiece.ouvrir(lignePiece.modelData, lignePiece)
                            }
                            TapHandler {
                                acceptedDevices: PointerDevice.TouchScreen
                                onLongPressed: menuPiece.ouvrir(lignePiece.modelData, lignePiece)
                            }
                            // Étiquette du type, en couleur : PDF, XLSX, JPG…
                            Rectangle {
                                implicitWidth: 38
                                implicitHeight: 20
                                radius: 3
                                color: lignePiece.genre.couleur
                                Label {
                                    anchors.centerIn: parent
                                    text: lignePiece.genre.libelle
                                    color: "white"
                                    font.bold: true
                                    font.pointSize: fenetre.tailleMessage * 0.6
                                }
                            }
                            Label {
                                text: lignePiece.modelData.nom
                                elide: Text.ElideMiddle
                                Layout.fillWidth: true
                            }
                            Label {
                                text: fenetre.tailleLisible(lignePiece.modelData.taille)
                                opacity: 0.65
                            }
                            Button {
                                text: qsTr("Ouvrir")
                                flat: true
                                enabled: !lignePiece.modelData.risquee
                                onClicked: fenetre.ouvrirPiece(lignePiece.modelData)
                                ToolTip.visible: hovered
                                ToolTip.delay: 500
                                ToolTip.text: lignePiece.logiciel.length > 0
                                              ? qsTr("Avec %1, choisi pour les fichiers .%2 (clic droit : ouvrir avec…)")
                                                .arg(lanceur.nom(lignePiece.logiciel)).arg(fenetre.extensionDe(lignePiece.modelData.nom))
                                              : qsTr("Avec le logiciel du système (clic droit : ouvrir avec…)")
                            }
                            Button {
                                text: qsTr("Enregistrer sous…")
                                flat: true
                                // Sous Android, le dialogue rend une adresse
                                // « content:// » que le noyau ne sait pas écrire.
                                visible: Qt.platform.os !== "android"
                                onClicked: fenetre.enregistrerPiece(lignePiece.modelData)
                                ToolTip.visible: hovered && lignePiece.modelData.risquee
                                ToolTip.text: qsTr("Programme ou script : il s'enregistre, il ne s'ouvre pas depuis MMail.")
                            }
                        }
                    }
                }
            }

            // Images distantes : rien ne se télécharge sans le demander — une
            // image distante dit à l'expéditeur que le message a été ouvert.
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.corpsHtml
                         && fenetre.imagesBloquees > 0
                font.pointSize: fenetre.tailleMessage
                padding: 4
                background: Rectangle { color: "#e8f0fb" }
                RowLayout {
                    width: parent.width
                    Label {
                        text: (fenetre.imagesBloquees > 1
                               ? qsTr("%1 images distantes non téléchargées").arg(fenetre.imagesBloquees)
                               : qsTr("1 image distante non téléchargée"))
                              + qsTr(" : l'expéditeur saurait que vous avez ouvert le message.")
                        color: "#1c1f24"
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Télécharger les images")
                        onClicked: {
                            messageEtat.texte = qsTr("Téléchargement des images…")
                            boite.afficherImages(fenetre.uidCourant)
                        }
                    }
                }
            }

            ScrollView {
                id: cadreCorps
                Layout.fillWidth: true
                Layout.fillHeight: true
                ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                // Réserve fixe : le texte se replie, sa hauteur dépend donc de la
                // largeur, et une réserve qui suivrait le débordement bouclerait.
                rightPadding: ScrollBar.vertical.width
                // La source n'est pas repliée : elle défile aussi en largeur.
                ScrollBar.horizontal.policy: ScrollBar.horizontal.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                bottomPadding: fenetre.sourceVisible ? ScrollBar.horizontal.height : 0
                clip: true
                contentWidth: fenetre.sourceVisible ? -1 : availableWidth

                TextArea {
                    id: vueCorps
                    readOnly: true
                    selectByMouse: true
                    // Un message HTML en texte riche ; tout le reste — source,
                    // texte brut, attente — en texte simple, où une balise
                    // reste une balise.
                    textFormat: fenetre.corpsHtml && !fenetre.sourceVisible ? TextEdit.RichText : TextEdit.PlainText
                    wrapMode: fenetre.sourceVisible ? TextEdit.NoWrap : TextEdit.Wrap
                    font.pointSize: fenetre.tailleMessage
                    font.family: fenetre.sourceVisible ? fenetre.policeMono : fenetre.font.family
                    // Un message HTML se lit sur fond blanc, quelle que soit
                    // l'apparence : ses couleurs supposent ce fond.
                    color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#1f1f1f" : fenetre.palette.text
                    background: Rectangle {
                        color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#ffffff" : fenetre.palette.base
                    }
                    text: qsTr("Aucun message sélectionné.")

                    onLinkActivated: function(lien) { fenetre.ouvrirLien(lien) }
                    onLinkHovered: function(lien) { fenetre.lienSurvole = lien }

                    // Une image plus large que la colonne y est ramenée, et
                    // suit la colonne quand elle change de largeur.
                    MiseEnForme {
                        id: mefLecture
                        document: vueCorps.textDocument
                    }
                    onWidthChanged: minuterieImages.restart()
                    Timer {
                        id: minuterieImages
                        interval: 120
                        onTriggered: fenetre.bornerImages()
                    }
                    HoverHandler {
                        cursorShape: vueCorps.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
                    }

                    onSelectedTextChanged: if (selectedText.length > 0) fenetre.selectionChangee()
                    // Un Ctrl+C explicite écrit par la même règle que la copie
                    // automatique : sans cela, la règle y verrait un dépôt venu
                    // d'ailleurs, et se protégerait contre MMail lui-même.
                    Keys.onPressed: function(touche) {
                        if (touche.matches(StandardKey.Copy) && selectedText.length > 0) {
                            fenetre.copierTexte(selectedText, fenetre.sourceVisible ? "source" : "message")
                            touche.accepted = true
                        }
                    }
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: menuCorps.popup()
                    }
                    onPressAndHold: if (fenetre.compact) menuCorps.popup()

                    WheelHandler {
                        acceptedModifiers: Qt.ControlModifier
                        onWheel: function(roue) {
                            fenetre.zoomer("message", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                        }
                    }
                    PinchHandler {
                        target: null
                        property real depart: 1
                        onActiveChanged: if (active) depart = reglages.zoomMessage
                        onActiveScaleChanged: if (active) fenetre.poserZoom("message", depart * activeScale)
                    }
                }
            }
        }
    }

    // ------------------------------------------------------ barre d'information
    //
    // À gauche, ce qui est ouvert et ce qu'il contient ; au milieu, le dernier
    // message de l'application, qui s'efface de lui-même ; à droite, l'état du
    // presse-papier, des déplacements et de la synchronisation.
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 14
            Label {
                text: fenetre.lienSurvole.length > 0 ? fenetre.lienSurvole : fenetre.infoDossier
                color: fenetre.lienSurvole.length > 0 ? fenetre.texteAccent : fenetre.palette.windowText
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.fillWidth: true
                Layout.minimumWidth: 120
            }
            Label {
                visible: text.length > 0
                text: fenetre.passager
                color: fenetre.passagerErreur ? "#b00020" : palette.windowText
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.maximumWidth: fenetre.width * 0.4
            }
            Label {
                visible: reglePresse.restant > 0
                text: qsTr("presse-papier protégé %1 s").arg(reglePresse.restant)
                opacity: 0.75
            }
            Label {
                visible: boite.enAttente > 0
                text: fenetre.accord(boite.enAttente, qsTr("déplacement en attente"), qsTr("déplacements en attente"))
            }
            Label {
                text: fenetre.etatSynchro()
                opacity: 0.75
            }
        }
    }

    // Message passager : il s'efface au bout de quelques secondes — plus
    // longtemps pour une erreur.
    property string passager: ""
    property bool passagerErreur: false
    property string infoDossier: ""
    property var derniereSynchro: null

    Timer {
        id: minuteurPassager
        onTriggered: fenetre.passager = ""
    }

    // ---------------------------------------------------------- mise à jour
    function texteMiseAJour() {
        var v = versionNouvelle
        var autorisation = installeur.pourTousLesUtilisateurs()
                ? " " + qsTr("Windows demandera l'autorisation d'un administrateur.") : ""
        switch (etatMiseAJour) {
        case "proposee":
            if (genreMiseAJour.length === 0)
                return qsTr("Une nouvelle version de MMail est disponible : %1 (vous utilisez la %2).")
                       .arg(v).arg(miseAJour.versionActuelle())
            return qsTr("MMail %1 est disponible — vous utilisez la %2 (<a href=\"%3\">nouveautés</a>). L'installer :")
                   .arg(v).arg(miseAJour.versionActuelle()).arg(pageVersion) + autorisation
        case "telechargement":
            return qsTr("Téléchargement de MMail %1… %2 %").arg(v).arg(progressionMiseAJour)
        case "prete":
            if (genreMiseAJour === "appimage")
                return qsTr("MMail %1 est installé : redémarrez MMail pour l'utiliser.").arg(v)
            return qsTr("MMail %1 est prêt : redémarrez MMail pour l'installer. Les rédactions en cours sont gardées.")
                   .arg(v) + autorisation
        case "echec":
            return qsTr("La mise à jour vers MMail %1 n'a pas abouti — %2").arg(v).arg(erreurMiseAJour)
        }
        return ""
    }

    function versionDisponible(version, page) {
        versionNouvelle = version
        pageVersion = page
        var choix = reglagesMiseAJour.version === version ? reglagesMiseAJour.choix : ""
        if (choix === "jamais") {
            etatMiseAJour = ""
        } else if (genreMiseAJour.length > 0 && (choix === "maintenant" || choix === "fermeture")) {
            // Choix déjà fait : le paquet est repris, ou retrouvé intact.
            if (etatMiseAJour !== "prete")
                telechargerMiseAJour()
        } else {
            etatMiseAJour = "proposee"
        }
        if (essai && essai.scenario.indexOf("mise-a-jour") === 0)
            etapeScenarioMiseAJour("proposee")
    }

    function choisirMiseAJour(choix) {
        reglagesMiseAJour.version = versionNouvelle
        reglagesMiseAJour.choix = choix
        if (choix === "jamais") {
            etatMiseAJour = ""
            annoncer(qsTr("La version %1 ne sera plus proposée ; son lien reste dans « À propos de MMail ».")
                     .arg(versionNouvelle), false)
            return
        }
        if (choix === "fermeture")
            annoncer(qsTr("MMail %1 sera installé à la fermeture de MMail.").arg(versionNouvelle), false)
        telechargerMiseAJour()
    }

    function telechargerMiseAJour() {
        erreurMiseAJour = ""
        progressionMiseAJour = 0
        // « À la fermeture » : en silence, sauf échec.
        etatMiseAJour = reglagesMiseAJour.choix === "maintenant" ? "telechargement" : ""
        miseAJour.telecharger(versionNouvelle, genreMiseAJour, installeur.destination(versionNouvelle))
    }

    function miseAJourPrete(version, fichier) {
        if (version !== versionNouvelle)
            return
        fichierMiseAJour = fichier
        // Installé à la fermeture, quoi qu'il arrive ; « Redémarrer » rouvre MMail.
        installeur.programmer(fichier, false)
        etatMiseAJour = reglagesMiseAJour.choix === "maintenant" ? "prete" : ""
        if (essai && essai.scenario.indexOf("mise-a-jour") === 0)
            etapeScenarioMiseAJour("prete")
    }

    function miseAJourEchouee(version, message) {
        erreurMiseAJour = message
        etatMiseAJour = "echec"
        if (essai && essai.scenario.indexOf("mise-a-jour") === 0)
            etapeScenarioMiseAJour("echec")
    }

    // Les rédactions ouvertes sont gardées sur le poste à la fermeture, et
    // reproposées au démarrage suivant.
    function redemarrerPourMettreAJour() {
        installeur.programmer(fichierMiseAJour, true)
        Qt.quit()
    }

    // Paquets d'une version installée ou abandonnée, téléchargements
    // interrompus : effacés au démarrage.
    function nettoyerMisesAJour() {
        if (genreMiseAJour.length === 0)
            return
        var garder = reglagesMiseAJour.choix !== "jamais" && reglagesMiseAJour.version.length > 0
                ? installeur.destination(reglagesMiseAJour.version) : ""
        miseAJour.nettoyer(genreMiseAJour, installeur.dossier(), garder)
    }

    function annoncer(texte, erreur) {
        if (!texte || texte.length === 0)
            return
        passager = texte
        passagerErreur = erreur
        minuteurPassager.interval = erreur ? 12000 : 6000
        minuteurPassager.restart()
    }

    Connections {
        target: boite
        function onErreurChanged() {
            if (boite.erreur.length > 0)
                fenetre.annoncer(boite.erreur, true)
        }
    }

    /// « 1 message », « 3 messages » : qsTr ne sait accorder qu'avec un
    /// fichier de traduction, que l'application n'a pas (elle est en français).
    function accord(n, singulier, pluriel) {
        return n + " " + (n > 1 ? pluriel : singulier)
    }

    function etatSynchro() {
        if (boite.occupe)
            return qsTr("Synchronisation…")
        if (!derniereSynchro)
            return qsTr("Hors ligne")
        function deux(n) { return (n < 10 ? "0" : "") + n }
        return qsTr("À jour à %1").arg(deux(derniereSynchro.getHours()) + ":" + deux(derniereSynchro.getMinutes()))
    }

    /// Compte et dossier ouverts, leurs compteurs, et la sélection s'il y en a
    /// plusieurs.
    function majInfoDossier() {
        if (vueUnifiee) {
            var nonLusReception = nonLusReceptions
            infoDossier = qsTr("Toutes les boîtes de réception") + "  —  "
                    + accord(modeleMessages.count, qsTr("message"), qsTr("messages"))
                    + (modeleMessages.count >= 1000 ? qsTr(" (les plus récents)") : "")
                    + (nonLusReception > 0 ? ", " + accord(nonLusReception, qsTr("non lu"), qsTr("non lus")) : "")
            return
        }
        if (rechercheGlobale) {
            infoDossier = qsTr("Toutes les boîtes") + "  —  " + (modeleMessages.count >= 500
                    ? qsTr("recherche « %1 » : les 500 plus récents, précisez la recherche").arg(recherche)
                    : qsTr("recherche « %1 » : %2").arg(recherche)
                      .arg(accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats"))))
            return
        }
        if (boite.dossierCourant.length === 0) {
            var n = listeComptes().length
            infoDossier = n === 0 ? qsTr("Aucun compte")
                                  : accord(n, qsTr("compte"), qsTr("comptes")) + qsTr(", aucun dossier ouvert")
            return
        }
        var ligne = null
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre === "dossier" && l.compte === boite.compteCourant && l.chemin === boite.dossierCourant) {
                ligne = l
                break
            }
        }
        var texte = ligne ? ligne.adresse + " › " + libelleLigne(ligne) : boite.dossierCourant
        if (recherche.length > 0) {
            texte += "  —  " + (modeleMessages.count >= 500
                    ? qsTr("recherche « %1 » : les 500 plus récents, précisez la recherche").arg(recherche)
                    : qsTr("recherche « %1 » : %2").arg(recherche)
                      .arg(accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats"))))
        } else {
            var total = ligne ? ligne.messages : modeleMessages.count
            texte += "  —  " + accord(total, qsTr("message"), qsTr("messages"))
            if (ligne && ligne.nonLus > 0)
                texte += ", " + accord(ligne.nonLus, qsTr("non lu"), qsTr("non lus"))
        }
        var choisis = Object.keys(selection).length
        if (choisis > 1)
            texte += "  —  " + accord(choisis, qsTr("sélectionné"), qsTr("sélectionnés"))
        infoDossier = texte
    }

    onSelectionChanged: majInfoDossier()

    // ------------------------------------------------------- presse-papier
    Timer {
        id: minuteurSelection
        interval: 300
        onTriggered: fenetre.copierSelectionAuto()
    }
    Timer {
        interval: 1000
        repeat: true
        running: reglePresse.restant > 0
        onTriggered: reglePresse.rafraichir(fenetre.maintenant())
    }
    Connections {
        target: pressePapier
        function onChange() {
            reglePresse.depot(pressePapier.texte, fenetre.maintenant())
        }
    }

    // Heure en secondes, telle que l'attend la règle du presse-papier.
    function maintenant() {
        return Date.now() / 1000
    }

    function selectionChangee() {
        if (reglagesEdition.copieAuto)
            minuteurSelection.restart()
    }

    /// Copie automatique : la sélection du message part au presse-papier une
    /// fois stabilisée, sauf si un autre logiciel vient d'y déposer quelque
    /// chose — ce contenu-là est protégé une minute.
    function copierSelectionAuto() {
        var texte = vueCorps.selectedText
        if (!reglagesEdition.copieAuto || texte.length === 0)
            return
        if (!reglePresse.peutEcraser(maintenant())) {
            annoncer(qsTr("Presse-papier protégé : la sélection n'est pas copiée."), false)
            return
        }
        copierTexte(texte, sourceVisible ? "source" : "message")
    }

    /// Copie explicite, qui passe toujours : c'est l'utilisateur qui la demande.
    function copierTexte(texte, origine) {
        if (!texte || texte.length === 0)
            return
        // La règle apprend d'abord que l'écriture vient de nous : sous Linux,
        // le presse-papier signale le dépôt pendant même l'écriture, et la
        // règle y verrait sinon un dépôt étranger — MMail se protégerait
        // alors une minute contre sa propre copie.
        reglePresse.ecriture(texte, origine, maintenant())
        pressePapier.deposer(texte)
        annoncer(qsTr("Copié : %1.").arg(accord(texte.length, qsTr("caractère"), qsTr("caractères"))), false)
    }

    function copierChamp(champ) {
        var uids = uidsChoisis()
        if (uids.length !== 1)
            return
        var i = indexDe(uids[0])
        if (i >= 0)
            copierTexte(modeleMessages.get(i)[champ], "message")
    }

    // Les messages de l'application passent par la barre d'information.
    QtObject {
        id: messageEtat
        property string texte: ""
        onTexteChanged: fenetre.annoncer(texte, false)
    }

    // ------------------------------------------------------------- rédaction
    Component {
        id: composantFenetreRedaction
        // ApplicationWindow et non Window : c'est elle qui transmet police et
        // palette aux éléments flottants (listes, menus, dialogues) — une
        // simple Window les laisse aux valeurs par défaut du système.
        ApplicationWindow {
            id: fenetreRedaction
            font: fenetre.font
            palette: fenetre.palette
            width: Math.min(900, Screen.desktopAvailableWidth - 40)
            height: Math.min(700, Screen.desktopAvailableHeight - 60)
            minimumWidth: 480
            minimumHeight: 360
            // Fenêtre indépendante, qui peut passer derrière la principale.
            transientParent: null
            visible: true
            color: fenetre.palette.window
            title: redaction ? redaction.titre : ""
            property alias redaction: chargeurFenetre.item
            onActiveChanged: if (active && redaction) fenetre.redactionActive = redaction.jeton
            function fermerRedaction() {
                redaction.fermetureConfirmee = true
                redaction.oublierGarde()
                fenetreRedaction.close()
            }
            Loader {
                id: chargeurFenetre
                anchors.fill: parent
                sourceComponent: composantRedaction
                onLoaded: item.conteneur = fenetreRedaction
            }
            onClosing: function(fermeture) {
                if (redaction && redaction.modifie && !redaction.fermetureConfirmee) {
                    fermeture.accepted = false
                    // Fermeture demandée par le système (mise à jour, arrêt du
                    // poste) : il n'attendra peut-être pas la réponse.
                    redaction.garder()
                    redaction.demanderFermeture()
                    return
                }
                if (redaction)
                    redaction.oublierGarde()
                fenetre.oublierRedaction(redaction ? redaction.jeton : "")
                Qt.callLater(function() { fenetreRedaction.destroy() })
            }
        }
    }

    Component {
        id: composantVoletRedaction
        Popup {
            id: voletRedaction
            parent: Overlay.overlay
            x: 0
            y: 0
            width: parent ? parent.width : 0
            height: parent ? parent.height : 0
            modal: true
            padding: 0
            closePolicy: Popup.NoAutoClose
            property alias redaction: chargeurVolet.item
            function fermerRedaction() {
                redaction.fermetureConfirmee = true
                redaction.oublierGarde()
                voletRedaction.close()
            }
            Loader {
                id: chargeurVolet
                anchors.fill: parent
                sourceComponent: composantRedaction
                onLoaded: item.conteneur = voletRedaction
            }
            onClosed: {
                fenetre.oublierRedaction(redaction ? redaction.jeton : "")
                Qt.callLater(function() { voletRedaction.destroy() })
            }
        }
    }

    Component {
        id: composantRedaction
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

            function choisirCompte(compte) {
                for (var i = 0; i < fenetre.comptesConnus.length; ++i)
                    if (fenetre.comptesConnus[i].compte === compte)
                        choixCompte.currentIndex = i
            }
            function compteChoisi() {
                return fenetre.comptesConnus[choixCompte.currentIndex] || null
            }

            function identiteChoisie() {
                var c = compteChoisi()
                return fenetre.identite(c ? c.adresse : "")
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
                champA.text = p.a || ""
                champCc.text = p.cc || ""
                champCci.text = p.cci || ""
                champObjet.text = p.objet || ""
                if (p.html && p.html.length > 0) {
                    miseEnForme = true
                    mef.poserHtml(p.html)
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
                if (fermerApresEnregistrement)
                    conteneur.fermerRedaction()
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
                        model: fenetre.comptesConnus
                        displayText: currentIndex >= 0 && fenetre.comptesConnus[currentIndex]
                                     ? fenetre.comptesConnus[currentIndex].adresse : ""
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
                        visible: redac.miseEnForme
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
                        visible: redac.miseEnForme
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
                        visible: redac.miseEnForme
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
                        visible: redac.miseEnForme
                        text: qsTr("• Liste")
                        checkable: true
                        checked: mef.puces
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerListe(false)
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("1. Liste")
                        checkable: true
                        checked: mef.numeros
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerListe(true)
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("Lien…")
                        focusPolicy: Qt.NoFocus
                        onClicked: dlgAdresseLien.open()
                    }
                    ToolButton {
                        visible: redac.miseEnForme
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
                                Label {
                                    text: modelData.nom + "  (" + fenetre.tailleLisible(modelData.taille) + ")"
                                    Layout.leftMargin: 6
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
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                    rightPadding: ScrollBar.vertical.width
                    TextArea {
                        id: corpsRedaction
                        textFormat: TextEdit.RichText
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        persistentSelection: true
                        font.pointSize: fenetre.tailleColonne("message")
                        enabled: !redac.chargement
                        placeholderText: qsTr("Votre message")
                        background: Rectangle { color: fenetre.palette.base }
                        onTextChanged: if (!redac.chargement) redac.modifie = true
                        Keys.onPressed: function(ev) {
                            if (ev.matches(StandardKey.Paste) && redac.collerImage())
                                ev.accepted = true
                        }
                    }
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
    }

    // ------------------------------------------------ ligne d'arborescence
    Component {
        id: ligneArborescence

        ItemDelegate {
            id: ligne
            width: ListView.view.width
            readonly property bool estDossier: model.genre === "dossier" || model.genre === "favori"
            // Pas en vue multi-dossiers : le noyau y suit le message choisi,
            // ce n'est pas le dossier qu'on regarde.
            readonly property bool estCourant: estDossier && !fenetre.listeMulti
                    && model.compte === boite.compteCourant
                    && model.chemin === boite.dossierCourant
            // Vrai pendant qu'un glisser-déposer accepté survole la ligne.
            property bool survol: false
            highlighted: estCourant
            hoverEnabled: true

            function cliquer() {
                if (model.genre === "compte")
                    boite.replierCompte(model.compte, !model.replie)
                else if (estDossier && model.selectionnable)
                    fenetre.ouvrirDossier(model.compte, model.chemin)
            }

            /// Vrai si le clic tombe sur le chevron d'un dossier parent (marge
            /// comprise, pour le doigt comme pour la souris). Mesuré depuis le
            /// centre du chevron : le chevron d'un dossier déplié est tourné
            /// d'un quart de tour autour de ce centre, et son coin supérieur
            /// gauche, lui, se déplace — le clic sur un dossier déplié tombait à
            /// côté de la zone (retour de Manu, 01/10).
            function surChevron(x) {
                if (!chevronDossier.visible || !model.enfants)
                    return false
                var centre = chevronDossier.mapToItem(zoneLigne, chevronDossier.width / 2, chevronDossier.height / 2)
                return Math.abs(x - centre.x) <= chevronDossier.width / 2 + 6
            }

            // La rubrique Favoris sur un fond plus soutenu que celui des
            // comptes. Au-dessus du fond propre à la ligne (z -1, opaque dans
            // le style Fusion), sauf quand la ligne est choisie ou pressée.
            Rectangle {
                z: -0.5
                anchors.fill: parent
                visible: model.groupe === "favoris" && !ligne.highlighted && !ligne.down
                color: fenetre.fondFavoris
            }

            function menuLigne() {
                if (model.genre === "compte")
                    menuCompte.ouvrir(model.compte, model.adresse, model.hote, model.etat)
                else if (estDossier)
                    menuDossier.ouvrir(model.compte, model.chemin, model.favori, model.masque, model.nom, model.selectionnable)
            }

            // Cible de dépôt : des messages glissés depuis la liste (sur un
            // dossier), ou un dossier glissé depuis l'arborescence (sur la
            // rubrique Favoris ou l'un de ses favoris, décision 3).
            // Un compte glissé sur un autre compte prend sa place (moitié haute)
            // ou la suivante (moitié basse).
            DropArea {
                id: depot
                anchors.fill: parent
                keys: ["mmail/messages", "mmail/dossier", "mmail/compte"]
                onEntered: function(glisse) {
                    var dossierGlisse = glisse.keys.indexOf("mmail/dossier") >= 0
                    var compteGlisse = glisse.keys.indexOf("mmail/compte") >= 0
                    var accepte = compteGlisse
                            ? (model.genre === "compte" && glisse.source && glisse.source.compte !== model.compte)
                            : dossierGlisse
                            ? (model.genre === "rubrique" || model.genre === "favori"
                               || model.genre === "favori-vide")
                            : (ligne.estDossier && model.selectionnable)
                    glisse.accepted = accepte
                    ligne.survol = accepte
                }
                onExited: ligne.survol = false
                onDropped: function(depose) {
                    ligne.survol = false
                    if (depose.keys.indexOf("mmail/compte") >= 0) {
                        boite.placerCompte(depose.source.compte,
                                           depose.y < height / 2 ? model.compte : fenetre.compteSuivant(model.compte))
                    } else if (depose.keys.indexOf("mmail/dossier") >= 0) {
                        var source = depose.source
                        boite.placerFavori(source.compte, source.chemin,
                                           model.genre === "favori" ? model.compte : 0,
                                           model.genre === "favori" ? model.chemin : "")
                    } else {
                        fenetre.deplacerVers(model.compte, model.chemin)
                    }
                    depose.accept(Qt.MoveAction)
                }
            }
            Rectangle {
                anchors.fill: parent
                visible: ligne.survol
                color: "transparent"
                border.color: fenetre.accent
                border.width: 2
                radius: 3
            }

            // Ce qui part quand on glisse un dossier : son nom, qui suit le
            // pointeur. Il porte de quoi désigner le dossier au dépôt.
            Rectangle {
                id: etiquetteDossier
                property int compte: model.compte
                property string chemin: model.chemin
                width: texteEtiquetteDossier.implicitWidth + 16
                height: texteEtiquetteDossier.implicitHeight + 8
                radius: 4
                color: fenetre.palette.highlight
                visible: zoneLigne.drag.active
                Drag.active: zoneLigne.drag.active
                Drag.keys: model.genre === "compte" ? ["mmail/compte"] : ["mmail/dossier"]
                Drag.hotSpot.x: 8
                Drag.hotSpot.y: height / 2
                Drag.supportedActions: Qt.MoveAction
                Label {
                    id: texteEtiquetteDossier
                    anchors.centerIn: parent
                    color: fenetre.palette.highlightedText
                    text: fenetre.libelleLigne(model)
                }
                states: State {
                    when: zoneLigne.drag.active
                    ParentChange { target: etiquetteDossier; parent: fenetre.contentItem }
                }
            }

            MouseArea {
                id: zoneLigne
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Un dossier (vers les Favoris) ou un en-tête de compte (pour
                // l'ordre des comptes) se glisse, et pas au doigt : glisser y
                // fait défiler l'arborescence.
                drag.target: (ligne.estDossier || model.genre === "compte") && !fenetre.compact
                             ? etiquetteDossier : null
                drag.threshold: 8
                onPressed: function(souris) {
                    etiquetteDossier.x = souris.x
                    etiquetteDossier.y = souris.y
                }
                onClicked: function(souris) {
                    if (souris.button === Qt.RightButton)
                        ligne.menuLigne()
                    else if (ligne.surChevron(souris.x))
                        boite.replierDossier(model.compte, model.chemin, !model.replie)
                    else
                        ligne.cliquer()
                }
                onPressAndHold: ligne.menuLigne()
                drag.onActiveChanged: fenetre.glisse = drag.active ? etiquetteDossier : null
                onReleased: {
                    if (drag.active)
                        etiquetteDossier.Drag.drop()
                }
            }

            contentItem: RowLayout {
                spacing: 6
                Item {
                    Layout.preferredWidth: model.genre === "dossier" ? model.profondeur * 14
                                         : model.genre === "favori" || model.genre === "favori-vide" ? 14 : 0
                }
                // Chevron d'un dossier qui a des sous-dossiers : un clic les
                // replie ou les déplie (retour de Manu, 01/10). La place est
                // gardée pour les autres, pour que les noms restent alignés.
                Label {
                    id: chevronDossier
                    visible: model.genre === "dossier"
                    text: model.enfants ? "›" : ""
                    color: fenetre.palette.windowText
                    font.bold: true
                    rotation: model.replie ? 0 : 90
                    opacity: 0.7
                    horizontalAlignment: Text.AlignHCenter
                    Layout.preferredWidth: 14
                }
                // « › » plutôt qu'un triangle : présent dans toutes les polices.
                Label {
                    visible: model.genre === "compte"
                    text: "›"
                    color: fenetre.palette.windowText
                    font.bold: true
                    rotation: model.replie ? 0 : 90
                    opacity: 0.7
                }
                Label {
                    readonly property bool entete: model.genre === "compte" || model.genre === "rubrique"
                    text: fenetre.libelleLigne(model)
                    color: ligne.highlighted ? fenetre.palette.highlightedText
                         : model.genre === "rubrique" ? fenetre.couleurRubrique
                         : fenetre.palette.windowText
                    font.bold: entete || model.nonLus > 0
                    // Les en-têtes, rubrique et comptes, un cran au-dessus des
                    // dossiers ; sous une apparence de marque, dans la police
                    // des titres, la rubrique en capitales espacées.
                    font.pointSize: entete ? fenetre.tailleArborescence * (fenetre.titresEtroits ? 1.25 : 1.12)
                                           : fenetre.tailleArborescence
                    font.family: fenetre.marque && entete ? fenetre.policeTitres : fenetre.font.family
                    font.capitalization: fenetre.marque && model.genre === "rubrique" ? Font.AllUppercase : Font.MixedCase
                    font.letterSpacing: fenetre.marque && model.genre === "rubrique" ? 0.8 : 0
                    font.italic: model.masque || model.genre === "favori-vide"
                    opacity: model.masque || model.genre === "favori-vide"
                             || (ligne.estDossier && !model.selectionnable) ? 0.5 : 1
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                Label {
                    visible: model.genre === "compte" && model.etat !== "pret"
                    text: model.etat === "connexion" ? qsTr("connexion…")
                        : model.etat === "erreur" ? qsTr("erreur") : qsTr("hors ligne")
                    color: model.etat === "erreur" ? "#b00020" : fenetre.palette.windowText
                    opacity: 0.7
                    font.pointSize: fenetre.tailleArborescence * 0.85
                    // Le motif de l'erreur reste lisible au survol, une fois le
                    // message passager effacé.
                    HoverHandler { id: survolEtat }
                    ToolTip.visible: survolEtat.hovered && model.motif.length > 0
                    ToolTip.text: model.motif
                }
                Label {
                    visible: ligne.estDossier && model.nonLus > 0 && fenetre.marque === null
                    text: model.nonLus
                    font.bold: true
                    color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.texteAccent
                }
                CompteurNonLus {
                    visible: ligne.estDossier && model.nonLus > 0 && fenetre.marque !== null
                    nombre: model.nonLus
                    Layout.rightMargin: 2
                }
            }

            // Dossier ouvert, sous une apparence de marque : un filet cyan à
            // gauche de la ligne.
            Rectangle {
                visible: fenetre.marque !== null && ligne.highlighted
                width: 4
                height: parent.height
                color: fenetre.accent
            }
        }
    }

    // ---------------------------------------------------- ligne de message
    Component {
        id: ligneMessage

        ItemDelegate {
            id: ligne
            width: ListView.view.width
            readonly property bool choisi: fenetre.listeMulti
                                           ? fenetre.cleChoisie === fenetre.cleResultat(model.compte, model.chemin, model.uid)
                                           : fenetre.selection[model.uid] === true
            highlighted: choisi
            padding: 0

            // Bandeau des non-lus, comme Outlook ; sous une apparence de
            // marque, le message choisi porte aussi un filet, pétrole s'il est lu.
            Rectangle {
                width: fenetre.marque ? 4 : 3
                height: parent.height
                visible: !model.lu || (fenetre.marque !== null && ligne.highlighted)
                color: fenetre.marque && model.lu ? fenetre.couleurRubrique : fenetre.accent
            }

            // Des lignes plus aérées, séparées d'un trait (retour de Fabienne,
            // 29/09 : les messages se distinguaient mal les uns des autres).
            topPadding: 7
            bottomPadding: 7
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: fenetre.palette.windowText
                opacity: 0.18
            }
            contentItem: ColumnLayout {
                spacing: 2
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 8
                    Label {
                        text: model.expediteur
                        color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                        font.bold: !model.lu
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    // Trombone : le message porte des pièces jointes.
                    Canvas {
                        visible: model.pieces === true
                        Layout.preferredHeight: dateLigne.implicitHeight
                        Layout.preferredWidth: Math.round(dateLigne.implicitHeight * 0.62)
                        property color encre: ligne.highlighted ? fenetre.palette.highlightedText
                                                                : fenetre.palette.windowText
                        onEncreChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()
                        onPaint: fenetre.dessinerTrombone(getContext("2d"), width, height, encre)
                    }
                    Label {
                        id: dateLigne
                        text: fenetre.dateCourte(model.date)
                        color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                        opacity: 0.7
                        font.pointSize: fenetre.tailleListe * 0.85
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    // Place du drapeau, posé par-dessus en bout de ligne.
                    Layout.rightMargin: 8 + drapeauLigne.width
                    spacing: 4
                    // Importance annoncée par l'expéditeur : « ! » haute, « ↓ » basse.
                    Label {
                        visible: model.importance !== 0
                        text: model.importance > 0 ? "!" : "↓"
                        font.bold: true
                        color: ligne.highlighted ? fenetre.palette.highlightedText
                             : model.importance > 0 ? fenetre.couleurAlerte : "#1a4480"
                    }
                    // Répondu, répondu à tous, transféré — par MMail ou par un
                    // autre logiciel, le serveur porte les drapeaux : des
                    // pastilles devant l'objet, aux préfixes d'Outlook (forme A,
                    // choisie par Manu le 10/10 ; les flèches de la 0.5.4 et les
                    // mots en bout de ligne de la 0.5.5 ne lui convenaient pas).
                    MarqueSuite {
                        visible: model.repondu === true
                        texte: model.reponduTous === true ? qsTr("RE TOUS") : qsTr("RE")
                        infobulle: model.reponduTous === true ? qsTr("Répondu à tous") : qsTr("Répondu")
                        color: "#6b3fa0"
                        border.width: ligne.highlighted ? 1 : 0
                    }
                    MarqueSuite {
                        visible: model.transfere === true
                        texte: qsTr("TR")
                        infobulle: qsTr("Transféré")
                        color: "#1a4480"
                        border.width: ligne.highlighted ? 1 : 0
                    }
                    Label {
                        text: model.sujet
                        font.bold: !model.lu
                        color: ligne.highlighted ? fenetre.palette.highlightedText
                             : model.lu ? fenetre.palette.windowText : fenetre.texteAccent
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
                // Recherche dans toutes les boîtes : le dossier du message, sur
                // sa ligne, coupé au milieu au besoin — le compte et le dossier
                // restent lisibles (demande de Manu, 08/10).
                Label {
                    visible: fenetre.listeMulti
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 8
                    elide: Text.ElideMiddle
                    font.pointSize: fenetre.tailleListe * 0.9
                    color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.texteAccent
                    text: model.cheminAffiche || ""
                }
            }

            // Ce qui part quand on glisse : l'étiquette qui suit le pointeur.
            Rectangle {
                id: etiquette
                property string uids: ""
                width: texteEtiquette.implicitWidth + 16
                height: texteEtiquette.implicitHeight + 8
                radius: 4
                color: fenetre.palette.highlight
                visible: zone.drag.active
                Drag.active: zone.drag.active
                Drag.keys: ["mmail/messages"]
                Drag.hotSpot.x: 8
                Drag.hotSpot.y: height / 2
                Drag.supportedActions: Qt.MoveAction
                Label {
                    id: texteEtiquette
                    anchors.centerIn: parent
                    color: fenetre.palette.highlightedText
                    text: fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages"))
                }
                states: State {
                    when: zone.drag.active
                    ParentChange { target: etiquette; parent: fenetre.contentItem }
                }
            }

            MouseArea {
                id: zone
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Au doigt — téléphone, ou écran tactile —, glisser fait défiler
                // la liste : pas de glisser-déposer (le tri passe par l'appui
                // long et le menu), et pas de choix à l'appui. Choisir à l'appui
                // ouvrait le message au premier contact, avant que la liste ait
                // reconnu le geste : elle ne défilait plus (APK, retour de Manu
                // du 05/10). Au doigt, un message s'ouvre au toucher bref.
                property bool auDoigt: fenetre.compact
                drag.target: auDoigt ? null : etiquette
                drag.threshold: 8
                // Vrai si l'appui a déjà choisi la ligne : le clic n'a alors
                // plus rien à faire.
                property bool choisieAuPress: false

                onPressed: function(souris) {
                    auDoigt = fenetre.compact || souris.source !== Qt.MouseEventNotSynthesized
                    choisieAuPress = !auDoigt && souris.button === Qt.LeftButton && !ligne.choisi
                            && !(souris.modifiers & (Qt.ControlModifier | Qt.ShiftModifier))
                    // Glisser une ligne non choisie la choisit d'abord.
                    if (choisieAuPress)
                        fenetre.choisir(index, 0)
                    etiquette.x = souris.x
                    etiquette.y = souris.y
                }
                onClicked: function(souris) {
                    if (souris.button === Qt.RightButton) {
                        if (!ligne.choisi)
                            fenetre.choisir(index, 0)
                        menuMessage.popup()
                        return
                    }
                    if (!choisieAuPress)
                        fenetre.choisir(index, souris.modifiers)
                }
                onPressAndHold: {
                    if (!ligne.choisi)
                        fenetre.choisir(index, 0)
                    menuMessage.popup()
                }
                onDoubleClicked: function(souris) {
                    if (souris.button === Qt.LeftButton)
                        fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")
                }
                drag.onActiveChanged: fenetre.glisse = drag.active ? etiquette : null
                onReleased: {
                    if (drag.active)
                        etiquette.Drag.drop()
                }
            }

            // Drapeau de suivi, en bout de seconde ligne, comme Outlook : plein
            // s'il est posé ; en creux au survol, pour montrer qu'un clic le pose.
            Canvas {
                id: drapeauLigne
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 7
                height: dateLigne.implicitHeight
                width: Math.round(height * 0.8)
                visible: model.suivi === true || ligne.hovered || survolDrapeau.containsMouse
                property color encre: model.suivi === true
                                      ? (ligne.highlighted ? fenetre.palette.highlightedText : fenetre.couleurAlerte)
                                      : (ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText)
                property bool plein: model.suivi === true
                opacity: plein ? 1 : 0.45
                onEncreChanged: requestPaint()
                onPleinChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: fenetre.dessinerDrapeau(getContext("2d"), width, height, encre, plein)
                MouseArea {
                    id: survolDrapeau
                    anchors.fill: parent
                    anchors.margins: -4
                    hoverEnabled: true
                    onClicked: fenetre.basculerSuivi([model.uid], model.suivi !== true)
                }
            }
        }
    }

    // ---------------------------------------------------------------- menus
    Menu {
        id: menuComptes
        onAboutToShow: width = fenetre.largeurMenu(menuComptes)
        MenuItem {
            text: qsTr("Ajouter un compte…")
            onTriggered: fenetre.demanderCompte(null, "")
        }
        MenuItem {
            text: qsTr("Ajouter avec un lien de configuration…")
            onTriggered: dlgLien.ouvrir("", "")
        }
        MenuItem {
            text: qsTr("Nom et signature…")
            enabled: fenetre.comptesConnus.length > 0
            onTriggered: dlgIdentite.ouvrir(boite.compteCourant)
        }
        MenuSeparator {}
        // Le retrait est aussi au clic droit sur le compte ; il est repris ici
        // pour qu'on le trouve sans le connaître.
        Menu {
            id: menuRetrait
            title: qsTr("Retirer un compte")
            onAboutToShow: width = fenetre.largeurMenu(menuRetrait)
            Instantiator {
                model: fenetre.comptesConnus
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.adresse + "…"
                    onTriggered: dlgRetrait.ouvrir(modelData.compte, modelData.adresse, modelData.hote)
                }
                onObjectAdded: function(indice, objet) { menuRetrait.insertItem(indice, objet) }
                onObjectRemoved: function(indice, objet) { menuRetrait.removeItem(objet) }
            }
            MenuItem {
                text: qsTr("Aucun compte")
                enabled: false
                visible: fenetre.comptesConnus.length === 0
                height: visible ? implicitHeight : 0
            }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Vider les corbeilles et indésirables…")
            enabled: fenetre.comptesConnus.length > 0
            onTriggered: dlgViderCorbeilles.open()
        }
        MenuSeparator {}
        MenuItem {
            text: reglagesSauvegarde.date > 0
                  ? qsTr("Sauvegarder la configuration (dernière : %1)").arg(fenetre.dateLongue(reglagesSauvegarde.date))
                  : qsTr("Sauvegarder la configuration")
            enabled: fenetre.comptesConnus.length > 0
            onTriggered: fenetre.sauvegarderConfiguration(true)
        }
        MenuItem {
            text: qsTr("Restaurer une configuration sauvegardée…")
            enabled: fenetre.comptesConnus.length > 0 && !fenetre.rechercheSauvegarde
            onTriggered: fenetre.chercherSauvegardes(true)
        }
    }

    Menu {
        id: menuAide
        onAboutToShow: width = fenetre.largeurMenu(menuAide)
        MenuItem {
            text: qsTr("Aide (F1)")
            onTriggered: dlgAide.open()
        }
        MenuItem {
            text: qsTr("À propos de MMail…")
            onTriggered: dlgAPropos.open()
        }
    }

    Menu {
        id: menuAffichage
        onAboutToShow: width = fenetre.largeurMenu(menuAffichage)
        Repeater {
            model: fenetre.nomsApparence
            MenuItem {
                required property string modelData
                text: fenetre.jeuxApparence[modelData].libelle
                checkable: true
                checked: fenetre.apparence === modelData
                onTriggered: fenetre.choisirApparence(modelData)
            }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Logiciels d'ouverture des pièces jointes…")
            visible: Qt.platform.os !== "android"
            height: visible ? implicitHeight : 0
            onTriggered: dlgLogiciels.open()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Agrandir la colonne survolée (Ctrl +)")
            onTriggered: fenetre.zoomer(fenetre.colonneActive, 1.1)
        }
        MenuItem {
            text: qsTr("Réduire la colonne survolée (Ctrl −)")
            onTriggered: fenetre.zoomer(fenetre.colonneActive, 1 / 1.1)
        }
        MenuItem {
            text: qsTr("Taille normale partout")
            onTriggered: {
                fenetre.poserZoom("arborescence", 1)
                fenetre.poserZoom("liste", 1)
                fenetre.poserZoom("message", 1)
            }
        }
    }

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
            onTriggered: {
                dlgChoisirLogiciel.piece = menuPiece.piece
                dlgChoisirLogiciel.open()
            }
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

    // « Ouvrir avec… » : un logiciel du poste, pour cette fois ou pour toutes
    // les pièces de la même extension (décision 9).
    FileDialog {
        id: dlgChoisirLogiciel
        property var piece: null
        title: qsTr("Ouvrir « %1 » avec…").arg(piece ? piece.nom : "")
        fileMode: FileDialog.OpenFile
        nameFilters: Qt.platform.os === "windows" ? [qsTr("Programmes (*.exe)"), qsTr("Tous les fichiers (*)")] : []
        onAccepted: dlgOuvrirAvec.ouvrir(piece, selectedFile.toString())
    }

    Dialog {
        id: dlgOuvrirAvec
        property var piece: null
        property string programme: ""
        readonly property string extension: piece ? fenetre.extensionDe(piece.nom) : ""
        function ouvrir(p, prog) {
            piece = p
            programme = prog
            toujoursAvec.checked = false
            open()
        }
        title: qsTr("Ouvrir avec %1").arg(lanceur.nom(programme))
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        ColumnLayout {
            width: dlgOuvrirAvec.availableWidth
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Ouvrir « %1 » avec %2.").arg(dlgOuvrirAvec.piece ? dlgOuvrirAvec.piece.nom : "")
                      .arg(lanceur.nom(dlgOuvrirAvec.programme))
            }
            CheckBox {
                id: toujoursAvec
                visible: dlgOuvrirAvec.extension.length > 0
                text: qsTr("Toujours ouvrir les fichiers .%1 avec ce logiciel").arg(dlgOuvrirAvec.extension)
            }
        }
        onAccepted: {
            if (toujoursAvec.checked && extension.length > 0)
                fenetre.retenirLogiciel(extension, programme)
            fenetre.ouvrirPiece(piece, programme)
        }
    }

    // Les logiciels retenus, par extension, sur ce poste ; « Retirer » rend le
    // type au logiciel du système.
    Dialog {
        id: dlgLogiciels
        property var liste: []
        function actualiser() {
            var m = fenetre.logicielsPieces()
            liste = Object.keys(m).sort().map(function(e) { return { extension: e, programme: m[e] } })
        }
        onAboutToShow: actualiser()
        title: qsTr("Logiciels d'ouverture des pièces jointes")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, fenetre.width - 24)
        standardButtons: Dialog.Close
        ColumnLayout {
            width: dlgLogiciels.availableWidth
            spacing: 4
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.8
                text: dlgLogiciels.liste.length === 0
                      ? qsTr("Aucun : les pièces jointes s'ouvrent avec le logiciel du système. Pour en choisir un : clic droit sur une pièce, « Ouvrir avec… ».")
                      : qsTr("Sur ce poste, ces types de fichiers s'ouvrent avec le logiciel choisi ; les autres, avec celui du système.")
            }
            Repeater {
                model: dlgLogiciels.liste
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Label {
                        text: "." + modelData.extension
                        font.bold: true
                        Layout.preferredWidth: 70
                    }
                    Label {
                        text: lanceur.nom(modelData.programme)
                        elide: Text.ElideMiddle
                        Layout.fillWidth: true
                        HoverHandler { id: survolLogiciel }
                        ToolTip.visible: survolLogiciel.hovered
                        ToolTip.text: lanceur.chemin(modelData.programme)
                    }
                    Button {
                        text: qsTr("Retirer")
                        flat: true
                        onClicked: {
                            fenetre.retenirLogiciel(modelData.extension, "")
                            dlgLogiciels.actualiser()
                        }
                    }
                }
            }
        }
    }

    FileDialog {
        id: dlgEnregistrerPiece
        property var piece: null
        property int uid: 0
        title: qsTr("Enregistrer la pièce jointe")
        fileMode: FileDialog.SaveFile
        onAccepted: boite.enregistrerPiece(uid, piece.indice, selectedFile.toString())
    }

    Menu {
        id: menuCorps
        font.pointSize: fenetre.tailleColonne("message")
        onAboutToShow: width = fenetre.largeurMenu(menuCorps)
        MenuItem {
            text: qsTr("Copier")
            enabled: vueCorps.selectedText.length > 0
            onTriggered: fenetre.copierTexte(vueCorps.selectedText, fenetre.sourceVisible ? "source" : "message")
        }
        MenuItem {
            text: qsTr("Tout sélectionner")
            onTriggered: vueCorps.selectAll()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Copier la sélection automatiquement")
            checkable: true
            checked: reglagesEdition.copieAuto
            onTriggered: reglagesEdition.copieAuto = checked
        }
    }

    Menu {
        id: menuMessage
        font.pointSize: fenetre.tailleColonne("liste")
        onAboutToShow: width = fenetre.largeurMenu(menuMessage)
        MenuItem {
            visible: fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Reprendre")
            onTriggered: fenetre.rediger("brouillon")
        }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Répondre")
            onTriggered: fenetre.rediger("repondre")
        }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Répondre à tous")
            onTriggered: fenetre.rediger("repondre_tous")
        }
        MenuItem {
            text: qsTr("Transférer")
            onTriggered: fenetre.rediger("transferer")
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Poser un drapeau de suivi")
            onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), true)
        }
        MenuItem {
            text: qsTr("Retirer le drapeau")
            onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), false)
        }
        MenuSeparator {}
        MenuItem { text: qsTr("Déplacer vers…"); onTriggered: fenetre.ouvrirDeplacer() }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Courrier indésirable")
            onTriggered: fenetre.signalerIndesirable()
        }
        MenuSeparator {}
        MenuItem { text: qsTr("Marquer comme lu"); onTriggered: fenetre.marquerSelection(true) }
        MenuItem { text: qsTr("Marquer comme non lu"); onTriggered: fenetre.marquerSelection(false) }
        MenuSeparator {}
        MenuItem { text: qsTr("Supprimer"); onTriggered: fenetre.supprimerSelection() }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("adresse")
        }
        MenuItem {
            text: qsTr("Copier l'objet")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("sujet")
        }
        MenuItem {
            text: qsTr("Afficher la source")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: {
                fenetre.sourceVisible = false
                fenetre.basculerSource()
            }
        }
    }

    Menu {
        id: menuDossier
        font.pointSize: fenetre.tailleColonne("arborescence")
        onAboutToShow: width = fenetre.largeurMenu(menuDossier)
        property int compte: 0
        property string chemin: ""
        property bool favori: false
        property bool masque: false
        property string nom: ""
        property bool selectionnable: true
        function ouvrir(c, ch, f, m, n, sel) {
            compte = c; chemin = ch; favori = f; masque = m; nom = n; selectionnable = sel
            popup()
        }
        MenuItem {
            text: menuDossier.favori ? qsTr("Retirer des favoris") : qsTr("Ajouter aux favoris")
            onTriggered: boite.epinglerDossier(menuDossier.compte, menuDossier.chemin, !menuDossier.favori)
        }
        MenuItem {
            text: menuDossier.masque ? qsTr("Réafficher ce dossier") : qsTr("Masquer ce dossier")
            onTriggered: boite.masquerDossier(menuDossier.compte, menuDossier.chemin, !menuDossier.masque)
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Nouveau sous-dossier…")
            onTriggered: dlgNouveauDossier.ouvrir(menuDossier.compte, menuDossier.chemin, menuDossier.nom)
        }
        MenuItem {
            text: qsTr("Vider ce dossier…")
            enabled: menuDossier.selectionnable
            onTriggered: dlgViderDossier.ouvrir(menuDossier.compte, menuDossier.chemin, menuDossier.nom)
        }
    }

    Menu {
        id: menuCompte
        font.pointSize: fenetre.tailleColonne("arborescence")
        onAboutToShow: width = fenetre.largeurMenu(menuCompte)
        property int compte: 0
        property string adresse: ""
        property string hote: ""
        property string etat: ""
        function ouvrir(c, a, h, e) {
            compte = c; adresse = a; hote = h; etat = e
            popup()
        }
        MenuItem {
            text: menuCompte.etat === "pret" ? qsTr("Se déconnecter") : qsTr("Se connecter…")
            onTriggered: {
                if (menuCompte.etat === "pret")
                    boite.deconnecter(menuCompte.compte)
                else
                    fenetre.connecterCompte({ id: menuCompte.compte, adresse: menuCompte.adresse,
                                              hote: menuCompte.hote })
            }
        }
        MenuItem {
            text: qsTr("Oublier le mot de passe")
            onTriggered: coffre.effacer(fenetre.cle(menuCompte.adresse, menuCompte.hote))
        }
        MenuSeparator {}
        // L'ordre des comptes se règle aussi en glissant leur en-tête.
        MenuItem {
            text: qsTr("Monter ce compte")
            onTriggered: boite.decalerCompte(menuCompte.compte, -1)
        }
        MenuItem {
            text: qsTr("Descendre ce compte")
            onTriggered: boite.decalerCompte(menuCompte.compte, 1)
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Retirer ce compte de MMail…")
            onTriggered: dlgRetrait.ouvrir(menuCompte.compte, menuCompte.adresse, menuCompte.hote)
        }
    }

    Shortcut {
        sequences: [StandardKey.Delete]
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: fenetre.supprimerSelection()
    }
    // Raccourcis d'Outlook : déplacer, marquer lu, marquer non lu.
    Shortcut {
        sequence: "Ctrl+Shift+V"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: fenetre.ouvrirDeplacer()
    }
    Shortcut {
        sequence: "Ctrl+Q"
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.marquerSelection(true)
    }
    Shortcut {
        sequence: "Ctrl+U"
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.marquerSelection(false)
    }
    Shortcut {
        sequences: [StandardKey.Refresh]
        onActivated: boite.actualiser()
    }
    // Courrier indésirable, comme dans Outlook.
    Shortcut {
        sequence: "Ctrl+Alt+J"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0 && !fenetre.dossierDeReprise
        onActivated: fenetre.signalerIndesirable()
    }
    // Insertion : pose ou retire le drapeau, comme dans Outlook.
    Shortcut {
        sequence: "Ins"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: {
            var uids = fenetre.uidsChoisis()
            var i = fenetre.indexDe(uids[0])
            fenetre.basculerSuivi(uids, !(i >= 0 && modeleMessages.get(i).suivi))
        }
    }

    // Raccourcis d'Outlook pour la rédaction.
    Shortcut {
        sequences: [StandardKey.New]
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.rediger("nouveau")
    }
    Shortcut {
        sequence: "Ctrl+R"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")
    }
    Shortcut {
        sequence: "Ctrl+Shift+R"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger("repondre_tous")
    }
    Shortcut {
        sequence: "Ctrl+F"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger("transferer")
    }
    // Rechercher, comme dans Outlook (Ctrl+F y transfère).
    Shortcut {
        sequence: "Ctrl+1"
        onActivated: fenetre.module = "courrier"
    }
    Shortcut {
        sequence: "Ctrl+2"
        onActivated: fenetre.module = "agenda"
    }
    Shortcut {
        sequence: "Ctrl+E"
        enabled: !fenetre.dialogueOuvert()
        onActivated: {
            champRecherche.forceActiveFocus()
            champRecherche.selectAll()
        }
    }
    Shortcut {
        sequences: [StandardKey.HelpContents]
        enabled: !fenetre.dialogueOuvert()
        onActivated: dlgAide.open()
    }
    Shortcut {
        sequences: [StandardKey.ZoomIn, "Ctrl+="]
        onActivated: fenetre.zoomer(fenetre.colonneActive, 1.1)
    }
    Shortcut {
        sequences: [StandardKey.ZoomOut]
        onActivated: fenetre.zoomer(fenetre.colonneActive, 1 / 1.1)
    }
    Shortcut {
        sequence: "Ctrl+0"
        onActivated: fenetre.poserZoom(fenetre.colonneActive, 1)
    }
    Shortcut {
        sequences: [StandardKey.SelectAll]
        enabled: !fenetre.dialogueOuvert() && !vueCorps.activeFocus
        onActivated: fenetre.toutChoisir()
    }

    // ----------------------------------------------------------- dialogues
    Dialog {
        id: dlgCompte
        property int compteId: 0
        title: compteId > 0 ? qsTr("Mot de passe du compte") : qsTr("Ajouter un compte")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        // Vrai dès que la personne a touché au serveur : la configuration
        // automatique ne réécrit plus ce champ.
        property bool hoteSaisi: false
        property string infoServeur: ""
        onAccepted: fenetre.validerCompte()
        onRejected: fenetre.demandeSuivante()
        onOpened: (champAdresse.text.length > 0 ? champMotDePasse : champAdresse).forceActiveFocus()

        GridLayout {
            anchors.fill: parent
            columns: 2
            columnSpacing: 8
            rowSpacing: 6

            Label {
                id: noteCompte
                Layout.columnSpan: 2
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Label { text: qsTr("Adresse :") }
            TextField {
                id: champAdresse
                placeholderText: "nom@exemple.fr"
                enabled: dlgCompte.compteId === 0
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                onEditingFinished: fenetre.chercherServeur()
            }
            Label { text: qsTr("Serveur IMAP :") }
            TextField {
                id: champHote
                placeholderText: "mail.exemple.fr"
                enabled: dlgCompte.compteId === 0
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                onTextEdited: dlgCompte.hoteSaisi = text.length > 0
            }
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: dlgCompte.infoServeur
            }
            Label { text: qsTr("Mot de passe :") }
            TextField {
                id: champMotDePasse
                echoMode: TextInput.Password
                Layout.fillWidth: true
                onAccepted: dlgCompte.accept()
            }
            CheckBox {
                id: caseMemoriser
                Layout.columnSpan: 2
                checked: reglages.memoriser
                text: qsTr("Mémoriser le mot de passe dans le coffre du système")
            }
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: qsTr("Le mot de passe n'est jamais écrit par MMail : il est confié au coffre du système (Gestionnaire d'identifiants sous Windows, Secret Service ou KWallet sous Linux, Keystore sous Android). Connexion en IMAPS, port 993.")
            }
        }
    }

    Dialog {
        id: dlgIdentite
        title: qsTr("Nom et signature")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(760, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string note: ""
        function ouvrir(compte) {
            var i = 0
            for (var k = 0; k < fenetre.comptesConnus.length; ++k)
                if (fenetre.comptesConnus[k].compte === compte)
                    i = k
            choixIdentite.currentIndex = i
            charger()
            open()
        }
        function adresse() {
            var c = fenetre.comptesConnus[choixIdentite.currentIndex]
            return c ? c.adresse : ""
        }
        function charger() {
            var i = fenetre.identite(adresse())
            champNomAffiche.text = i.nom
            if (i.html.length > 0)
                mefSignature.poserHtml(i.html)
            else
                mefSignature.poserTexte(i.signature)
            champSignature.cursorPosition = 0
            caseSignatureNouveaux.checked = i.nouveaux
            caseSignatureReponses.checked = i.reponses
            note = ""
        }
        // Deux versions : le texte, pour les messages sans mise en forme, et
        // le HTML s'il y a de quoi — images, gras, liens.
        onAccepted: fenetre.poserIdentite(adresse(), {
            nom: champNomAffiche.text.trim(), signature: mefSignature.texte(),
            html: mefSignature.enrichi() ? mefSignature.html() : "",
            nouveaux: caseSignatureNouveaux.checked, reponses: caseSignatureReponses.checked })

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            ComboBox {
                id: choixIdentite
                Layout.fillWidth: true
                model: fenetre.comptesConnus
                displayText: currentIndex >= 0 && fenetre.comptesConnus[currentIndex]
                             ? fenetre.comptesConnus[currentIndex].adresse : ""
                delegate: ItemDelegate {
                    required property var modelData
                    width: ListView.view ? ListView.view.width : implicitWidth
                    text: modelData.adresse
                }
                onActivated: dlgIdentite.charger()
            }
            Label { text: qsTr("Nom affiché chez les destinataires :") }
            TextField {
                id: champNomAffiche
                Layout.fillWidth: true
                placeholderText: qsTr("Prénom Nom")
            }
            Label { text: qsTr("Signature :") }
            // Barre de mise en forme de la signature ; les boutons ne prennent
            // pas le focus, la sélection reste celle sur laquelle ils agissent.
            Flow {
                Layout.fillWidth: true
                spacing: 4
                ToolButton {
                    text: qsTr("G")
                    font.bold: true
                    checkable: true
                    checked: mefSignature.gras
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerGras()
                }
                ToolButton {
                    text: qsTr("I")
                    font.italic: true
                    checkable: true
                    checked: mefSignature.italique
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerItalique()
                }
                ToolButton {
                    text: qsTr("S")
                    font.underline: true
                    checkable: true
                    checked: mefSignature.souligne
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerSouligne()
                }
                ToolButton {
                    text: qsTr("Lien…")
                    focusPolicy: Qt.NoFocus
                    onClicked: dlgLienSignature.open()
                }
                ToolButton {
                    text: qsTr("Image…")
                    focusPolicy: Qt.NoFocus
                    onClicked: choixImageSignature.open()
                }
                ToolButton {
                    text: qsTr("Importer une signature Outlook…")
                    focusPolicy: Qt.NoFocus
                    onClicked: choixImportSignature.open()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: qsTr("Le fichier .htm du dossier %APPDATA%\\Microsoft\\Signatures, avec ses images.")
                }
                ToolButton {
                    text: qsTr("Effacer la mise en forme")
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.effacerMiseEnForme()
                }
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(320, fenetre.height * 0.4)
                TextArea {
                    id: champSignature
                    textFormat: TextEdit.RichText
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                    placeholderText: qsTr("Prénom Nom\nFonction — Société\nTéléphone")
                    color: "#1f1f1f"
                    background: Rectangle { color: "#ffffff"; border.color: fenetre.palette.mid }
                }
            }
            MiseEnForme {
                id: mefSignature
                document: champSignature.textDocument
                curseur: champSignature.cursorPosition
                debut: champSignature.selectionStart
                fin: champSignature.selectionEnd
            }
            Label {
                visible: dlgIdentite.note.length > 0
                text: dlgIdentite.note
                color: "#b00020"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            CheckBox {
                id: caseSignatureNouveaux
                text: qsTr("L'ajouter aux nouveaux messages")
            }
            CheckBox {
                id: caseSignatureReponses
                text: qsTr("L'ajouter aux réponses et aux transferts")
            }
        }

        FileDialog {
            id: choixImageSignature
            title: qsTr("Insérer une image dans la signature")
            nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.bmp *.webp)")]
            onAccepted: {
                var url = boite.imageSignature(dlgIdentite.adresse(), selectedFile)
                if (url.length > 0) {
                    mefSignature.insererImage(url, 640)
                    dlgIdentite.note = ""
                } else {
                    dlgIdentite.note = boite.erreur
                }
            }
        }
        FileDialog {
            id: choixImportSignature
            title: qsTr("Importer une signature Outlook")
            nameFilters: [qsTr("Signatures (*.htm *.html)")]
            onAccepted: {
                var html = boite.importerSignature(dlgIdentite.adresse(), selectedFile)
                if (html.length > 0) {
                    mefSignature.poserHtml(html)
                    champSignature.cursorPosition = 0
                    dlgIdentite.note = ""
                } else {
                    dlgIdentite.note = boite.erreur
                }
            }
        }
        Dialog {
            id: dlgLienSignature
            title: qsTr("Insérer un lien")
            modal: true
            anchors.centerIn: parent
            width: Math.min(520, dlgIdentite.width - 24)
            standardButtons: Dialog.Ok | Dialog.Cancel
            onOpened: {
                champLienSignature.text = "https://"
                champLienSignature.forceActiveFocus()
                champLienSignature.cursorPosition = champLienSignature.text.length
            }
            onAccepted: mefSignature.poserLien(champLienSignature.text)
            TextField {
                id: champLienSignature
                anchors.left: parent.left
                anchors.right: parent.right
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                onAccepted: dlgLienSignature.accept()
            }
        }
    }

    Dialog {
        id: dlgReprise
        title: qsTr("Rédactions retrouvées")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, fenetre.width - 24)
        Label {
            width: dlgReprise.availableWidth
            wrapMode: Text.Wrap
            text: {
                var liste = fenetre.redactionsRetrouvees
                var lignes = []
                for (var k = 0; k < liste.length; ++k) {
                    var c = liste[k].contenu || {}
                    var objet = (c.objet || "").length > 0 ? "« " + c.objet + " »" : qsTr("(sans objet)")
                    var a = (c.a || "").length > 0 ? qsTr(" à %1").arg(c.a) : ""
                    lignes.push("• " + objet + a + qsTr(", modifié le %1").arg(fenetre.dateLongue(liste[k].modifie * 1000)))
                }
                return (liste.length > 1
                        ? qsTr("MMail s'est fermé alors que %1 messages étaient en cours de rédaction :").arg(liste.length)
                        : qsTr("MMail s'est fermé alors qu'un message était en cours de rédaction :"))
                       + "\n" + lignes.join("\n")
            }
        }
        footer: DialogButtonBox {
            Button {
                text: qsTr("Rouvrir")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
            Button {
                text: qsTr("Abandonner")
                DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
            }
            Button {
                text: qsTr("Plus tard")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }
        onAccepted: fenetre.rouvrirRedactions()
        onDiscarded: {
            for (var k = 0; k < fenetre.redactionsRetrouvees.length; ++k)
                boite.oublierRedactionGardee(fenetre.redactionsRetrouvees[k].id)
            fenetre.redactionsRetrouvees = []
            dlgReprise.close()
        }
    }

    Dialog {
        id: dlgRestauration
        title: qsTr("Configuration sauvegardée")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, fenetre.width - 24)
        property int choix: 0
        readonly property var choisie: fenetre.sauvegardesProposees[choix] || null

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: fenetre.sauvegardesProposees.length > 1
                      ? qsTr("Plusieurs sauvegardes de votre configuration existent. Laquelle restaurer ?")
                      : qsTr("Une sauvegarde de votre configuration existe. La restaurer ?")
            }
            Repeater {
                model: fenetre.sauvegardesProposees
                delegate: RadioButton {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    checked: index === dlgRestauration.choix
                    onClicked: dlgRestauration.choix = index
                    text: qsTr("%1, du %2").arg(modelData.nom).arg(fenetre.dateLongue(modelData.date * 1000))
                          + (modelData.poste === reglagesSauvegarde.poste ? qsTr(" (ce poste)") : "")
                }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: dlgRestauration.choisie !== null
                text: dlgRestauration.choisie
                      ? qsTr("Comptes : %1.").arg(dlgRestauration.choisie.comptes.join(", ")) : ""
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                text: qsTr("Reviennent les comptes et leurs mots de passe, les noms et signatures, les dossiers favoris et masqués, l'apparence et les réglages. Les comptes déjà présents sur ce poste restent tels quels.")
            }
        }
        footer: DialogButtonBox {
            Button {
                text: qsTr("Restaurer")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
            Button {
                text: qsTr("Ne pas restaurer")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }
        onAccepted: {
            if (choisie)
                fenetre.restaurerConfiguration(choisie)
        }
    }

    Dialog {
        id: dlgLien
        title: qsTr("Ajouter avec un lien de configuration")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(520, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        function ouvrir(lien, note) {
            champLien.text = lien
            noteLien.text = note
            open()
        }
        onOpened: champLien.forceActiveFocus()
        onAccepted: fenetre.lireLien(champLien.text.trim())

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                id: noteLien
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Collez le lien de configuration que vous avez reçu :")
            }
            TextField {
                id: champLien
                placeholderText: "https://…"
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                onAccepted: dlgLien.accept()
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: qsTr("Le lien ne sert qu'une fois. Il apporte l'adresse, le serveur et le mot de passe de chaque compte ; le mot de passe va directement au coffre du système, sans être affiché.")
            }
        }
    }

    Dialog {
        id: dlgDeplacer
        title: qsTr("Déplacer %1 vers…").arg(fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages")))
        modal: true
        anchors.centerIn: Overlay.overlay
        // Zoom de la colonne d'où il est ouvert (liste, message), fixé à
        // l'ouverture ; les dimensions suivent, dans la limite de la fenêtre.
        property real zoom: 1
        font.pointSize: fenetre.tailleBase * zoom
        width: Math.min(520 * zoom, fenetre.width - 24)
        height: Math.min(560 * zoom, fenetre.height - 40)
        standardButtons: Dialog.Ok | Dialog.Cancel
        property var cibles: []
        onOpened: {
            champFiltre.text = ""
            filtrer()
            champFiltre.forceActiveFocus()
        }
        onAccepted: {
            if (vueCibles.currentIndex >= 0 && vueCibles.currentIndex < modeleCibles.count) {
                var c = modeleCibles.get(vueCibles.currentIndex)
                fenetre.deplacerVers(c.compte, c.chemin)
            }
        }
        function filtrer() {
            modeleCibles.clear()
            var filtre = champFiltre.text.toLowerCase()
            for (var i = 0; i < cibles.length; ++i) {
                var c = cibles[i]
                if (c.compte === boite.compteCourant && c.chemin === boite.dossierCourant)
                    continue
                var texte = (c.adresse + " " + c.chemin + " " + fenetre.libelleLigne(c)).toLowerCase()
                if (filtre.length === 0 || texte.indexOf(filtre) >= 0)
                    modeleCibles.append(c)
            }
            vueCibles.currentIndex = modeleCibles.count > 0 ? 0 : -1
        }

        ColumnLayout {
            anchors.fill: parent
            TextField {
                id: champFiltre
                Layout.fillWidth: true
                placeholderText: qsTr("Filtrer : nom du dossier ou de la boîte")
                onTextChanged: dlgDeplacer.filtrer()
                Keys.onDownPressed: vueCibles.incrementCurrentIndex()
                Keys.onUpPressed: vueCibles.decrementCurrentIndex()
                onAccepted: dlgDeplacer.accept()
            }
            ListView {
                id: vueCibles
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                ScrollBar.vertical: ScrollBar {
                    policy: size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                }
                model: ListModel { id: modeleCibles }
                delegate: ItemDelegate {
                    width: ListView.view.width
                           - (vueCibles.ScrollBar.vertical.size < 1 ? vueCibles.ScrollBar.vertical.width : 0)
                    highlighted: ListView.isCurrentItem
                    onClicked: vueCibles.currentIndex = index
                    onDoubleClicked: dlgDeplacer.accept()
                    contentItem: RowLayout {
                        Label {
                            text: model.adresse
                            opacity: 0.6
                            Layout.preferredWidth: 170 * dlgDeplacer.zoom
                            elide: Text.ElideRight
                        }
                        Item { Layout.preferredWidth: model.profondeur * 12 }
                        Label {
                            text: fenetre.libelleLigne(model)
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }

    // Aide : ce que l'interface ne dit pas d'elle-même — gestes, raccourcis,
    // et ce qu'un geste fait ou ne fait pas sur le serveur.
    Dialog {
        id: dlgAide
        title: qsTr("Aide de MMail")
        modal: true
        anchors.centerIn: Overlay.overlay
        // Taille fixée, et non déduite du texte : même raison que plus bas pour
        // le dialogue de retrait.
        width: Math.min(620, fenetre.width - 24)
        height: Math.min(620, fenetre.height - 24)
        standardButtons: Dialog.Close
        ScrollView {
            id: defilementAide
            anchors.fill: parent
            ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            // Réserve fixe, comme pour le corps du message : texte replié.
            rightPadding: ScrollBar.vertical.width
            contentWidth: availableWidth
            clip: true
            Label {
                width: defilementAide.availableWidth
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: qsTr(
                    "<b>Comptes</b><br>"
                    + "Menu « Comptes » : ajouter ou retirer un compte. À l'ajout, le serveur est "
                    + "cherché d'après l'adresse, si son domaine publie sa configuration. Un lien de "
                    + "configuration, reçu de votre administrateur, ajoute un ou plusieurs comptes "
                    + "sans rien saisir ; il ne sert qu'une fois. Retirer un compte efface de ce "
                    + "poste son index et son mot de passe mémorisé ; rien n'est supprimé sur le serveur. "
                    + "Clic droit sur un compte : se connecter, se déconnecter, oublier le mot de passe. "
                    + "Un clic sur le compte replie ou déplie ses dossiers.<br><br>"
                    + "<b>Rédaction</b><br>"
                    + "« Nouveau message » (Ctrl+N), « Répondre » (Ctrl+R), « Répondre à tous » "
                    + "(Ctrl+Maj+R), « Transférer » (Ctrl+F) — boutons au-dessus du message, ou clic "
                    + "droit. Un double clic répond ; dans les brouillons, il reprend le brouillon. "
                    + "Dans la fenêtre de rédaction : Ctrl+Entrée envoie, Ctrl+S enregistre le "
                    + "brouillon sur le serveur — ce que MMail fait aussi seul, 30 secondes après la "
                    + "première frappe puis chaque minute tant que le message change, "
                    + "et le message est gardé sur le poste toutes les 10 secondes : si MMail se "
                    + "ferme avant l'envoi, il le propose à la reprise au démarrage suivant ; "
                    + "« Joindre… » ou un glisser-déposer de fichiers "
                    + "ajoute des pièces jointes ; une capture d'écran se colle dans le corps (Ctrl+V), et "
                    + "une image glissée sur le corps s'y insère. Une copie de chaque message envoyé est gardée dans "
                    + "« Éléments envoyés ». Mise en forme : gras (Ctrl+B), italique (Ctrl+I), souligné "
                    + "(Ctrl+U), listes, liens ; décochez « Mise en forme » pour un message en texte "
                    + "brut. En tapant un destinataire, les adresses connues sont proposées : flèches "
                    + "et Entrée pour choisir. Nom affiché et signature de chaque compte : menu "
                    + "« Comptes », « Nom et signature… » — la signature peut porter des images "
                    + "(« Image… ») ou reprendre celle d'Outlook (« Importer une signature "
                    + "Outlook… », le fichier .htm de %APPDATA%\\Microsoft\\Signatures). "
                    + "Menu « Options » : importance, accusé "
                    + "de réception, confirmation de lecture, envoi différé — le message attend "
                    + "dans « Envoi différé » et part à l'heure dite si MMail est ouvert.<br><br>"
                    + "<b>Suivi</b><br>"
                    + "Un clic au bout de la seconde ligne d'un message, ou la touche Insertion, pose "
                    + "ou retire un drapeau de suivi, que les autres logiciels de messagerie voient "
                    + "aussi. « ! » signale un message d'importance haute, « ↓ » d'importance basse ; "
                    + "les pastilles « RE » (répondu) et « TR » (transféré) devant l'objet signalent ce "
                    + "qui a été fait du message, ici ou depuis un autre logiciel ; « RE TOUS » une réponse "
                    + "à tous faite depuis MMail (ailleurs, elle reste « RE »). "
                    + "Quand un expéditeur demande une confirmation de lecture, MMail propose de "
                    + "l'envoyer ou de l'ignorer.<br><br>"
                    + "<b>Favoris</b><br>"
                    + "Glissez un dossier sur la rubrique Favoris pour l'y épingler ; glissez un favori "
                    + "sur un autre pour le placer avant lui. Clic droit sur un dossier : ajouter aux "
                    + "favoris ou les retirer, masquer le dossier, créer un sous-dossier. Le bouton « Dossiers masqués » les "
                    + "réaffiche. Le chevron d'un dossier replie ou déplie ses sous-dossiers.<br><br>"
                    + "<b>Comptes</b><br>"
                    + "Glissez l'en-tête d'un compte sur un autre pour changer leur ordre, ou clic droit "
                    + "sur le compte : « Monter », « Descendre ».<br><br>"
                    + "<b>Recherche</b><br>"
                    + "Ctrl+E, puis Entrée : dans le dossier actif ou dans toutes les boîtes. Pendant une "
                    + "recherche, « Trier par » range les résultats : plus récents, plus anciens, "
                    + "expéditeur, objet, plus gros ; dossier, dans toutes les boîtes. Échap en sort.<br><br>"
                    + "<b>Trier</b><br>"
                    + "Glissez un ou plusieurs messages sur un dossier, de n'importe quel compte. Clic "
                    + "droit sur un message, ou Ctrl+Maj+V : « Déplacer vers… », avec un filtre sur le "
                    + "nom du dossier. Entre deux boîtes, le message n'est retiré de la source qu'une "
                    + "fois déposé dans la cible ; un déplacement interrompu reprend à la connexion "
                    + "suivante.<br><br>"
                    + "<b>Courrier indésirable</b><br>"
                    + "« Indésirable » (Ctrl+Alt+J) range le message dans le dossier d'indésirables de "
                    + "sa boîte : le filtre du serveur l'apprend et reconnaîtra les messages semblables. "
                    + "Depuis ce dossier, « Pas indésirable » le remet en boîte de réception, appris "
                    + "comme légitime.<br><br>"
                    + "<b>Clavier</b><br>"
                    + "Suppr : envoyer à la corbeille de la boîte, rien n'est détruit · Ctrl+Q : marquer "
                    + "comme lu · Ctrl+U : marquer comme non lu · Ctrl+Maj+V : déplacer vers… · "
                    + "Ctrl+A : tout sélectionner · F5 : actualiser · F1 : cette aide. "
                    + "Ctrl+clic et Maj+clic sélectionnent plusieurs messages.<br><br>"
                    + "<b>Affichage</b><br>"
                    + "Chaque colonne a son propre zoom : Ctrl + molette sur la colonne, ou Ctrl +, "
                    + "Ctrl − et Ctrl 0 sur la dernière colonne survolée ; pincement au doigt. Le menu "
                    + "« Affichage » propose trois apparences.<br><br>"
                    + "<b>Message</b><br>"
                    + "Un message HTML s'affiche mis en page, sur fond blanc. Ses images distantes ne "
                    + "sont téléchargées qu'à la demande (« Télécharger les images ») : les télécharger "
                    + "dit à l'expéditeur que le message a été ouvert. L'adresse d'un lien survolé "
                    + "s'affiche dans la barre du bas. "
                    + "Le bouton « Source » affiche le message brut, en-têtes compris. Un texte "
                    + "sélectionné part au presse-papier, sauf ce qu'un autre logiciel vient d'y "
                    + "déposer, protégé une minute.<br><br>"
                    + "<b>Pièces jointes</b><br>"
                    + "Une ligne par pièce sous l'en-tête du message : son type, son nom, sa taille, "
                    + "« Ouvrir » et « Enregistrer sous… ». Clic droit sur une pièce : « Ouvrir avec… » un "
                    + "autre logiciel, pour cette fois ou pour tous les fichiers du même type (menu "
                    + "Affichage, « Logiciels d'ouverture… », pour les revoir) ; « Joindre au message en "
                    + "cours » ou « Joindre à un nouveau message ». Un programme ou un script ne s'ouvre "
                    + "pas depuis MMail, il s'enregistre.")
            }
        }
    }

    Dialog {
        id: dlgAPropos
        title: qsTr("À propos de MMail")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Close
        ColumnLayout {
            width: dlgAPropos.availableWidth
            spacing: 8
            Label {
                text: "MMail " + Qt.application.version
                font.bold: true
                font.pointSize: fenetre.font.pointSize * 1.4
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Client de messagerie IMAP pour Windows, Linux et Android.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("© M-Media — logiciel libre, distribué sous licence GNU GPL version 3.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: qsTr("Sources : %1").arg("<a href=\"https://github.com/mmedia-fr/mmail\">github.com/mmedia-fr/mmail</a>")
                onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
            }
            // Version plus récente : son lien reste ici, même après « Jamais ».
            Label {
                visible: fenetre.versionNouvelle.length > 0
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: qsTr("Version %1 disponible : <a href=\"%2\">page de téléchargement</a>.")
                      .arg(fenetre.versionNouvelle).arg(fenetre.pageVersion)
                      + (installeur.programme() && fenetre.fichierMiseAJour.length > 0
                         && fenetre.genreMiseAJour === "setup"
                         ? " " + qsTr("Elle s'installera à la fermeture de MMail.") : "")
                onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                text: fenetre.noyau + " · Qt " + versionQt
            }
        }
    }

    Dialog {
        id: dlgRetrait
        property int compteId: 0
        property string adresse: ""
        property string hote: ""
        function ouvrir(c, a, h) { compteId = c; adresse = a; hote = h; open() }
        title: qsTr("Retirer le compte")
        modal: true
        anchors.centerIn: Overlay.overlay
        // Largeur fixée ici, et non déduite du texte : sans quoi le texte, qui
        // se replie selon la largeur, et le dialogue se calculent l'un l'autre.
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: dlgRetrait.availableWidth
            wrapMode: Text.Wrap
            text: qsTr("Retirer %1 de MMail ? L'index local de ce compte et son mot de passe mémorisé sont effacés de ce poste. Rien n'est supprimé sur le serveur.").arg(dlgRetrait.adresse)
        }
        onAccepted: {
            coffre.effacer(fenetre.cle(adresse, hote))
            boite.retirerCompte(compteId)
            fenetre.viderListe()
        }
    }

    // Pictogramme d'avertissement (triangle ambre « ! »), dessiné pour ne
    // dépendre d'aucune police d'émojis. Réutilisé par les dialogues de vidage.
    /// Pastille de la liste des messages : « RE », « RE TOUS », « TR ».
    component MarqueSuite: Rectangle {
        property alias texte: libelleMarque.text
        property string infobulle: ""
        implicitWidth: libelleMarque.implicitWidth + 10
        implicitHeight: libelleMarque.implicitHeight + 2
        radius: 3
        border.color: "white"
        Label {
            id: libelleMarque
            anchors.centerIn: parent
            font.bold: true
            font.pointSize: fenetre.tailleListe * 0.75
            color: "white"
        }
        HoverHandler { id: survolMarque }
        ToolTip.visible: survolMarque.hovered && infobulle.length > 0
        ToolTip.text: infobulle
        ToolTip.delay: 500
    }

    component Avertissement: Canvas {
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

    Dialog {
        id: dlgViderCorbeilles
        title: qsTr("Vider les corbeilles et indésirables ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: dlgViderCorbeilles.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Vider définitivement la corbeille et les dossiers d'indésirables (spam) de TOUS les comptes ? Les messages qui s'y trouvent sont effacés du serveur, sans repasser par une corbeille : l'action est irréversible.")
            }
        }
        onAccepted: {
            fenetre.messagesVides = 0
            boite.viderCorbeilles()
            messageEtat.texte = qsTr("Vidage des corbeilles et indésirables en cours…")
        }
    }

    Dialog {
        id: dlgViderDossier
        property int compte: 0
        property string chemin: ""
        property string nom: ""
        function ouvrir(c, ch, n) { compte = c; chemin = ch; nom = n; open() }
        title: qsTr("Vider ce dossier ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: dlgViderDossier.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Vider définitivement le dossier « %1 » ? Tous ses messages sont effacés du serveur, sans repasser par une corbeille : l'action est irréversible.").arg(dlgViderDossier.nom)
            }
        }
        onAccepted: {
            fenetre.messagesVides = 0
            boite.viderDossier(dlgViderDossier.compte, dlgViderDossier.chemin)
            messageEtat.texte = qsTr("Vidage de « %1 » en cours…").arg(dlgViderDossier.nom)
        }
    }

    // Sous-dossier créé sur le serveur, puis montré dans l'arborescence, son
    // parent déplié (signal `dossierCree`). Un refus immédiat — nom vide ou
    // interdit, dossier existant, compte hors ligne — reste dans le dialogue.
    Dialog {
        id: dlgNouveauDossier
        property int compte: 0
        property string cheminParent: ""
        property string nomParent: ""
        property string motif: ""
        function ouvrir(c, ch, n) {
            compte = c; cheminParent = ch; nomParent = n; motif = ""
            champNouveauDossier.text = ""
            open()
            champNouveauDossier.forceActiveFocus()
        }
        function creer() {
            motif = boite.creerSousDossier(compte, cheminParent, champNouveauDossier.text)
            if (motif.length > 0)
                return
            messageEtat.texte = qsTr("Création de « %1 » en cours…").arg(champNouveauDossier.text.trim())
            close()
        }
        title: qsTr("Nouveau sous-dossier")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(420, fenetre.width - 24)
        closePolicy: Popup.CloseOnEscape
        // « Créer » ne ferme pas d'office : un refus laisse corriger le nom.
        footer: DialogButtonBox {
            Button {
                text: qsTr("Créer")
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: dlgNouveauDossier.creer()
            }
            Button {
                text: qsTr("Annuler")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }
        ColumnLayout {
            width: dlgNouveauDossier.availableWidth
            spacing: 6
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Dans « %1 » :").arg(dlgNouveauDossier.nomParent)
            }
            TextField {
                id: champNouveauDossier
                Layout.fillWidth: true
                placeholderText: qsTr("Nom du dossier")
                onTextEdited: dlgNouveauDossier.motif = ""
                Keys.onReturnPressed: dlgNouveauDossier.creer()
                Keys.onEnterPressed: dlgNouveauDossier.creer()
            }
            Label {
                Layout.fillWidth: true
                visible: dlgNouveauDossier.motif.length > 0
                wrapMode: Text.Wrap
                color: "#b00020"
                text: dlgNouveauDossier.motif
            }
        }
    }

    // ------------------------------------------------------ coffre et session
    Coffre {
        id: coffre

        onLu: function(cle, secret, trouve, erreur) {
            var compte = fenetre.compteParCle(cle)
            if (!compte)
                return
            if (trouve) {
                boite.connecter(compte.hote, compte.adresse, secret)
                return
            }
            if (erreur.length > 0)
                messageEtat.texte = qsTr("Coffre du système indisponible : %1").arg(erreur)
            fenetre.demanderCompte(compte, "")
        }
        onEcrit: function(cle, ok, erreur) {
            if (!ok)
                messageEtat.texte = qsTr("Le coffre du système a refusé le mot de passe : %1").arg(erreur)
        }
        onEfface: function(cle, ok, erreur) {
            messageEtat.texte = ok
                    ? qsTr("Mot de passe retiré du coffre du système.")
                    : qsTr("Retrait du coffre impossible : %1").arg(erreur)
        }
    }

    // Secrets à confier au coffre une fois la connexion acceptée par le
    // serveur : on n'y range jamais un mot de passe que le serveur a refusé.
    property var secretsAConserver: ({})
    // Comptes dont le mot de passe est à demander, l'un après l'autre.
    property var aDemander: []
    property var essai: null

    Connections {
        target: boite

        function onConfigurationSauvegardee(empreinte, copies) {
            reglagesSauvegarde.empreinte = empreinte
            reglagesSauvegarde.date = Date.now()
            if (fenetre.sauvegardeDemandee)
                messageEtat.texte = qsTr("Configuration sauvegardée.")
            fenetre.sauvegardeDemandee = false
            if (fenetre.essai && fenetre.essai.scenario === "sauvegarde")
                console.log("scenario: sauvegardée,", copies, "copie(s)")
        }

        function onSauvegardeEchouee(message) {
            // Une sauvegarde automatique qui échoue se refera à la suivante :
            // elle ne se signale que si elle a été demandée.
            console.log("sauvegarde : " + message)
            if (fenetre.sauvegardeDemandee)
                messageEtat.texte = qsTr("Sauvegarde de la configuration impossible : %1").arg(message)
            fenetre.sauvegardeDemandee = false
        }

        function onSauvegardesTrouvees(compte, sauvegardes, erreur) {
            fenetre.sauvegardesTrouvees(sauvegardes, erreur)
        }

        function onConnecte(compte, adresse) {
            if (reglagesSauvegarde.aChercher && !fenetre.rechercheSauvegarde) {
                fenetre.rechercheSauvegarde = true
                boite.chercherSauvegardes(compte)
            }
            if (fenetre.essai && fenetre.essai.scenario === "sauvegarde" && fenetre.essai.etape === 0) {
                fenetre.essai.etape = 1
                var pauseSauvegarde = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
                pauseSauvegarde.triggered.connect(function() { fenetre.etapeScenarioSauvegarde(compte, adresse) })
                pauseSauvegarde.start()
            }
            // Les agendas suivent, une fois les connexions du démarrage faites.
            minuteurAgendas.interval = 5000
            minuteurAgendas.restart()
            // Scénario « agenda » : MMAIL_DOSSIER donne la vue (jour, semaine,
            // mois) ou « saisie » pour le formulaire d'un nouvel événement.
            if (fenetre.essai && (fenetre.essai.scenario === "agenda" || fenetre.essai.scenario === "agenda-ecriture")) {
                if (identifiantsEssai.dossier && identifiantsEssai.dossier !== "saisie" && identifiantsEssai.dossier.indexOf("fiche:") !== 0)
                    vueAgenda.genre = identifiantsEssai.dossier
                fenetre.module = "agenda"
            }
            var secret = fenetre.secretsAConserver[adresse]
            if (secret) {
                coffre.ecrire(secret.cle, secret.motDePasse)
                delete fenetre.secretsAConserver[adresse]
            }
            messageEtat.texte = qsTr("%1 connecté.").arg(adresse)
            if (fenetre.essai && fenetre.essai.connecter2) {
                var e2 = fenetre.essai.connecter2
                fenetre.essai.connecter2 = null
                boite.connecter(e2.hote, e2.adresse, e2.motDePasse)
            }
            // Premier compte prêt : sa boîte de réception est le point de
            // départ naturel.
            if (boite.dossierCourant.length === 0)
                fenetre.ouvrirDossier(compte, "INBOX")
            else if (fenetre.essai && fenetre.essai.scenario) {
                fenetre.etapeScenario()
                if (fenetre.essai.scenario === "glisser")
                    fenetre.etapeScenarioGlisser()
            }
        }

        function onDossierOuvert(compte, chemin, veille) {
            fenetre.derniereSynchro = new Date()
            if (compte !== boite.compteCourant || chemin !== boite.dossierCourant)
                return
            if (fenetre.essai && fenetre.essai.scenario === "envoi-fond" && fenetre.essai.depart && chemin !== "INBOX")
                console.log("scenario: « " + chemin + " » ouvert", Date.now() - fenetre.essai.depart, "ms après l'envoi")
            // Scénario « recherche » : le texte de MMAIL_DOSSIER est cherché dans
            // INBOX, une fois le dossier synchronisé.
            if (fenetre.essai && fenetre.essai.scenario === "recherche" && !fenetre.essai.cherche) {
                fenetre.essai.cherche = true
                champRecherche.text = identifiantsEssai.dossier
                fenetre.lancerRecherche()
                console.log("scenario: l'index trouve", modeleMessages.count, "message(s)")
                return
            }
            // Scénario « sous-dossier » : le sous-dossier MMAIL_DOSSIER de la
            // boîte de réception, créé par le dialogue du clic droit. Un nom
            // déjà pris ou interdit est refusé dans le dialogue même.
            if (fenetre.essai && fenetre.essai.scenario === "sous-dossier" && !fenetre.essai.cree) {
                fenetre.essai.cree = true
                dlgNouveauDossier.ouvrir(compte, "INBOX", qsTr("Boîte de réception"))
                champNouveauDossier.text = "a/b"
                dlgNouveauDossier.creer()
                console.log("scenario: refus de « a/b » :", dlgNouveauDossier.motif, "| dialogue ouvert :", dlgNouveauDossier.visible)
                champNouveauDossier.text = identifiantsEssai.dossier
                dlgNouveauDossier.creer()
                console.log("scenario: demande partie :", dlgNouveauDossier.motif === "", "| dialogue ouvert :", dlgNouveauDossier.visible)
                return
            }
            // Scénario « vidage » : « Vider les corbeilles et indésirables… »,
            // confirmé dans son dialogue, comme au clic. MMAIL_DOSSIER donne
            // l'attente en secondes après l'ouverture de la boîte de réception
            // (10 par défaut) : la session est alors en veille, comme chez
            // l'utilisateur.
            if (fenetre.essai && fenetre.essai.scenario === "vidage" && !fenetre.essai.vide) {
                fenetre.essai.vide = true
                var attenteVidage = Qt.createQmlObject('import QtQuick; Timer {}', fenetre)
                attenteVidage.interval = (parseInt(identifiantsEssai.dossier) || 10) * 1000
                attenteVidage.triggered.connect(function() {
                    dlgViderCorbeilles.open()
                    dlgViderCorbeilles.accept()
                    console.log("scenario: vidage demandé |", messageEtat.texte)
                })
                attenteVidage.start()
                return
            }
            // Scénario « historique » : douze recherches passées, proposées
            // sous le champ (dix visibles, un ascenseur).
            if (fenetre.essai && fenetre.essai.scenario === "historique" && !fenetre.essai.cherche) {
                fenetre.essai.cherche = true
                for (var h = 12; h >= 1; --h)
                    fenetre.noterRecherche(qsTr("recherche passée %1").arg(h))
                champRecherche.forceActiveFocus()
                fenetre.proposerHistorique()
                if (!panneauHistorique.opened) {
                    panneauHistorique.entrees = fenetre.historiqueRecherches()
                    panneauHistorique.open()
                }
                console.log("scenario: historique proposé", panneauHistorique.entrees.length, "| ouvert", panneauHistorique.opened)
            }
            if (fenetre.essai && fenetre.essai.scenario === "recherche-partout" && fenetre.essai.etape < 3) {
                fenetre.etapeScenarioRecherchePartout(compte, chemin)
                return
            }
            fenetre.rafraichirListe()
            // En recherche dans toutes les boîtes, la liste est celle des
            // résultats : ne pas l'annoncer comme le contenu du dossier.
            if (!veille && !fenetre.listeMulti)
                messageEtat.texte = qsTr("%1 : %2.").arg(fenetre.libelleCourant())
                        .arg(fenetre.accord(modeleMessages.count, qsTr("message"), qsTr("messages")))
            if (fenetre.essai && fenetre.essai.afficherPremier && modeleMessages.count > 0) {
                fenetre.essai.afficherPremier = false
                fenetre.choisir(0, 0)
            }
            if (fenetre.essai && fenetre.essai.scenario)
                fenetre.etapeScenario()
            if (fenetre.essai && fenetre.essai.scenario === "glisser")
                fenetre.etapeScenarioGlisser()
            if (fenetre.essai && fenetre.essai.scenario === "chevron")
                fenetre.etapeScenarioChevron()
            if (fenetre.essai && (fenetre.essai.scenario === "defilement" || fenetre.essai.scenario === "favoris-fixes"))
                fenetre.etapeScenarioDefilement()
            if (fenetre.essai && fenetre.essai.scenario === "joindre" && !fenetre.essai.choisi) {
                for (var k = 0; k < modeleMessages.count; ++k) {
                    if (modeleMessages.get(k).pieces) {
                        fenetre.essai.choisi = true
                        fenetre.choisir(k, 0)
                        break
                    }
                }
            }
            if (fenetre.essai && fenetre.essai.scenario.indexOf("signature") === 0)
                fenetre.etapeScenarioSignature()
        }

        function onDrapeauxModifies() {
            fenetre.rafraichirListe()
        }

        function onInvitationRecue(uid, invitation) {
            if (uid === fenetre.uidCourant)
                fenetre.invitation = invitation.length > 0 ? JSON.parse(invitation) : ({})
            // Scénario « invitation » : l'invitation affichée est acceptée par
            // le bandeau.
            if (fenetre.essai && fenetre.essai.scenario === "invitation" && invitation.length > 0) {
                var i = fenetre.invitation
                console.log("scenario: bandeau", i.invitation.methode, "|", i.invitation.titre, "|", i.invitation.quand,
                            "| dans l'agenda", i.dansAgenda, "| ma réponse", i.maReponse, "| planification", i.planification)
                if (i.invitation.methode === "REQUEST" && !fenetre.essai.repondu) {
                    fenetre.essai.repondu = true
                    fenetre.donnerSuite("ACCEPTED")
                }
            }
        }

        function onCorpsRecu(uid, texte, brut, html, bloquees, pieces, confirmation) {
            if (uid !== fenetre.uidCourant || brut !== fenetre.sourceVisible)
                return
            // Le format d'abord : le texte s'interprète selon lui.
            fenetre.lienSurvole = ""
            fenetre.corpsHtml = html
            fenetre.htmlAffiche = html ? texte : ""
            fenetre.imagesBloquees = bloquees
            vueCorps.text = html ? fenetre.htmlAuZoom(texte) : texte
            fenetre.bornerImages()
            vueCorps.cursorPosition = 0
            if (!brut) {
                fenetre.confirmationDemandee = confirmation
                fenetre.pieces = JSON.parse(pieces)
                // Le message entier dit s'il porte des pièces : la ligne suit.
                var i = fenetre.rangDe(uid)
                if (i >= 0 && modeleMessages.get(i).pieces !== (fenetre.pieces.length > 0))
                    modeleMessages.setProperty(i, "pieces", fenetre.pieces.length > 0)
            }
            if (fenetre.essai && fenetre.essai.scenario === "pieces")
                fenetre.etapeScenarioPieces()
            if (fenetre.essai && fenetre.essai.scenario === "ouvrir-avec")
                fenetre.etapeScenarioOuvrirAvec()
            // Scénario « joindre » : la première pièce du message affiché,
            // jointe à un nouveau message.
            if (fenetre.essai && fenetre.essai.scenario === "joindre" && !brut
                    && fenetre.pieces.length > 0 && !fenetre.essai.joint) {
                fenetre.essai.joint = true
                console.log("scenario: message", fenetre.sujetCourant(), "pièce", fenetre.pieces[0].nom)
                fenetre.joindrePiece(fenetre.pieces[0], "nouveau")
            }
            // Scénario « images » : le bouton « Télécharger les images »,
            // pressé une fois.
            if (fenetre.essai && fenetre.essai.scenario === "images" && html && bloquees > 0
                    && !fenetre.essai.imagesDemandees) {
                fenetre.essai.imagesDemandees = true
                boite.afficherImages(uid)
            }
        }

        function onPreparation(jeton, contenu) {
            var r = fenetre.redactions[jeton]
            if (r)
                r.remplir(JSON.parse(contenu))
        }

        function onEnvoye(jeton, avertissement) {
            if (fenetre.essai && fenetre.essai.scenario)
                console.log("scenario: envoyé", avertissement)
            if (fenetre.essai && fenetre.essai.scenario === "envoi-fond") {
                console.log("scenario: envoyé", Date.now() - fenetre.essai.depart, "ms après l'envoi | objet",
                            fenetre.essai.sujet)
                Qt.callLater(Qt.quit)
            }
            var r = fenetre.redactions[jeton]
            if (r)
                r.conteneur.fermerRedaction()
            messageEtat.texte = avertissement.length > 0
                    ? qsTr("Message envoyé — %1").arg(avertissement)
                    : qsTr("Message envoyé.")
            fenetre.rafraichirListe()
        }

        function onProgramme(jeton, echeance) {
            var r = fenetre.redactions[jeton]
            if (r)
                r.conteneur.fermerRedaction()
            messageEtat.texte = qsTr("Message programmé pour %1, dans « Envoi différé ». Il partira si MMail est ouvert à ce moment-là.")
                                .arg(fenetre.heureEnvoi(Number(echeance)))
        }

        function onDifferesEnvoyes(nombre, erreurs) {
            var texte = nombre > 0 ? qsTr("Envoi différé : %1.").arg(fenetre.accord(nombre, qsTr("message parti"), qsTr("messages partis"))) : ""
            if (erreurs.length > 0)
                texte += (texte.length > 0 ? " " : "") + qsTr("Échec : %1").arg(erreurs)
            if (texte.length > 0)
                messageEtat.texte = texte
            fenetre.rafraichirListe()
        }

        function onBrouillonEnregistre(jeton, uid) {
            var r = fenetre.redactions[jeton]
            if (!r)
                return
            r.brouillonEnregistre(uid)
            if (fenetre.essai && fenetre.essai.scenario === "garde-serveur")
                console.log("scenario: brouillon enregistré", uid, "| modifiée", r.modifie,
                            "| gardées après", JSON.parse(boite.redactionsGardees()).length, "| note", r.note)
            if (fenetre.essai && fenetre.essai.scenario === "brouillon-auto")
                fenetre.etapeScenarioBrouillonAuto(r, uid)
            if (fenetre.essai && fenetre.essai.scenario === "coller-image") {
                console.log("scenario: brouillon enregistré", uid, "| note", r.note)
                Qt.quit()
            }
        }

        function onAgendasSynchronises(change, erreurs) {
            if (erreurs.length > 0 && fenetre.module === "agenda")
                fenetre.annoncer(qsTr("Agenda : %1").arg(erreurs.split("\n")[0]), true)
            if (fenetre.essai && fenetre.essai.scenario === "agenda-ecriture")
                fenetre.etapeEcriture()
            // « fiche:aaaa-mm-jj » : la fiche du premier événement de ce jour.
            if (fenetre.essai && fenetre.essai.scenario === "agenda" && identifiantsEssai.dossier.indexOf("fiche:") === 0
                    && !fenetre.essai.fiche) {
                fenetre.essai.fiche = true
                vueAgenda.allerAuJour(identifiantsEssai.dossier.substring(6))
                Qt.callLater(function() {
                    var fiches = vueAgenda.donnees.fiches || []
                    var i = fiches.findIndex(function(f) { return (f.participants || []).length > 0 })
                    vueAgenda.montrerFiche(Math.max(0, i))
                })
            }
            if (fenetre.essai && fenetre.essai.scenario === "agenda" && identifiantsEssai.dossier === "saisie"
                    && !fenetre.essai.saisie) {
                fenetre.essai.saisie = true
                vueAgenda.nouvelEvenement(Qt.formatDate(new Date(), "yyyy-MM-dd"), 14 * 60, false)
            }
            if (fenetre.essai && fenetre.essai.scenario === "agenda") {
                var v = JSON.parse(boite.vueAgenda("semaine", Qt.formatDate(new Date(), "yyyy-MM-dd")))
                console.log("scenario: agendas synchronisés | changement", change, "| erreurs", JSON.stringify(erreurs),
                            "| agendas", JSON.parse(boite.agendas()).length, "| cette semaine", v.nombre,
                            "| titre", v.titre)
            }
        }

        function onEvenementEnregistre(ok, message) {
            if (fenetre.essai && fenetre.essai.scenario === "invitation")
                console.log("scenario: écriture", ok, JSON.stringify(message), "| réponse", fenetre.reponseInvitation)
            if (fenetre.reponseInvitation.length > 0) {
                var action = fenetre.reponseInvitation
                fenetre.reponseInvitation = ""
                if (ok) {
                    var i = Object.assign({}, fenetre.invitation)
                    if (action === "RETIRER") {
                        i.dansAgenda = false
                        fenetre.annoncer(qsTr("Réunion retirée de l'agenda."), false)
                    } else {
                        i.maReponse = action
                        i.dansAgenda = action !== "DECLINED"
                        fenetre.annoncer(action === "DECLINED" ? qsTr("Invitation refusée : l'organisateur en est averti.")
                                                               : qsTr("Réponse envoyée à l'organisateur ; la réunion est dans l'agenda."), false)
                    }
                    fenetre.invitation = i
                } else {
                    fenetre.annoncer(qsTr("Réponse impossible : %1").arg(message), true)
                }
            }
            if (fenetre.essai && fenetre.essai.scenario === "agenda-ecriture")
                console.log("scenario: écriture", fenetre.essai.etape || 0, ok, JSON.stringify(message))
        }

        function onEchecRedaction(jeton, message) {
            if (fenetre.essai && fenetre.essai.scenario)
                console.log("scenario: échec de la rédaction :", message)
            var r = fenetre.redactions[jeton]
            if (r)
                r.echouer(message)
            else
                messageEtat.texte = message
        }

        // Seules les lignes marquées changent : la liste n'est pas relue.
        function onLusModifies(uids, lu) {
            var liste = uids.split(",")
            for (var k = 0; k < liste.length; ++k) {
                var i = fenetre.rangDe(parseInt(liste[k]))
                if (i >= 0 && modeleMessages.get(i).lu !== lu)
                    modeleMessages.setProperty(i, "lu", lu)
            }
            fenetre.majInfoDossier()
        }

        function onPieceAJoindre(url) {
            var r = fenetre.redactions[fenetre.pieceCible]
            if (!r) {
                r = fenetre.ouvrirRedaction(boite.compteCourant > 0 ? boite.compteCourant
                                                                    : fenetre.comptesConnus[0].compte)
                r.pret()
            }
            fenetre.pieceCible = ""
            r.ajouterFichiers([url])
            if (fenetre.essai && fenetre.essai.scenario === "joindre")
                console.log("scenario: pièces de la rédaction :", JSON.stringify(r.pieces.map(function(p) { return p.nom })))
            if (r.conteneur && r.conteneur.raise) {
                r.conteneur.raise()
                r.conteneur.requestActivate()
            }
            messageEtat.texte = qsTr("Pièce jointe ajoutée au message « %1 ».").arg(r.objetActuel())
        }

        function onPieceEcrite(url, chemin, ouvrir) {
            if (ouvrir) {
                // Le logiciel d'« Ouvrir avec… », ou celui retenu pour
                // l'extension ; à défaut, ou s'il a disparu, celui du système.
                var nom = chemin.replace(/\\/g, "/").split("/").pop()
                var choisi = fenetre.ouvertureAvec
                fenetre.ouvertureAvec = null
                var programme = choisi && choisi.extension === fenetre.extensionDe(nom) ? choisi.programme
                              : Qt.platform.os === "android" ? "" : fenetre.logicielPour(nom)
                if (programme.length > 0 && lanceur.lancer(programme, chemin)) {
                    messageEtat.texte = qsTr("Ouverture de %1 avec %2…").arg(chemin).arg(lanceur.nom(programme))
                } else if (!Qt.openUrlExternally(url)) {
                    messageEtat.texte = qsTr("Aucun logiciel n'a pu ouvrir %1.").arg(chemin)
                } else {
                    messageEtat.texte = programme.length > 0
                            ? qsTr("%1 n'a pas pu être lancé : ouverture de %2 avec le logiciel du système…")
                              .arg(lanceur.nom(programme)).arg(chemin)
                            : qsTr("Ouverture de %1…").arg(chemin)
                }
            } else {
                messageEtat.texte = qsTr("Pièce jointe enregistrée : %1").arg(chemin)
            }
            if (fenetre.essai && fenetre.essai.scenario === "pieces")
                console.log("scenario: piece ecrite", chemin, ouvrir)
        }

        // Ce que le serveur a trouvé dans le texte des messages, ajouté aux
        // résultats de l'index si la recherche n'a pas changé entre-temps.
        function onResultatsServeur(compte, chemin, texte, uids) {
            if (fenetre.essai && fenetre.essai.scenario === "recherche")
                console.log("scenario: le serveur trouve", uids.length > 0 ? uids.split(",").length : 0,
                            "message(s) pour", texte)
            if (texte !== fenetre.recherche || compte !== boite.compteCourant || chemin !== boite.dossierCourant)
                return
            fenetre.uidsServeur = uids.length > 0 ? uids.split(",").map(Number) : []
            fenetre.poserResultats()
            messageEtat.texte = qsTr("Recherche « %1 » : %2.").arg(texte)
                    .arg(fenetre.accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats")))
            if (fenetre.essai && fenetre.essai.scenario === "recherche") {
                console.log("scenario: liste après le serveur :", modeleMessages.count, "| premier :",
                            modeleMessages.count > 0 ? modeleMessages.get(0).sujet : "—")
                // Chaque tri, ses trois premières lignes.
                for (var t = 0; t < fenetre.trisRecherche.length; ++t) {
                    var cle = fenetre.trisRecherche[t].cle
                    fenetre.trierRecherche(cle)
                    var tete = []
                    for (var r = 0; r < Math.min(3, modeleMessages.count); ++r) {
                        var m = modeleMessages.get(r)
                        tete.push(new Date(m.h * 1000).toISOString().slice(0, 10) + " " + m.taille + " o "
                                  + (m.expediteur || m.adresse) + " — " + m.sujet)
                    }
                    console.log("scenario: tri", cle, "(" + modeleMessages.count + ") :", tete.join(" | "))
                }
                fenetre.trierRecherche("recents")
                Qt.quit()
            }
        }

        function onDeplacementTermine(nombre, erreurs) {
            fenetre.deplacementsEnVol = Math.max(0, fenetre.deplacementsEnVol - 1)
            if (fenetre.deplacementsEnVol === 0)
                fenetre.enDeplacement = ({})
            // En échec, les lignes retirées d'avance doivent revenir : le noyau
            // ne les a jamais vues partir, il faut la liste entière.
            fenetre.rafraichirListe(erreurs.length > 0)
            if (erreurs.length === 0)
                messageEtat.texte = fenetre.accord(nombre, qsTr("message déplacé."), qsTr("messages déplacés."))
            if (fenetre.essai && fenetre.essai.scenario) {
                console.log("scenario: deplacement termine", nombre, erreurs)
                fenetre.etapeScenario()
                if (fenetre.essai.scenario === "glisser")
                    fenetre.etapeScenarioGlisser()
            }
        }

        function onDossierCree(compte, chemin, nom) {
            messageEtat.texte = qsTr("Dossier « %1 » créé.").arg(nom)
            if (fenetre.essai && fenetre.essai.scenario === "sous-dossier") {
                // Laisser le temps à l'arborescence de se redessiner.
                Qt.callLater(function() {
                    var ligne = null
                    for (var i = 0; i < modeleArborescence.count; ++i) {
                        var l = modeleArborescence.get(i)
                        if (l.genre === "dossier" && l.compte === compte && l.chemin === chemin)
                            ligne = l
                    }
                    console.log("scenario: créé", chemin, "| dans l'arborescence :", ligne !== null,
                                ligne ? "| nom : " + ligne.nom + " | profondeur : " + ligne.profondeur : "")
                    dlgNouveauDossier.ouvrir(compte, "INBOX", qsTr("Boîte de réception"))
                    champNouveauDossier.text = nom
                    dlgNouveauDossier.creer()
                    console.log("scenario: second essai du même nom :", dlgNouveauDossier.motif)
                    Qt.quit()
                })
            }
        }

        function onCorbeillesVidees(nombre) {
            fenetre.messagesVides += nombre
            fenetre.rafraichirListe()
            messageEtat.texte = fenetre.accord(fenetre.messagesVides,
                qsTr("message effacé des corbeilles et indésirables."),
                qsTr("messages effacés des corbeilles et indésirables."))
            if (fenetre.essai && fenetre.essai.scenario === "vidage")
                fenetre.bilanScenarioVidage("vidées, " + nombre + " message(s) | " + messageEtat.texte)
        }

        function onEchec(compte, etape, message) {
            if (fenetre.essai && fenetre.essai.scenario === "vidage")
                fenetre.bilanScenarioVidage("échec du compte " + compte + " | étape " + etape + " | " + message)
            if (etape === "tri") {
                // Le compte des déplacements en cours n'est pas touché ici : un
                // déplacement rend toujours « deplacementTermine », réussi ou
                // non ; un « tri » échoué peut aussi être un marquage.
                fenetre.rafraichirListe()
            } else if (etape === "identifiants") {
                // Mot de passe refusé : on redemande, sans effacer d'office
                // celui du coffre — il a pu changer côté serveur.
                var c = fenetre.compteParId(compte)
                fenetre.demanderCompte(c ? c : fenetre.dernierDemande, message)
            } else if (etape === "connexion" && fenetre.dernierDemande
                       && !fenetre.compteParId(compte)) {
                // Premier essai d'un compte nouveau : il n'a pas été conservé,
                // on rouvre le dialogue tel qu'il était.
                fenetre.demanderCompte(fenetre.dernierDemande, message)
            } else if (etape === "message") {
                fenetre.afficherTexte("")
            }
        }
    }

    // ------------------------------------------------------------- logique
    property var dernierDemande: null

    function cle(adresse, hote) {
        return adresse + "@" + hote
    }

    function listeComptes() {
        return JSON.parse(boite.comptes())
    }

    function compteParCle(c) {
        var comptes = listeComptes()
        for (var i = 0; i < comptes.length; ++i)
            if (cle(comptes[i].adresse, comptes[i].hote) === c)
                return comptes[i]
        return null
    }

    function compteParId(id) {
        var comptes = listeComptes()
        for (var i = 0; i < comptes.length; ++i)
            if (comptes[i].id === id)
                return comptes[i]
        return null
    }

    /// Trombone de la liste des messages, tracé plutôt que pris d'une police :
    /// le caractère 📎 n'existe pas partout, et une icône en couleur ne
    /// suivrait ni l'apparence ni la surbrillance. Dessiné sur une grille de
    /// 10 × 16, mise à l'échelle de la ligne.
    function dessinerTrombone(ctx, w, h, encre) {
        var x = function(v) { return v * w / 10 }
        var y = function(v) { return v * h / 16 }
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.lineWidth = Math.max(1.2, w * 0.13)
        ctx.lineCap = "round"
        ctx.lineJoin = "round"
        ctx.beginPath()
        ctx.moveTo(x(7.5), y(5))
        ctx.lineTo(x(7.5), y(11.5))
        ctx.arc(x(5), y(11.5), x(2.5), 0, Math.PI, false)
        ctx.lineTo(x(2.5), y(3.5))
        ctx.arc(x(4.25), y(3.5), x(1.75), Math.PI, 0, false)
        ctx.lineTo(x(6), y(10.5))
        ctx.arc(x(5), y(10.5), x(1), 0, Math.PI, false)
        ctx.lineTo(x(4), y(5.5))
        ctx.stroke()
    }

    /// Loupe du champ de recherche : un cercle et un manche.
    function dessinerLoupe(ctx, w, h, encre) {
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.lineWidth = Math.max(1.5, w * 0.14)
        ctx.lineCap = "round"
        ctx.beginPath()
        ctx.arc(w * 0.42, h * 0.42, w * 0.3, 0, 2 * Math.PI, false)
        ctx.moveTo(w * 0.64, h * 0.64)
        ctx.lineTo(w * 0.92, h * 0.92)
        ctx.stroke()
    }

    /// Drapeau de suivi : une hampe et une flamme, pleine ou en creux.
    function dessinerDrapeau(ctx, w, h, encre, plein) {
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.fillStyle = encre
        ctx.lineWidth = Math.max(1.2, w * 0.12)
        ctx.lineJoin = "round"
        ctx.beginPath()
        ctx.moveTo(w * 0.22, h * 0.08)
        ctx.lineTo(w * 0.22, h * 0.96)
        ctx.stroke()
        ctx.beginPath()
        ctx.moveTo(w * 0.22, h * 0.1)
        ctx.lineTo(w * 0.9, h * 0.3)
        ctx.lineTo(w * 0.22, h * 0.52)
        ctx.closePath()
        if (plein)
            ctx.fill()
        else
            ctx.stroke()
    }

    function basculerSuivi(uids, suivi) {
        if (uids.length === 0)
            return
        boite.marquerSuivi(uids.join(","), suivi)
        for (var i = 0; i < uids.length; ++i) {
            var j = indexDe(uids[i])
            if (j >= 0)
                modeleMessages.setProperty(j, "suivi", suivi)
        }
    }

    /// « mardi 30 septembre à 08:00 » pour une heure d'envoi (secondes Unix).
    function heureEnvoi(secondes) {
        var d = new Date(secondes * 1000)
        return d.toLocaleDateString(Qt.locale("fr_FR"), "dddd d MMMM") + qsTr(" à ")
             + Qt.formatTime(d, "hh:mm")
    }

    /// Taille de police d'une colonne, zoom compris : les menus et dialogues
    /// ouverts depuis une colonne la reprennent.
    function tailleColonne(colonne) {
        return tailleBase * zoomDe(colonne)
    }

    /// Largeur d'un menu : celle de sa plus longue entrée. Le style Fusion la
    /// fixe sinon à 200 pixels, et tronque ce qui dépasse. Appelée avant
    /// l'affichage : les entrées n'y sont pas encore visibles, d'où aucun tri
    /// sur `visible`.
    function largeurMenu(menu) {
        var largeur = 0
        for (var i = 0; i < menu.count; ++i) {
            var entree = menu.itemAt(i)
            if (entree)
                largeur = Math.max(largeur, entree.implicitWidth)
        }
        return Math.max(200, largeur + menu.leftPadding + menu.rightPadding)
    }

    function dialogueOuvert() {
        return dlgCompte.visible || dlgDeplacer.visible || dlgRetrait.visible
            || dlgAide.visible || dlgAPropos.visible || dlgLien.visible || dlgIdentite.visible
            || dlgNouveauDossier.visible
            // Sur téléphone, la rédaction couvre la fenêtre principale.
            || (compact && Object.keys(redactions).length > 0)
    }

    /// Ouvre le dialogue de compte : `compte` nul pour un nouveau compte.
    function demanderCompte(compte, note) {
        if (dlgCompte.visible) {
            if (compte)
                aDemander.push({ compte: compte, note: note })
            return
        }
        dernierDemande = compte
        dlgCompte.compteId = compte && compte.id ? compte.id : 0
        champAdresse.text = compte ? compte.adresse : ""
        champHote.text = compte ? compte.hote : ""
        champMotDePasse.text = ""
        caseMemoriser.checked = reglages.memoriser
        noteCompte.text = note
        dlgCompte.hoteSaisi = champHote.text.length > 0
        dlgCompte.infoServeur = ""
        dlgCompte.open()
    }

    /// Configuration automatique : le serveur se déduit de l'adresse, tant que
    /// la personne ne l'a pas saisi elle-même.
    function chercherServeur() {
        var adresse = champAdresse.text.trim()
        if (dlgCompte.compteId > 0 || dlgCompte.hoteSaisi || adresse.indexOf("@") < 1)
            return
        dlgCompte.infoServeur = qsTr("Recherche du serveur…")
        configuration.decouvrir(adresse)
    }

    function serveurDecouvert(adresse, hote) {
        // Réponse tardive : l'adresse a changé, ou le serveur a été saisi.
        if (!dlgCompte.visible || dlgCompte.hoteSaisi || champAdresse.text.trim() !== adresse)
            return
        if (hote.length > 0) {
            champHote.text = hote
            dlgCompte.infoServeur = qsTr("Serveur trouvé automatiquement.")
        } else {
            dlgCompte.infoServeur = qsTr("Serveur introuvable automatiquement : saisissez-le.")
        }
    }

    function lireLien(lien) {
        if (lien.length === 0)
            return
        lienEnCours = lien
        messageEtat.texte = qsTr("Lecture du lien de configuration…")
        configuration.lireLien(lien)
    }

    /// Document d'un lien de configuration (cf. README) :
    /// `{"mmail": 1, "comptes": [{"adresse", "hote", "motDePasse"}]}`.
    function appliquerLien(contenu) {
        var lien = lienEnCours
        lienEnCours = ""
        var document = null
        try {
            document = JSON.parse(contenu)
        } catch (e) {
            document = null
        }
        var comptes = document && document.mmail === 1 && Array.isArray(document.comptes)
                ? document.comptes : []
        var retenus = []
        var ecartes = []
        for (var i = 0; i < comptes.length; ++i) {
            var c = comptes[i]
            var valide = c !== null && typeof c === "object"
                    && typeof c.adresse === "string" && c.adresse.trim().indexOf("@") > 0
                    && typeof c.hote === "string" && c.hote.trim().length > 0
                    && typeof c.motDePasse === "string" && c.motDePasse.length > 0
                    && (c.port === undefined || c.port === 993)
            if (!valide) {
                ecartes.push(c && typeof c.adresse === "string" ? c.adresse : "?")
                continue
            }
            retenus.push({ adresse: c.adresse.trim(), hote: c.hote.trim().toLowerCase(),
                           motDePasse: c.motDePasse })
        }
        if (retenus.length === 0) {
            // Le lien est consommé : le rendre au dialogue ne servirait qu'à
            // montrer ce qui a été reçu.
            dlgLien.ouvrir("", qsTr("Ce lien ne contient aucun compte utilisable par MMail."))
            return
        }
        // Un compte venu d'un lien n'a pas de mot de passe connu de la
        // personne : il ne vit que par le coffre.
        reglages.memoriser = true
        for (var j = 0; j < retenus.length; ++j) {
            var r = retenus[j]
            secretsAConserver[r.adresse] = { cle: cle(r.adresse, r.hote), motDePasse: r.motDePasse }
            boite.connecter(r.hote, r.adresse, r.motDePasse)
        }
        var texte = qsTr("Lien de configuration : connexion de %1.")
                .arg(retenus.map(function(r) { return r.adresse }).join(", "))
        if (ecartes.length > 0)
            texte += " " + qsTr("Écarté : %1.").arg(ecartes.join(", "))
        messageEtat.texte = texte
    }

    function demandeSuivante() {
        if (aDemander.length > 0) {
            var suivante = aDemander.shift()
            demanderCompte(suivante.compte, suivante.note)
        }
    }

    function validerCompte() {
        var adresse = champAdresse.text.trim()
        var hote = champHote.text.trim()
        var motDePasse = champMotDePasse.text
        champMotDePasse.text = ""
        reglages.memoriser = caseMemoriser.checked
        dernierDemande = { id: dlgCompte.compteId, adresse: adresse, hote: hote }
        if (caseMemoriser.checked)
            secretsAConserver[adresse] = { cle: cle(adresse, hote), motDePasse: motDePasse }
        else
            coffre.effacer(cle(adresse, hote))
        messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
        boite.connecter(hote, adresse, motDePasse)
        demandeSuivante()
    }

    // ---- sauvegarde de la configuration

    /// Les réglages qui suivent la personne d'un poste à l'autre, par
    /// catégorie. Restent sur le poste : la fenêtre, les logiciels choisis
    /// pour les pièces jointes, la mise à jour et la sauvegarde elle-même.
    function reglagesSauvegardes() {
        return { interface: reglages, redaction: reglagesRedaction, edition: reglagesEdition,
                 apparence: reglagesApparence, agenda: vueAgenda.reglagesAgenda }
    }

    /// Valeurs des réglages, lues propriété par propriété : un réglage ajouté
    /// plus tard part de lui-même.
    function lireReglages() {
        var objets = reglagesSauvegardes()
        var tout = {}
        for (var categorie in objets) {
            var o = objets[categorie]
            var valeurs = {}
            for (var cle in o) {
                if (cle === "objectName" || cle === "category" || cle === "fileName")
                    continue
                var v = o[cle]
                if (typeof v === "boolean" || typeof v === "number" || typeof v === "string")
                    valeurs[cle] = v
            }
            tout[categorie] = valeurs
        }
        return tout
    }

    /// Pose des réglages restaurés, chacun converti au type du réglage : ce
    /// qui n'existe plus est ignoré.
    function poserReglages(tout) {
        var objets = reglagesSauvegardes()
        for (var categorie in tout) {
            var o = objets[categorie]
            var valeurs = tout[categorie]
            if (!o || !valeurs || typeof valeurs !== "object")
                continue
            for (var cle in valeurs) {
                // Numéro d'agenda propre à l'index de l'autre poste.
                if (cle === "objectName" || cle === "category" || cle === "fileName"
                        || cle === "agendaParDefaut" || !(cle in o))
                    continue
                var actuel = o[cle], v = valeurs[cle]
                if (typeof actuel === "boolean")
                    o[cle] = v === true || v === "true"
                else if (typeof actuel === "number" && isFinite(Number(v)))
                    o[cle] = Number(v)
                else if (typeof actuel === "string")
                    o[cle] = String(v)
            }
        }
    }

    /// Sauvegarde la configuration si elle a changé depuis la dernière ;
    /// `demandee` : par le menu — elle part alors même sans changement.
    function sauvegarderConfiguration(demandee) {
        if (reglagesSauvegarde.aChercher && !demandee)
            return
        if (reglagesSauvegarde.poste.length === 0)
            reglagesSauvegarde.poste = boite.nouveauPoste()
        var lance = boite.sauvegarderConfiguration(JSON.stringify(lireReglages()), reglagesSauvegarde.poste,
                                                    demandee ? "" : reglagesSauvegarde.empreinte,
                                                    reglages.memoriser)
        if (demandee) {
            sauvegardeDemandee = lance
            messageEtat.texte = lance ? qsTr("Sauvegarde de la configuration…")
                                      : qsTr("Sauvegarde impossible pour l'instant : aucun compte connecté, ou une sauvegarde en cours.")
        }
    }

    /// Cherche les sauvegardes que la clé du compte courant — à défaut, du
    /// premier compte connecté — ouvre.
    function chercherSauvegardes(demandee) {
        var comptes = JSON.parse(boite.comptes())
        var connectes = comptes.filter(function(c) { return c.etat === "pret" })
        var choisi = connectes.filter(function(c) { return c.id === boite.compteCourant })[0] || connectes[0]
        if (!choisi) {
            messageEtat.texte = qsTr("Aucun compte connecté : la sauvegarde se retrouve par le mot de passe d'un compte.")
            return
        }
        rechercheSauvegarde = true
        rechercheManuelle = demandee
        messageEtat.texte = qsTr("Recherche des sauvegardes de %1…").arg(choisi.adresse)
        boite.chercherSauvegardes(choisi.id)
    }

    function sauvegardesTrouvees(texte, erreur) {
        rechercheSauvegarde = false
        var manuelle = rechercheManuelle
        rechercheManuelle = false
        if (erreur.length > 0) {
            // Recherche du premier compte : elle se refera à la prochaine
            // connexion.
            messageEtat.texte = qsTr("Sauvegardes inaccessibles : %1").arg(erreur)
            return
        }
        var liste = []
        try {
            liste = JSON.parse(texte)
        } catch (e) {
            liste = []
        }
        if (!manuelle) {
            reglagesSauvegarde.aChercher = false
            // Ce poste-ci n'a rien à se proposer.
            liste = liste.filter(function(s) { return s.poste !== reglagesSauvegarde.poste })
        }
        if (fenetre.essai && fenetre.essai.scenario === "restauration") {
            console.log("scenario: sauvegardes trouvées :", liste.length)
            if (liste.length > 0)
                restaurerConfiguration(liste[0])
            return
        }
        if (liste.length === 0) {
            if (manuelle)
                messageEtat.texte = qsTr("Aucune sauvegarde de configuration pour ce compte.")
            return
        }
        sauvegardesProposees = liste
        dlgRestauration.choix = 0
        dlgRestauration.open()
    }

    /// Restaure la sauvegarde choisie : comptes et réglages ; les comptes
    /// qui ne sont pas connectés se connectent — avec le mot de passe
    /// sauvegardé, ou en le demandant.
    function restaurerConfiguration(s) {
        var texte = boite.restaurerConfiguration(s.poste)
        if (texte.length === 0) {
            messageEtat.texte = qsTr("Restauration impossible : %1").arg(boite.erreur)
            return
        }
        var r = JSON.parse(texte)
        poserReglages(r.reglages || {})
        // Le même ordinateur, réinstallé : il reprend sa place chez le
        // service plutôt que d'y paraître deux fois.
        if (s.memeNom)
            reglagesSauvegarde.poste = s.poste
        reglagesSauvegarde.empreinte = ""
        reglagesSauvegarde.aChercher = false
        var presents = JSON.parse(boite.comptes())
        var connectes = 0
        for (var i = 0; i < (r.comptes || []).length; ++i) {
            var c = r.comptes[i]
            var p = presents.filter(function(x) { return x.adresse === c.adresse })[0]
            if (p && (p.etat === "pret" || p.etat === "connexion"))
                continue
            var hote = p ? p.hote : c.hote
            if (typeof c.motDePasse === "string" && c.motDePasse.length > 0) {
                if (reglages.memoriser)
                    secretsAConserver[c.adresse] = { cle: cle(c.adresse, hote), motDePasse: c.motDePasse }
                boite.connecter(hote, c.adresse, c.motDePasse)
                ++connectes
            } else {
                demanderCompte({ id: p ? p.id : 0, adresse: c.adresse, hote: hote },
                               qsTr("Le mot de passe de ce compte n'était pas sauvegardé."))
            }
        }
        rafraichirArborescence()
        messageEtat.texte = qsTr("Configuration du %1 restaurée.").arg(dateLongue(s.date * 1000))
        if (fenetre.essai && fenetre.essai.scenario === "restauration") {
            console.log("scenario: restaurée,", (r.comptes || []).length, "compte(s),", r.images, "image(s),", connectes, "connexion(s)")
            var bilan = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            bilan.triggered.connect(bilanScenarioRestauration)
            bilan.start()
        }
    }

    function connecterCompte(compte) {
        if (reglages.memoriser)
            coffre.lire(cle(compte.adresse, compte.hote))
        else
            demanderCompte(compte, "")
    }

    // ---- arborescence

    /// Toutes les lignes ont la même forme : un ListModel veut des rôles
    /// constants d'une ligne à l'autre.
    function normaliser(l) {
        return {
            genre: l.genre,
            compte: l.compte || 0,
            adresse: l.adresse || "",
            hote: l.hote || "",
            etat: l.etat || "",
            replie: l.replie === true,
            motif: l.motif || "",
            chemin: l.chemin || "",
            nom: l.nom || "",
            profondeur: l.profondeur || 0,
            role: l.role || "",
            selectionnable: l.selectionnable !== false,
            messages: l.messages || 0,
            nonLus: l.nonLus || 0,
            masque: l.masque === true,
            favori: l.favori === true,
            enfants: l.enfants === true,
            // Section de la liste : la rubrique Favoris, puis un groupe par compte.
            groupe: l.genre === "rubrique" || l.genre === "favori" || l.genre === "favori-vide"
                    ? "favoris" : "compte-" + (l.compte || 0)
        }
    }

    function clefLigne(l) {
        return l.genre + "\u0001" + l.compte + "\u0001" + l.chemin
    }

    /// Relit l'arborescence. Même structure : les lignes sont mises à jour en
    /// place, sans que la vue ne revienne en haut.
    function rafraichirArborescence() {
        var lignes = JSON.parse(boite.arborescence(reglages.afficherMasques)).map(normaliser)
        poserLignes(modeleFavoris, lignes.filter(function(l) { return l.groupe === "favoris" }))
        poserLignes(modeleArborescence, lignes.filter(function(l) { return l.groupe !== "favoris" }))
        var comptes = lignes.filter(function(l) { return l.genre === "compte" })
                            .map(function(l) { return { compte: l.compte, adresse: l.adresse, hote: l.hote } })
        // Réaffecté seulement s'il change : le menu reconstruit ses entrées à
        // chaque affectation, et l'arborescence est relue à chaque veille.
        if (JSON.stringify(comptes) !== JSON.stringify(comptesConnus))
            comptesConnus = comptes
        majInfoDossier()
    }

    /// Même structure : les lignes sont mises à jour en place, sans que la
    /// vue ne revienne en haut.
    function poserLignes(modele, lignes) {
        var memeForme = lignes.length === modele.count
        for (var i = 0; memeForme && i < lignes.length; ++i)
            memeForme = clefLigne(lignes[i]) === clefLigne(modele.get(i))
        if (memeForme) {
            for (var j = 0; j < lignes.length; ++j)
                modele.set(j, lignes[j])
        } else {
            modele.clear()
            modele.append(lignes)
        }
    }

    function libelleLigne(l) {
        if (l.genre === "rubrique")
            return qsTr("Favoris")
        if (l.genre === "favori-vide")
            return qsTr("Glissez un dossier ici")
        if (l.genre === "compte")
            return l.adresse
        var nom = l.nom
        if (l.role === "Sent") nom = qsTr("Éléments envoyés")
        else if (l.role === "Drafts") nom = qsTr("Brouillons")
        else if (l.role === "Trash") nom = qsTr("Éléments supprimés")
        else if (l.role === "Junk") nom = qsTr("Courrier indésirable")
        else if (l.role === "Archive") nom = qsTr("Archives")
        else if (l.chemin === "INBOX") nom = qsTr("Boîte de réception")
        if (l.genre === "favori")
            return nom + " — " + l.adresse
        return nom
    }

    function libelleCourant() {
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre === "dossier" && l.compte === boite.compteCourant
                    && l.chemin === boite.dossierCourant)
                return libelleLigne(l)
        }
        return boite.dossierCourant
    }

    // ---- liste des messages

    function ouvrirDossier(compte, chemin) {
        // Un dossier choisi dans l'arborescence met fin à une recherche dans
        // toutes les boîtes, même s'il est celui du dernier résultat lu.
        var sortieGlobale = rechercheGlobale || vueUnifiee
        if (sortieGlobale) {
            sortirRechercheGlobale()
            vueUnifiee = false
            unifieeAvantRecherche = false
            champRecherche.text = ""
        }
        if (sortieGlobale || compte !== boite.compteCourant || chemin !== boite.dossierCourant) {
            recherche = ""
            uidsServeur = []
            if (!porteeRecherche.partout)
                champRecherche.text = ""
            viderListe()
            if (boite.ouvrirDossier(compte, chemin)) {
                // L'index s'affiche tout de suite ; la synchronisation suit —
                // sauf hors connexion, que le noyau vient de signaler.
                rafraichirListe(sortieGlobale)
                if (boite.erreur.indexOf("compte hors ligne") !== 0)
                    messageEtat.texte = qsTr("Lecture de %1…").arg(libelleCourant())
            }
        } else {
            boite.ouvrirDossier(compte, chemin)
        }
        if (compact)
            vue = 1
    }

    function viderListe() {
        arreterSuite()
        pieces = []
        modeleMessages.clear()
        selection = ({})
        ancre = -1
        uidCourant = 0
        sujetAffiche.text = ""
        auteurAffiche.text = ""
        afficherTexte(qsTr("Aucun message sélectionné."))
    }

    /// Relit la liste depuis l'index. Le noyau ne rend que ce qui a changé
    /// depuis sa lecture précédente — retirés, ajoutés, modifiés —, appliqué
    /// ligne à ligne : relire et reconstruire 40 000 lignes figeait la fenêtre
    /// à chaque message arrivé. La liste entière ne revient qu'à l'ouverture
    /// d'un dossier, après de très nombreux changements, ou si `complet` —
    /// quand des lignes retirées d'avance doivent réapparaître.
    function rafraichirListe(complet) {
        // En recherche, la liste est celle des résultats : on la recherche à
        // nouveau, et l'index tient compte des changements.
        if (recherche.length > 0) {
            poserResultats()
            return
        }
        if (vueUnifiee) {
            poserBoitesUnifiees()
            return
        }
        var c = JSON.parse(boite.changementsListe(complet === true))
        if (c.complet !== undefined) {
            poserListe(c.complet, c.suite === true)
            return
        }
        var absents = {}
        for (var i = 0; i < c.retires.length; ++i) {
            var r = c.retires[i]
            var k = rangExact(r.h, r.uid)
            if (k >= 0)
                modeleMessages.remove(k)
            absents[r.uid] = true
        }
        // Drapeau de suivi compris : posé sur le téléphone ou dans le
        // webmail, il arrive ainsi sans changer de dossier.
        for (var j = 0; j < c.modifies.length; ++j) {
            var m = c.modifies[j]
            var km = rangExact(m.h, m.uid)
            if (km >= 0)
                modeleMessages.set(km, m)
        }
        for (var a = 0; a < c.ajoutes.length; ++a) {
            var n = c.ajoutes[a]
            delete absents[n.uid]
            if (fenetre.enDeplacement[cleDeplacement(n.uid)] === true || rangExact(n.h, n.uid) >= 0)
                continue
            modeleMessages.insert(rangInsertion(n.h, n.uid), n)
        }
        oublierAbsents(absents)
        majInfoDossier()
    }

    /// Pose une liste entière, en gardant la position et ce qui est choisi.
    /// `suite` : ce n'est que la première page d'un gros dossier, le reste
    /// arrive par paquets (cf. `chargerSuite`).
    function poserListe(liste, suite) {
        arreterSuite()
        var messages = liste.filter(function(m) {
            return fenetre.enDeplacement[cleDeplacement(m.uid)] !== true
        })
        var position = vueMessages.contentY
        var rangHaut = Math.max(0, vueMessages.indexAt(1, position + 1))
        modeleMessages.clear()
        modeleMessages.append(messages)
        var presents = {}
        for (var k = 0; k < messages.length; ++k)
            presents[messages[k].uid] = true
        if (suite === true) {
            minuteurSuite.start()
            // Ce qui était à l'écran et ce qui est choisi doivent y être tout
            // de suite : la suite se lit d'un trait jusqu'à les avoir.
            var attendus = {}
            for (var choisi in selection)
                attendus[choisi] = true
            if (uidCourant > 0)
                attendus[uidCourant] = true
            for (var p in presents)
                delete attendus[p]
            while (minuteurSuite.running
                   && (modeleMessages.count <= rangHaut + 80 || Object.keys(attendus).length > 0)) {
                var page = chargerSuite()
                for (var q = 0; q < page.length; ++q) {
                    presents[page[q].uid] = true
                    delete attendus[page[q].uid]
                }
            }
        }
        vueMessages.contentY = position
        // La sélection ne garde que les messages encore là — ceux qui manquent
        // alors qu'une suite reste à lire n'y sont plus non plus : elle a été
        // lue jusqu'à les trouver.
        var absents = {}
        for (var uid in selection)
            if (!presents[uid])
                absents[uid] = true
        if (uidCourant > 0 && !presents[uidCourant])
            absents[uidCourant] = true
        oublierAbsents(absents)
        majInfoDossier()
    }

    // Pagination (0.5.9) : ouvrir un dossier de 40 000 messages figeait la
    // fenêtre 1,5 s — l'essentiel à analyser le JSON de la liste entière. La
    // première page s'affiche aussitôt ; le reste suit par paquets, entre deux
    // images, relus dans l'index au moment où ils arrivent.
    // Un paquet coûte ~20 ms (index, JSON, modèle) : la fenêtre reste fluide.
    readonly property int taillePaquet: 500
    // Scénario « mesure-liste » : nombre et durée des paquets.
    property var mesureSuite: null
    Timer {
        id: minuteurSuite
        interval: 1
        repeat: true
        onTriggered: fenetre.chargerSuite()
    }

    /// Paquet suivant de la liste, ajouté à la fin ; rend ses lignes.
    function chargerSuite() {
        var debut = Date.now()
        var r = JSON.parse(boite.suiteListe(taillePaquet))
        if (!r.suite)
            minuteurSuite.stop()
        var lignes = r.lignes.filter(function(m) {
            return fenetre.enDeplacement[cleDeplacement(m.uid)] !== true
        })
        if (lignes.length > 0)
            modeleMessages.append(lignes)
        if (mesureSuite) {
            mesureSuite.paquets += 1
            mesureSuite.plusLong = Math.max(mesureSuite.plusLong, Date.now() - debut)
        }
        return lignes
    }

    function arreterSuite() {
        minuteurSuite.stop()
    }

    /// La liste entière, tout de suite : avant ce qui porte sur toutes ses
    /// lignes (tout sélectionner).
    function completerListe() {
        while (minuteurSuite.running)
            chargerSuite()
    }

    /// Retire de la sélection et de l'affichage les messages disparus.
    function oublierAbsents(absents) {
        var reste = {}
        var change = false
        for (var uid in selection) {
            if (absents[uid])
                change = true
            else
                reste[uid] = true
        }
        if (change)
            selection = reste
        if (uidCourant > 0 && absents[uidCourant]) {
            uidCourant = 0
            sujetAffiche.text = ""
            auteurAffiche.text = ""
            afficherTexte(qsTr("Aucun message sélectionné."))
        }
    }

    // ---- recherche

    /// Lance la recherche saisie, ou `texteImpose` (une recherche passée, choisie
    /// dans la liste). Dans le dossier ouvert comme dans toutes les boîtes, la
    /// liste ne contient plus que les résultats.
    function lancerRecherche(texteImpose) {
        var texte = (texteImpose !== undefined ? texteImpose : champRecherche.text).trim()
        panneauHistorique.close()
        if (texte.length === 0) {
            quitterRecherche()
            return
        }
        champRecherche.text = texte
        noterRecherche(texte)
        if (vueUnifiee) {
            vueUnifiee = false
            unifieeAvantRecherche = true
            cleChoisie = ""
            viderListe()
        }
        if (porteeRecherche.partout) {
            if (!rechercheGlobale)
                dossierAvantRecherche = boite.dossierCourant.length > 0
                        ? { compte: boite.compteCourant, chemin: boite.dossierCourant } : null
            rechercheGlobale = true
            recherche = texte
            uidsServeur = []
            cleChoisie = ""
            viderListe()
            poserResultats()
            messageEtat.texte = qsTr("Recherche « %1 » dans toutes les boîtes : %2.").arg(texte)
                    .arg(accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats")))
            return
        }
        // De toutes les boîtes au dossier actif : celui d'où l'on était parti.
        if (rechercheGlobale) {
            var avant = dossierAvantRecherche
            sortirRechercheGlobale()
            if (avant)
                boite.ouvrirDossier(avant.compte, avant.chemin)
            champRecherche.text = texte
        }
        if (boite.dossierCourant.length === 0)
            return
        recherche = texte
        uidsServeur = []
        poserResultats()
        if (!boite.chercherSurLeServeur(texte))
            messageEtat.texte = qsTr("Recherche dans l'index seulement (compte hors ligne).")
    }

    /// Résultats de l'index — et, dans le dossier ouvert, ceux que le serveur a
    /// trouvés dans le texte des messages —, du plus récent au plus ancien.
    function poserResultats() {
        if (rechercheGlobale) {
            poserListe(trierResultats(JSON.parse(boite.chercher(recherche, true)).map(function(r) {
                // Une liste JS deviendrait un sous-modèle dans la ListModel.
                r.cheminAffiche = cheminResultat(r)
                delete r.segments
                return r
            })))
            return
        }
        var lignes = JSON.parse(boite.chercher(recherche, false))
        var vus = {}
        for (var i = 0; i < lignes.length; ++i)
            vus[lignes[i].uid] = true
        var manquants = uidsServeur.filter(function(u) { return !vus[u] })
        if (manquants.length > 0)
            lignes = lignes.concat(JSON.parse(boite.messagesParUid(manquants.join(","))))
        poserListe(trierResultats(lignes))
    }

    /// Résultats rangés selon `triRecherche` ; à égalité, les plus récents
    /// d'abord. L'objet se compare sans ses « RE : », « TR : » : une
    /// conversation reste groupée.
    function trierResultats(lignes) {
        var recents = function(a, b) { return b.h - a.h || b.uid - a.uid }
        var parTexte = function(valeur) {
            return function(a, b) { return valeur(a).localeCompare(valeur(b)) || recents(a, b) }
        }
        var comparer = {
            anciens: function(a, b) { return a.h - b.h || a.uid - b.uid },
            expediteur: parTexte(function(m) { return (m.expediteur || m.adresse || "").toLowerCase() }),
            objet: parTexte(function(m) {
                return (m.sujet || "").replace(/^\s*((re|tr|fw|fwd|aw|wg)\s*(\[\d+\])?\s*:\s*)+/i, "")
                        .toLowerCase()
            }),
            taille: function(a, b) { return b.taille - a.taille || recents(a, b) },
            dossier: parTexte(function(m) { return (m.cheminAffiche || "").toLowerCase() })
        }[triRecherche] || recents
        return lignes.slice().sort(comparer)
    }

    function trierRecherche(cle) {
        triRecherche = cle
        if (recherche.length === 0)
            return
        poserResultats()
        vueMessages.positionViewAtBeginning()
    }

    function sortirRechercheGlobale() {
        rechercheGlobale = false
        cleChoisie = ""
        dossierAvantRecherche = null
        recherche = ""
        uidsServeur = []
        viderListe()
    }

    /// Sort de la recherche : la liste du dossier revient entière — celui d'où
    /// l'on était parti, après une recherche dans toutes les boîtes.
    function quitterRecherche() {
        champRecherche.text = ""
        panneauHistorique.close()
        if (recherche.length === 0)
            return
        if (unifieeAvantRecherche) {
            sortirRechercheGlobale()
            ouvrirBoitesUnifiees()
            return
        }
        if (rechercheGlobale) {
            var avant = dossierAvantRecherche
            sortirRechercheGlobale()
            if (avant)
                boite.ouvrirDossier(avant.compte, avant.chemin)
            // Le noyau a suivi les résultats choisis : la liste est relue en
            // entier, jamais par différence.
            rafraichirListe(true)
            return
        }
        recherche = ""
        uidsServeur = []
        rafraichirListe(true)
    }

    /// Chemin lisible du dossier d'un résultat : compte › dossiers parents ›
    /// dossier, nommés comme dans l'arborescence.
    function cheminResultat(r) {
        var c = compteParId(r.compte)
        var noms = (r.segments && r.segments.length > 0 ? r.segments : [r.dossier]).slice()
        var dernier = noms.length - 1
        if (dernier > 0 && noms[0].toUpperCase() === "INBOX")
            noms[0] = qsTr("Boîte de réception")
        noms[dernier] = libelleLigne({ genre: "dossier", nom: noms[dernier], role: r.role || "", chemin: r.chemin })
        return (c ? c.adresse + " › " : "") + noms.join(" › ")
    }

    /// Boîte de réception unifiée : les réceptions de tous les comptes.
    function ouvrirBoitesUnifiees() {
        if (rechercheGlobale)
            sortirRechercheGlobale()
        recherche = ""
        uidsServeur = []
        champRecherche.text = ""
        unifieeAvantRecherche = false
        cleChoisie = ""
        vueUnifiee = true
        viderListe()
        poserBoitesUnifiees()
        if (compact)
            vue = 1
    }

    function poserBoitesUnifiees() {
        poserListe(JSON.parse(boite.boitesDeReception()).map(function(r) {
            r.cheminAffiche = cheminResultat(r)
            delete r.segments
            return r
        }))
    }

    function cleResultat(compte, chemin, uid) {
        return compte + "\u0001" + chemin + "\u0001" + uid
    }

    // ---- historique des recherches

    function historiqueRecherches() {
        try {
            var h = JSON.parse(reglages.historiqueRecherches)
            return Array.isArray(h) ? h : []
        } catch (e) {
            return []
        }
    }

    /// La plus récente en tête, sans doublon (casse ignorée), 50 au plus.
    function noterRecherche(texte) {
        var h = historiqueRecherches().filter(function(t) { return t.toLowerCase() !== texte.toLowerCase() })
        h.unshift(texte)
        reglages.historiqueRecherches = JSON.stringify(h.slice(0, 50))
    }

    /// Propose, sous le champ, les recherches passées qui contiennent ce qui
    /// est tapé (toutes si rien ne l'est).
    function proposerHistorique() {
        var filtre = champRecherche.text.trim().toLowerCase()
        var h = historiqueRecherches().filter(function(t) {
            return filtre.length === 0 || t.toLowerCase().indexOf(filtre) >= 0
        })
        panneauHistorique.entrees = h
        if (h.length > 0 && champRecherche.activeFocus)
            panneauHistorique.open()
        else
            panneauHistorique.close()
    }

    /// Premier rang dont la ligne ne précède pas (h, uid) : la liste est triée
    /// du plus récent au plus ancien, puis par UID décroissant, comme l'index.
    function rangInsertion(h, uid) {
        var bas = 0
        var haut = modeleMessages.count
        while (bas < haut) {
            var milieu = (bas + haut) >> 1
            var l = modeleMessages.get(milieu)
            if (l.h > h || (l.h === h && l.uid > uid))
                bas = milieu + 1
            else
                haut = milieu
        }
        return bas
    }

    /// Rang d'un message de la liste par sa date et son UID, -1 s'il n'y est pas.
    function rangExact(h, uid) {
        var i = rangInsertion(h, uid)
        return i < modeleMessages.count && modeleMessages.get(i).uid === uid ? i : -1
    }

    function indexDe(uid) {
        for (var i = 0; i < modeleMessages.count; ++i)
            if (modeleMessages.get(i).uid === uid)
                return i
        return -1
    }

    /// Clic sur la ligne `index` : seul (et affiché), ajouté ou retiré (Ctrl),
    /// ou étendu depuis l'ancre (Maj).
    function choisir(index, modificateurs) {
        if (index < 0 || index >= modeleMessages.count)
            return
        if (listeMulti) {
            choisirResultat(index)
            return
        }
        var uid = modeleMessages.get(index).uid
        var nouvelle = {}
        if (modificateurs & Qt.ShiftModifier && ancre >= 0) {
            var debut = Math.min(ancre, index), fin = Math.max(ancre, index)
            for (var i = debut; i <= fin; ++i)
                nouvelle[modeleMessages.get(i).uid] = true
        } else if (modificateurs & Qt.ControlModifier) {
            for (var u in selection)
                nouvelle[u] = true
            if (nouvelle[uid])
                delete nouvelle[uid]
            else
                nouvelle[uid] = true
            ancre = index
        } else {
            nouvelle[uid] = true
            ancre = index
        }
        selection = nouvelle
        vueMessages.currentIndex = index
        vueMessages.forceActiveFocus()
        var nombre = Object.keys(nouvelle).length
        if (nombre === 1 && nouvelle[uid])
            afficherMessage(uid, index)
        else if (nombre > 1)
            messageEtat.texte = accord(nombre, qsTr("message sélectionné."), qsTr("messages sélectionnés."))
    }

    /// Recherche dans toutes les boîtes : un seul résultat à la fois ; le noyau
    /// passe au dossier du message, sans que la liste des résultats ne change.
    function choisirResultat(index) {
        var r = modeleMessages.get(index)
        cleChoisie = cleResultat(r.compte, r.chemin, r.uid)
        var s = {}
        s[r.uid] = true
        selection = s
        ancre = index
        vueMessages.currentIndex = index
        vueMessages.forceActiveFocus()
        if (r.compte !== boite.compteCourant || r.chemin !== boite.dossierCourant)
            boite.ouvrirDossier(r.compte, r.chemin)
        afficherMessage(r.uid, index)
    }

    function deplacerCurseur(pas, etendre) {
        var index = Math.max(0, Math.min(modeleMessages.count - 1, vueMessages.currentIndex + pas))
        choisir(index, etendre ? Qt.ShiftModifier : 0)
        vueMessages.positionViewAtIndex(index, ListView.Contain)
    }

    function toutChoisir() {
        if (listeMulti)
            return
        completerListe()
        var nouvelle = {}
        for (var i = 0; i < modeleMessages.count; ++i)
            nouvelle[modeleMessages.get(i).uid] = true
        selection = nouvelle
    }

    // ---- rédaction

    /// Nom affiché et signature d'un compte, avec leurs valeurs par défaut.
    function identite(adresse) {
        var table = {}
        try { table = JSON.parse(reglagesRedaction.identites) } catch (e) { table = {} }
        var i = table[adresse] || {}
        return { nom: i.nom || "", signature: i.signature || "", html: i.html || "",
                 nouveaux: i.nouveaux !== false, reponses: i.reponses !== false }
    }

    function poserIdentite(adresse, valeur) {
        var table = {}
        try { table = JSON.parse(reglagesRedaction.identites) } catch (e) { table = {} }
        table[adresse] = valeur
        reglagesRedaction.identites = JSON.stringify(table)
    }

    /// Ouvre une rédaction : une fenêtre à part sur le bureau, comme Outlook ;
    /// en vue compacte (téléphone), un volet qui couvre la fenêtre principale.
    function ouvrirRedaction(compte) {
        compteurRedactions += 1
        var jeton = "r" + compteurRedactions
        var conteneur = compact ? composantVoletRedaction.createObject(fenetre)
                                : composantFenetreRedaction.createObject(fenetre)
        if (compact)
            conteneur.open()
        var r = conteneur.redaction
        r.jeton = jeton
        r.idGarde = Date.now().toString(36) + "-" + Math.floor(Math.random() * 1e9).toString(36)
        redactionActive = jeton
        r.choisirCompte(compte)
        var table = redactions
        table[jeton] = r
        redactions = table
        return r
    }

    /// Rédactions qu'une session précédente n'a pas menées à leur terme
    /// (MMail fermé avant l'envoi ou l'enregistrement) : proposées à la reprise.
    function proposerReprise() {
        var liste = []
        try { liste = JSON.parse(boite.redactionsGardees()) } catch (e) { liste = [] }
        if (liste.length === 0)
            return
        redactionsRetrouvees = liste
        dlgReprise.open()
    }

    function rouvrirRedactions() {
        var liste = redactionsRetrouvees
        redactionsRetrouvees = []
        for (var k = 0; k < liste.length; ++k) {
            var g = liste[k]
            var r = ouvrirRedaction(g.compte || (comptesConnus.length > 0 ? comptesConnus[0].compte : 0))
            r.idGarde = g.id
            var p = g.contenu || {}
            p.mode = "brouillon"
            r.remplir(p)
            // Rien de ce qui est repris n'est encore sur le serveur.
            r.modifie = true
            var manquantes = (p.pieces || []).length - r.pieces.length
            if (manquantes > 0)
                r.signaler(qsTr("Rédaction reprise ; %1 introuvable sur le poste.")
                           .arg(accord(manquantes, qsTr("pièce jointe"), qsTr("pièces jointes"))), true)
            else
                r.signaler(qsTr("Rédaction reprise, telle qu'au %1.").arg(dateLongue(g.modifie * 1000)), false)
        }
    }

    function oublierRedaction(jeton) {
        if (jeton && redactions[jeton]) {
            var table = redactions
            delete table[jeton]
            redactions = table
        }
    }

    /// `mode` : « nouveau », « repondre », « repondre_tous », « transferer »
    /// ou « brouillon » (reprise d'un brouillon du dossier ouvert).
    function rediger(mode) {
        var compte = boite.compteCourant > 0 ? boite.compteCourant
                   : (comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        if (mode === "nouveau") {
            ouvrirRedaction(compte).pret()
            return
        }
        var uids = uidsChoisis()
        var uid = uids.length === 1 ? uids[0] : uidCourant
        if (!uid) {
            messageEtat.texte = qsTr("Choisissez d'abord un message.")
            return
        }
        var r = ouvrirRedaction(compte)
        r.attendre(mode === "brouillon" ? qsTr("Reprise du brouillon…") : qsTr("Préparation…"))
        if (!boite.preparer(uid, mode, r.jeton))
            r.echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Préparation impossible."))
    }

    function uidsChoisis() {
        return Object.keys(selection).map(function(u) { return parseInt(u) })
    }

    function afficherMessage(uid, rang) {
        uidCourant = uid
        sourceVisible = false
        pieces = []
        var i = rang !== undefined && rang >= 0 && rang < modeleMessages.count
                && modeleMessages.get(rang).uid === uid ? rang : indexDe(uid)
        indexCourant = i
        if (i >= 0) {
            var ligne = modeleMessages.get(i)
            sujetAffiche.text = ligne.sujet
            importanceCourante = ligne.importance
            auteurAffiche.text = ligne.expediteur
                    + (ligne.adresse.length > 0 && ligne.adresse !== ligne.expediteur
                       ? " <" + ligne.adresse + ">" : "")
                    + "  ·  " + dateLongue(ligne.date)
        }
        confirmationDemandee = ""
        afficherTexte(qsTr("Chargement…"))
        // Le contenu après un court délai : en parcourant la liste au clavier,
        // seul le message où l'on s'arrête est chargé (et marqué lu).
        minuterieCorps.restart()
        if (compact)
            vue = 2
    }

    Timer {
        id: minuterieCorps
        interval: 60
        onTriggered: if (fenetre.uidCourant > 0) boite.demanderCorps(fenetre.uidCourant)
    }

    /// Rang dans la liste d'un message du dossier ouvert, le message affiché
    /// d'abord.
    function rangDe(uid) {
        if (indexCourant >= 0 && indexCourant < modeleMessages.count
                && modeleMessages.get(indexCourant).uid === uid)
            return indexCourant
        return indexDe(uid)
    }

    function sujetCourant() {
        return sujetAffiche.text
    }

    /// Texte simple dans le volet du message : attente, absence, erreur.
    function afficherTexte(texte) {
        corpsHtml = false
        htmlAffiche = ""
        imagesBloquees = 0
        lienSurvole = ""
        vueCorps.text = texte
    }

    /// Le HTML du message, tailles de police mises à l'échelle du zoom de la
    /// colonne : celles que l'expéditeur a écrites ne suivraient pas, sinon,
    /// la taille du texte.
    function htmlAuZoom(html) {
        var z = reglages.zoomMessage
        if (Math.abs(z - 1) < 0.01)
            return html
        return html.replace(/((?:font-size|line-height)\s*:\s*)([0-9]*\.?[0-9]+)(pt|px)/gi,
                            function(tout, avant, valeur, unite) {
                                return avant + (Math.round(parseFloat(valeur) * z * 10) / 10) + unite
                            })
    }

    /// Réapplique le zoom à un message HTML affiché, sans perdre la position.
    function reposerHtml() {
        if (!corpsHtml || sourceVisible)
            return
        var position = cadreCorps.ScrollBar.vertical.position
        vueCorps.text = htmlAuZoom(htmlAffiche)
        bornerImages()
        cadreCorps.ScrollBar.vertical.position = position
    }

    /// Les images du message HTML affiché, bornées à la largeur de la colonne.
    function bornerImages() {
        if (corpsHtml && !sourceVisible)
            mefLecture.bornerImages(Math.floor(vueCorps.width - vueCorps.leftPadding - vueCorps.rightPadding) - 2)
    }
    // Un cran de molette remettait en page tout le message : les crans
    // rapprochés n'en font qu'une.
    onTailleMessageChanged: minuterieZoomMessage.restart()
    Timer {
        id: minuterieZoomMessage
        interval: 150
        onTriggered: fenetre.reposerHtml()
    }

    /// Un lien du message : une adresse de courriel ouvre une rédaction dans
    /// MMail, une adresse web part au navigateur.
    function ouvrirLien(lien) {
        if (lien.toLowerCase().indexOf("mailto:") !== 0) {
            Qt.openUrlExternally(lien)
            return
        }
        var reste = lien.substring(7)
        var q = reste.indexOf("?")
        var adresse = q >= 0 ? reste.substring(0, q) : reste
        var objet = ""
        try {
            adresse = decodeURIComponent(adresse)
            var m = q >= 0 ? /(?:^|&)subject=([^&]*)/i.exec(reste.substring(q + 1)) : null
            if (m)
                objet = decodeURIComponent(m[1].replace(/\+/g, " "))
        } catch (e) {
        }
        var compte = boite.compteCourant > 0 ? boite.compteCourant
                   : (comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        ouvrirRedaction(compte).remplir({ a: adresse, objet: objet, mode: "nouveau" })
    }

    /// Signale la sélection comme indésirable — ou, depuis le dossier des
    /// indésirables, comme légitime. Le filtre du serveur l'apprend au
    /// déplacement (Mailcow : Rspamd, par IMAPSieve).
    function signalerIndesirable() {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        var retour = roleCourant === "Junk"
        if (!boite.signalerIndesirable(uids.join(",")))
            return
        retirerDeLaListe(uids)
        var n = accord(uids.length, qsTr("message"), qsTr("messages"))
        messageEtat.texte = retour
                ? (uids.length > 1 ? qsTr("%1 remis en boîte de réception : le filtre du serveur apprend qu'ils sont légitimes.")
                                   : qsTr("%1 remis en boîte de réception : le filtre du serveur apprend qu'il est légitime.")).arg(n)
                : (uids.length > 1 ? qsTr("%1 classés indésirables : le filtre du serveur l'apprend.")
                                   : qsTr("%1 classé indésirable : le filtre du serveur l'apprend.")).arg(n)
    }

    /// Compte qui suit `compte` dans l'arborescence, ou 0 s'il est le dernier.
    function compteSuivant(compte) {
        var vu = false
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre !== "compte")
                continue
            if (vu)
                return l.compte
            vu = l.compte === compte
        }
        return 0
    }

    function basculerSource() {
        if (uidCourant <= 0) {
            var uids = uidsChoisis()
            if (uids.length !== 1)
                return
            uidCourant = uids[0]
        }
        sourceVisible = !sourceVisible
        afficherTexte(qsTr("Chargement…"))
        if (sourceVisible)
            boite.demanderSource(uidCourant)
        else
            boite.demanderCorps(uidCourant)
    }

    function marquerSelection(lu) {
        var uids = uidsChoisis()
        if (uids.length > 0 && boite.marquerLu(uids.join(","), lu))
            rafraichirListe()
    }

    function ouvrirDeplacer() {
        if (uidsChoisis().length === 0)
            return
        dlgDeplacer.cibles = JSON.parse(boite.dossiersCibles())
        dlgDeplacer.zoom = zoomDe(colonneActive)
        dlgDeplacer.open()
    }

    /// Déplace la sélection vers un dossier de n'importe quel compte.
    function deplacerVers(compte, chemin) {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        if (!boite.deplacer(uids.join(","), compte, chemin))
            return
        retirerDeLaListe(uids)
        messageEtat.texte = qsTr("Déplacement de %1…").arg(accord(uids.length, qsTr("message"), qsTr("messages")))
    }

    function supprimerSelection() {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        if (boite.supprimer(uids.join(",")))
            retirerDeLaListe(uids)
    }

    /// Clé d'un message en cours de déplacement : un UID ne désigne un message
    /// que dans son dossier, et le même numéro existe ailleurs.
    function cleDeplacement(uid) {
        return boite.compteCourant + "|" + boite.dossierCourant + "|" + uid
    }

    function retirerDeLaListe(uids) {
        var retires = {}
        for (var u in enDeplacement)
            retires[u] = true
        for (var i = 0; i < uids.length; ++i)
            retires[cleDeplacement(uids[i])] = true
        enDeplacement = retires
        deplacementsEnVol += 1
        var suivant = -1
        for (var k = modeleMessages.count - 1; k >= 0; --k) {
            if (retires[cleDeplacement(modeleMessages.get(k).uid)]) {
                modeleMessages.remove(k)
                suivant = k
            }
        }
        selection = ({})
        uidCourant = 0
        sujetAffiche.text = ""
        auteurAffiche.text = ""
        afficherTexte(qsTr("Aucun message sélectionné."))
        // Comme Outlook : le message suivant prend la place.
        if (suivant >= 0 && modeleMessages.count > 0 && !compact)
            choisir(Math.min(suivant, modeleMessages.count - 1), 0)
        else if (compact)
            vue = 1
    }

    function choisirApparence(nom) {
        apparence = nom
        messageEtat.texte = qsTr("Apparence : %1.").arg(jeuxApparence[nom].libelle)
    }

    /// Écrit une pièce jointe du message affiché pour la joindre à une
    /// rédaction : `cible` est son jeton, ou « nouveau ».
    function joindrePiece(piece, cible) {
        if (!piece || uidCourant <= 0)
            return
        pieceCible = cible
        if (!boite.joindrePiece(uidCourant, piece.indice))
            messageEtat.texte = boite.erreur
    }

    /// `programme` : « Ouvrir avec… » pour cette fois ; sinon le logiciel
    /// retenu pour l'extension, à défaut celui du système (cf. onPieceEcrite).
    function ouvrirPiece(piece, programme) {
        if (!piece || piece.risquee)
            return
        ouvertureAvec = programme ? { extension: extensionDe(piece.nom), programme: programme } : null
        if (boite.ouvrirPiece(uidCourant, piece.indice))
            messageEtat.texte = qsTr("Préparation de %1…").arg(piece.nom)
    }

    function extensionDe(nom) {
        var i = nom.lastIndexOf(".")
        return i > 0 ? nom.substring(i + 1).toLowerCase() : ""
    }

    /// Étiquette d'une pièce jointe : l'extension, en couleur selon la famille
    /// (rouge PDF, bleu traitement de texte, vert tableur, violet image).
    function genrePiece(nom) {
        var ext = extensionDe(nom)
        var libelle = ext.length > 0 ? ext.toUpperCase().substring(0, 4) : "?"
        if (ext === "pdf")
            return { libelle: libelle, couleur: "#c42b1c" }
        if (["doc", "docx", "odt", "rtf", "txt"].indexOf(ext) >= 0)
            return { libelle: libelle, couleur: "#1a4480" }
        if (["xls", "xlsx", "xlsm", "ods", "csv"].indexOf(ext) >= 0)
            return { libelle: libelle, couleur: "#217346" }
        if (["ppt", "pptx", "odp"].indexOf(ext) >= 0)
            return { libelle: libelle, couleur: "#b7472a" }
        if (["jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "tif", "tiff"].indexOf(ext) >= 0)
            return { libelle: libelle, couleur: "#6b3fa0" }
        return { libelle: libelle, couleur: "#5f6368" }
    }

    function logicielsPieces() {
        try {
            var m = JSON.parse(reglagesPieces.logiciels)
            return m && typeof m === "object" && !Array.isArray(m) ? m : {}
        } catch (e) {
            return {}
        }
    }

    function logicielPour(nom) {
        var ext = extensionDe(nom)
        return ext.length > 0 ? (logicielsPieces()[ext] || "") : ""
    }

    /// Retient `programme` pour l'extension ; vide : la rend au système.
    function retenirLogiciel(extension, programme) {
        var m = logicielsPieces()
        if (programme.length > 0)
            m[extension] = programme
        else
            delete m[extension]
        reglagesPieces.logiciels = JSON.stringify(m)
    }

    function enregistrerPiece(piece) {
        if (!piece)
            return
        dlgEnregistrerPiece.piece = piece
        dlgEnregistrerPiece.uid = uidCourant
        dlgEnregistrerPiece.currentFolder = dossierTelechargements
        dlgEnregistrerPiece.selectedFile = dossierTelechargements + "/" + encodeURIComponent(piece.nom)
        dlgEnregistrerPiece.open()
    }

    // 1536 → « 1,5 Ko ».
    function tailleLisible(octets) {
        if (octets < 1024)
            return qsTr("%1 o").arg(octets)
        if (octets < 1024 * 1024)
            return qsTr("%1 Ko").arg((octets / 1024).toLocaleString(Qt.locale("fr_FR"), "f", 1))
        return qsTr("%1 Mo").arg((octets / 1024 / 1024).toLocaleString(Qt.locale("fr_FR"), "f", 1))
    }

    // « 2026-09-16T18:00:00+02:00 » → « 18:00 » aujourd'hui, « 16/09 18:00 »
    // cette année, « 16/09/2025 » avant. Les dates que le serveur n'a pas su
    // rendre en ISO sont affichées telles quelles.
    function dateCourte(date) {
        var d = new Date(date)
        if (isNaN(d.getTime()))
            return date
        function deux(n) { return (n < 10 ? "0" : "") + n }
        var maintenant = new Date()
        var heure = deux(d.getHours()) + ":" + deux(d.getMinutes())
        if (d.toDateString() === maintenant.toDateString())
            return heure
        var jour = deux(d.getDate()) + "/" + deux(d.getMonth() + 1)
        if (d.getFullYear() === maintenant.getFullYear())
            return jour + " " + heure
        return jour + "/" + d.getFullYear()
    }

    // « mer. 16/09/2026 14:54 », indépendamment de la langue du système.
    function dateLongue(date) {
        var d = new Date(date)
        if (isNaN(d.getTime()))
            return date
        function deux(n) { return (n < 10 ? "0" : "") + n }
        var jours = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."]
        return jours[d.getDay()] + " " + deux(d.getDate()) + "/" + deux(d.getMonth() + 1) + "/"
                + d.getFullYear() + " " + deux(d.getHours()) + ":" + deux(d.getMinutes())
    }

    // Fermeture de l'application : chaque rédaction ouverte est gardée sur le
    // poste, dans son dernier état.
    Connections {
        target: Qt.application
        function onAboutToQuit() {
            for (var jeton in fenetre.redactions)
                fenetre.redactions[jeton].garder()
        }
    }

    onClosing: function(fermeture) {
        // Sur téléphone, le retour arrière ramène de l'agenda au courrier, puis
        // remonte d'un volet avant de quitter.
        if (compact && module === "agenda") {
            fermeture.accepted = false
            module = "courrier"
        } else if (compact && vue > 0) {
            fermeture.accepted = false
            vue = vue - 1
        }
    }

    Component.onCompleted: {
        if (typeof modeControle !== "undefined" && modeControle)
            return
        if (!boite.ouvrirProfil(cheminProfil))
            return
        rafraichirArborescence()
        Qt.callLater(proposerReprise)
        // Session ouverte d'emblée quand l'environnement porte des identifiants
        // (cf. cpp/main.cpp) : sert aux captures et aux essais, jamais en usage
        // courant. Le coffre n'est alors pas touché.
        if (typeof identifiantsEssai !== "undefined" && identifiantsEssai
                && identifiantsEssai.hote) {
            essai = { afficherPremier: !identifiantsEssai.scenario
                                       || identifiantsEssai.scenario === "pieces"
                                       || identifiantsEssai.scenario === "ouvrir-avec"
                                       || identifiantsEssai.scenario === "images"
                                       || identifiantsEssai.scenario === "invitation"
                                       || identifiantsEssai.scenario.indexOf("signature") === 0,
                      connecter2: null, scenario: identifiantsEssai.scenario || "",
                      etape: 0, sujet: "", examines: 0 }
            // Scénarios « garde », « reprise », « reprise-ouvrir » : une
            // rédaction gardée sur le poste par une session est proposée par la
            // suivante, se rouvre telle quelle, et disparaît à son issue.
            if (essai.scenario === "garde" || essai.scenario === "reprise-ouvrir") {
                var pause = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                pause.triggered.connect(essai.scenario === "garde" ? etapeScenarioGarde : etapeScenarioReprise)
                pause.start()
                return
            }
            if (essai.scenario === "reprise")
                return
            // Scénario « restauration » : un profil neuf, dont le compte
            // retrouve la sauvegarde du scénario « sauvegarde ».
            if (essai.scenario === "restauration")
                reglagesSauvegarde.aChercher = true
            if (essai.scenario === "garde-serveur") {
                var delai = Qt.createQmlObject('import QtQuick; Timer { interval: 6000 }', fenetre)
                delai.triggered.connect(etapeScenarioGardeServeur)
                delai.start()
            }
            // Scénario « veille-fond » : le second compte n'est jamais ouvert ;
            // ses non-lus en boîte de réception sont relevés chaque demi-
            // seconde — un message qu'on y dépose doit paraître en quelques
            // secondes (IDLE), non à la veille de deux minutes.
            if (essai.scenario === "veille-fond") {
                var suivi = Qt.createQmlObject('import QtQuick; Timer { interval: 500; repeat: true }', fenetre)
                var dernier = -1
                suivi.triggered.connect(function() {
                    var c = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur2 })[0]
                    for (var i = 0; c && i < modeleArborescence.count; ++i) {
                        var l = modeleArborescence.get(i)
                        if (l.genre === "dossier" && l.compte === c.compte && l.chemin === "INBOX" && l.nonLus !== dernier) {
                            dernier = l.nonLus
                            console.log("scenario: non lus en réception du second compte :", l.nonLus, "à", new Date().toISOString())
                        }
                    }
                })
                suivi.start()
            }
            // Scénario « unifiee » : deux comptes ; la boîte de réception
            // unifiée les réunit, un message du second compte s'y choisit — le
            // noyau passe sur sa réception —, un dossier de l'arborescence en
            // fait sortir.
            if (essai.scenario === "unifiee") {
                var attenteUnifiee = Qt.createQmlObject('import QtQuick; Timer { interval: 12000 }', fenetre)
                attenteUnifiee.triggered.connect(function() {
                    ouvrirBoitesUnifiees()
                    var comptes = {}, autre = -1
                    var c2 = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur2 })[0]
                    for (var i = 0; i < modeleMessages.count; ++i) {
                        var r = modeleMessages.get(i)
                        comptes[r.compte] = (comptes[r.compte] || 0) + 1
                        if (autre < 0 && c2 && r.compte === c2.compte)
                            autre = i
                    }
                    console.log("scenario: unifiée", modeleMessages.count, "messages | par compte", JSON.stringify(comptes),
                                "| barre", infoDossier)
                    console.log("scenario: pastille", nonLusReceptions, "| image",
                                identifiantsEssai.sortie.length > 0
                                && pastille.enregistrer(nonLusReceptions, lanceur.chemin(identifiantsEssai.sortie + "/pastille.png")))
                    if (autre >= 0) {
                        choisir(autre, 0)
                        console.log("scenario: choisi", modeleMessages.get(autre).sujet, "| noyau sur compte",
                                    boite.compteCourant, boite.dossierCourant, "| liste", modeleMessages.count, "| unifiée", vueUnifiee)
                    }
                    ouvrirDossier(comptesConnus[0].compte, "INBOX")
                    console.log("scenario: après l'arborescence, unifiée", vueUnifiee, "| liste", modeleMessages.count)
                    Qt.quit()
                })
                attenteUnifiee.start()
            }
            if (essai.scenario.indexOf("mise-a-jour") === 0) {
                reglagesMiseAJour.version = ""
                reglagesMiseAJour.choix = ""
            }
            // Scénario « envoi-fond » : un message à soi-même, la pièce jointe
            // MMAIL_DOSSIER (un gros fichier) ; pendant l'envoi SMTP, un autre
            // dossier s'ouvre — sans attendre la fin de l'envoi depuis la 0.5.9.
            if (essai.scenario === "envoi-fond") {
                var attenteEnvoi = Qt.createQmlObject('import QtQuick; Timer { interval: 8000 }', fenetre)
                attenteEnvoi.triggered.connect(function() {
                    var c = comptesConnus[0]
                    var n = ouvrirRedaction(c.compte)
                    n.pret()
                    essai.sujet = "Essai envoi en fond " + Date.now()
                    n.remplir({ a: identifiantsEssai.utilisateur, objet: essai.sujet, mode: "nouveau",
                                texte: "Envoyé par le fil SMTP.", pieces: [identifiantsEssai.dossier] })
                    essai.depart = Date.now()
                    n.envoyer(false)
                    console.log("scenario: envoi lancé, pièce", n.pieces.length > 0 ? n.pieces[0].taille : "absente")
                    ouvrirDossier(c.compte, "Archive")
                })
                attenteEnvoi.start()
            }
            if (essai.scenario === "coller-image") {
                var attenteImage = Qt.createQmlObject('import QtQuick; Timer { interval: 6000 }', fenetre)
                attenteImage.triggered.connect(etapeScenarioCollerImage)
                attenteImage.start()
            }
            if (essai.scenario === "brouillon-auto") {
                var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 6000 }', fenetre)
                attente.triggered.connect(function() { etapeScenarioBrouillonAuto(null, 0) })
                attente.start()
            }
            // Scénario « horsligne » : aucune connexion ; le profil d'une
            // session précédente doit suffire à lire la boîte de réception.
            // Scénario « mesure-liste » : aucune connexion ; chronomètre
            // l'ouverture d'INBOX depuis l'index et ses rafraîchissements.
            if (essai.scenario === "mesure-recherche") {
                var mesureRecherche = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                mesureRecherche.triggered.connect(etapeScenarioMesureRecherche)
                mesureRecherche.start()
                return
            }
            if (essai.scenario === "mesure-liste") {
                var mesure = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                mesure.triggered.connect(etapeScenarioMesure)
                mesure.start()
                return
            }
            if (essai.scenario === "horsligne") {
                var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                attente.triggered.connect(etapeScenarioHorsLigne)
                attente.start()
                return
            }
            if (identifiantsEssai.utilisateur2)
                essai.connecter2 = { hote: identifiantsEssai.hote,
                                     adresse: identifiantsEssai.utilisateur2,
                                     motDePasse: identifiantsEssai.motDePasse2 }
            boite.connecter(identifiantsEssai.hote, identifiantsEssai.utilisateur,
                            identifiantsEssai.motDePasse)
            return
        }
        // Comptes connus : l'index local s'affiche tout de suite, chaque
        // connexion suit dès que le coffre a répondu.
        var comptes = listeComptes()
        if (comptes.length === 0) {
            reglagesSauvegarde.aChercher = true
            demanderCompte(null, "")
            return
        }
        for (var i = 0; i < comptes.length; ++i)
            connecterCompte(comptes[i])
    }

    /// Scénario « vidage » : chaque issue, puis l'état des comptes cinq
    /// secondes après la dernière — le 01/10, ils étaient tous passés en
    /// « erreur » après un vidage.
    function bilanScenarioVidage(issue) {
        console.log("scenario: " + issue)
        if (!essai.bilanVidage) {
            essai.bilanVidage = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
            essai.bilanVidage.triggered.connect(function() {
                JSON.parse(boite.arborescence(false))
                    .filter(function(l) { return l.genre === "compte" })
                    .forEach(function(l) {
                        console.log("scenario: compte", l.adresse, "| état :", l.etat, "| motif :", l.motif || "—")
                    })
                Qt.quit()
            })
        }
        essai.bilanVidage.restart()
    }

    /// Scénario « sauvegarde » : des réglages, une signature avec son image
    /// (MMAIL_DOSSIER : adresse d'une image) et un favori, puis la sauvegarde.
    /// Le scénario « restauration », sur un profil neuf, les retrouve.
    function etapeScenarioSauvegarde(compte, adresse) {
        reglages.zoomListe = 1.25
        reglagesApparence.theme = "classique"
        var image = boite.imageSignature(adresse, identifiantsEssai.dossier)
        var identites = {}
        identites[adresse] = { nom: "Essai Sauvegarde", signature: "<p>Cordialement</p><img src=\"" + image + "\">" }
        reglagesRedaction.identites = JSON.stringify(identites)
        boite.epinglerDossier(compte, "INBOX", true)
        console.log("scenario: image de signature", image)
        console.log("scenario: réglages", JSON.stringify(lireReglages()).length, "caractères")
        sauvegarderConfiguration(true)
    }

    /// Scénario « restauration » : ce que la sauvegarde a rapporté.
    function bilanScenarioRestauration() {
        var identites = JSON.parse(reglagesRedaction.identites)
        var adresses = Object.keys(identites)
        var signature = adresses.length > 0 ? identites[adresses[0]].signature : ""
        var image = (signature.match(/src="([^"]+)"/) || [])[1] || ""
        var favoris = 0
        for (var i = 0; i < modeleFavoris.count; ++i)
            if (modeleFavoris.get(i).genre === "favori")
                ++favoris
        console.log("scenario: bilan zoomListe", reglages.zoomListe, "apparence", fenetre.apparence,
                    "identités", adresses.join(","), "favoris", favoris)
        console.log("scenario: image", image)
    }

    function etapeScenarioGarde() {
        var r = ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai de reprise",
                    texte: "Texte tapé avant la coupure.", mode: "brouillon" })
        r.modifie = true
        r.garder()
        console.log("scenario: gardée", r.idGarde, r.etatGarde.length > 0 ? "OK" : "ECHEC")
    }

    /// Scénario « garde-serveur » : l'enregistrement périodique sur le
    /// serveur efface la rédaction gardée sur le poste.
    function etapeScenarioGardeServeur() {
        var r = ouvrirRedaction(comptesConnus[0].compte)
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai d'enregistrement automatique",
                    texte: "Brouillon parti seul.", mode: "brouillon" })
        r.modifie = true
        r.garder()
        console.log("scenario: gardées avant", JSON.parse(boite.redactionsGardees()).length)
        r.enregistrer(true)
    }

    /// Scénario « coller-image » : l'image de MMAIL_DOSSIER (un fichier) passe
    /// par le presse-papier, se colle dans un message neuf, et le brouillon
    /// part sur le serveur — l'image y doit être intégrée.
    // Scénarios « mise-a-jour » et « mise-a-jour-fermeture » : MMail se croit
    // ancien (MMAIL_VERSION_ESSAI), installé par l'installeur ou lancé en
    // AppImage. La dernière publication est proposée, choisie « Maintenant »
    // (ou « À la fermeture »), téléchargée et contrôlée ; puis MMail redémarre
    // (ou se ferme), et le programme l'installe.
    function etapeScenarioMiseAJour(etape) {
        var fermeture = essai.scenario === "mise-a-jour-fermeture"
        console.log("scenario: mise à jour", etape, "| publiée", versionNouvelle, "| actuelle", miseAJour.versionActuelle(),
                    "| genre", genreMiseAJour, "| état", etatMiseAJour, "| fichier", fichierMiseAJour,
                    "| programmée", installeur.programme(), erreurMiseAJour)
        capturerBandeauVersion(etape)
        if (etape === "proposee" && etatMiseAJour === "proposee") {
            var choix = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
            choix.triggered.connect(function() { choisirMiseAJour(fermeture ? "fermeture" : "maintenant") })
            choix.start()
        } else if (etape === "prete") {
            var pause = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
            pause.triggered.connect(function() {
                console.log("scenario: mise à jour —", fermeture ? "fermeture" : "redémarrage", "| pid", Qt.application.arguments)
                if (fermeture)
                    Qt.quit()
                else
                    redemarrerPourMettreAJour()
            })
            pause.start()
        } else if (etape === "echec") {
            Qt.quit()
        }
    }

    function capturerBandeauVersion(nom) {
        if (!identifiantsEssai.sortie)
            return
        var chemin = lanceur.chemin(identifiantsEssai.sortie + "/bandeau-" + nom + ".png")
        bandeauVersion.grabToImage(function(r) { r.saveToFile(chemin) })
    }

    function etapeScenarioCollerImage() {
        var r = ouvrirRedaction(comptesConnus[0].compte)
        r.pret()
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai image collée " + Date.now(),
                    texte: "Avant l'image.", mode: "brouillon" })
        console.log("scenario: image dans le presse-papier", pressePapier.deposerImage(identifiantsEssai.dossier))
        console.log("scenario: collée", r.collerImage(), "| mise en forme", r.miseEnForme,
                    "| image dans le corps", r.htmlActuel().indexOf("<img") >= 0,
                    "| parmi les images collées", r.htmlActuel().indexOf("images-redaction/collees/collee-") >= 0)
        essai.etape = 1
        r.enregistrer(false)
    }

    /// Scénario « brouillon-auto » : un message neuf que l'on ne touche pas ne
    /// passe pas pour modifié ; modifié, il est enregistré sur le serveur sans
    /// rien demander 30 s plus tard, puis une minute après la frappe suivante,
    /// la version précédente effacée (UID rendus par le serveur).
    function etapeScenarioBrouillonAuto(r, uid) {
        var ecoule = function() { return Math.round((Date.now() - essai.depart) / 1000) }
        if (essai.etape === 0) {
            essai.etape = 1
            var n = ouvrirRedaction(comptesConnus[0].compte)
            n.pret()
            var controle = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            controle.triggered.connect(function() {
                console.log("scenario: message neuf, modifié", n.modifie)
                n.remplir({ a: "destinataire@exemple.fr", objet: "Essai brouillon automatique " + Date.now(),
                            texte: "Première frappe.", mode: "brouillon" })
                n.modifie = true
                essai.depart = Date.now()
                var garde = Qt.createQmlObject('import QtQuick; Timer { interval: 150000 }', fenetre)
                garde.triggered.connect(function() {
                    if (essai.etape < 3)
                        console.log("scenario: ECHEC, étape", essai.etape, "| note", n.note)
                })
                garde.start()
            })
            controle.start()
        } else if (essai.etape === 1) {
            essai.etape = 2
            essai.premier = uid
            console.log("scenario: 1er enregistrement après", ecoule(), "s | uid", uid, "| modifiée", r.modifie,
                        "| note", r.note)
            // La frappe reprend un peu plus tard.
            var reprise = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
            reprise.triggered.connect(function() {
                r.modifie = true
                essai.depart = Date.now()
            })
            reprise.start()
        } else if (essai.etape === 2) {
            essai.etape = 3
            console.log("scenario: 2e enregistrement après", ecoule(), "s | uid", uid, "| précédent", essai.premier,
                        "| modifiée", r.modifie)
        }
    }

    /// Scénario « recherche-partout » : « Éléments envoyés » puis la boîte de
    /// réception passent par l'index ; le texte de MMAIL_DOSSIER est cherché
    /// dans toutes les boîtes ; un résultat d'un autre dossier est choisi — le
    /// noyau y passe, la liste reste celle des résultats ; Échap ramène au
    /// dossier de départ. MMAIL_SORTIE_PRECOCE absent : la fenêtre reste sur
    /// les résultats pour la capture.
    function etapeScenarioRecherchePartout(compte, chemin) {
        if (essai.etape === 0) {
            essai.etape = 1
            ouvrirDossier(compte, "Sent")
        } else if (essai.etape === 1 && chemin === "Sent") {
            essai.etape = 2
            ouvrirDossier(compte, "INBOX")
        } else if (essai.etape === 2 && chemin === "INBOX") {
            essai.etape = 3
            porteeRecherche.currentIndex = 1
            champRecherche.text = identifiantsEssai.dossier
            lancerRecherche()
            var dossiers = {}
            var autre = -1
            for (var i = 0; i < modeleMessages.count; ++i) {
                var r = modeleMessages.get(i)
                dossiers[r.chemin] = true
                if (autre < 0 && r.chemin !== "INBOX")
                    autre = i
            }
            console.log("scenario: résultats", modeleMessages.count, "| dossiers", Object.keys(dossiers).join(", "),
                        "| premier :", modeleMessages.count > 0 ? modeleMessages.get(0).cheminAffiche : "—",
                        "| historique", historiqueRecherches().slice(0, 3).join(" / "))
            if (autre < 0 || identifiantsEssai.sortie.length === 0)
                return
            choisir(autre, 0)
            var suite = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            suite.triggered.connect(function() {
                console.log("scenario: choisi", modeleMessages.get(autre).sujet, "| noyau sur", boite.dossierCourant,
                            "| affiché", sujetAffiche.text, "| liste", modeleMessages.count, "| globale", rechercheGlobale)
                quitterRecherche()
                var fin = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
                fin.triggered.connect(function() {
                    console.log("scenario: après Échap, noyau sur", boite.dossierCourant, "| liste", modeleMessages.count,
                                "| recherche", JSON.stringify(recherche), "| globale", rechercheGlobale)
                    Qt.quit()
                })
                fin.start()
            })
            suite.start()
        }
    }

    function etapeScenarioReprise() {
        console.log("scenario: proposées", redactionsRetrouvees.length, "| dialogue ouvert", dlgReprise.opened)
        dlgReprise.close()
        rouvrirRedactions()
        var jetons = Object.keys(redactions)
        for (var k = 0; k < jetons.length; ++k) {
            var r = redactions[jetons[k]]
            var c = r.contenu()
            console.log("scenario: reprise", r.idGarde, "|", c.a, "|", c.objet, "|", JSON.stringify(c.texte),
                        "| modifiée", r.modifie, "| note", r.note)
            r.conteneur.fermerRedaction()
        }
        console.log("scenario: restantes", JSON.parse(boite.redactionsGardees()).length)
    }

    /// Scénario d'essai « horsligne » : sans connexion, la boîte de réception
    /// s'ouvre sur l'index et son premier message s'affiche depuis le poste.
    function etapeScenarioHorsLigne() {
        var c = comptesConnus[0]
        ouvrirDossier(c.compte, "INBOX")
        console.log("scenario: hors ligne,", modeleMessages.count, "messages dans la liste")
        if (modeleMessages.count > 0)
            choisir(0, 0)
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
        attente.triggered.connect(function() {
            console.log("scenario: affiché :", sujetAffiche.text, "| html :", corpsHtml, "|",
                        vueCorps.text.length, "caractères",
                        vueCorps.text.indexOf("Chargement") === 0 || vueCorps.text.length < 20 ? "ECHEC" : "OK")
        })
        attente.start()
    }

    /// Scénario « mesure-recherche » : temps d'une recherche dans le dossier
    /// ouvert (index seul, hors connexion), du retour à la liste entière, et
    /// d'une recherche dans toutes les boîtes.
    function etapeScenarioMesureRecherche() {
        var c = comptesConnus[0]
        ouvrirDossier(c.compte, "INBOX")
        champRecherche.text = "Correspondant 12"
        var t = Date.now()
        lancerRecherche()
        console.log("mesure: recherche dans le dossier,", modeleMessages.count, "résultats,", Date.now() - t, "ms")
        t = Date.now()
        quitterRecherche()
        console.log("mesure: sortie de recherche,", modeleMessages.count, "messages,", Date.now() - t, "ms")
        porteeRecherche.currentIndex = 1
        champRecherche.text = "dossier 39"
        t = Date.now()
        lancerRecherche()
        console.log("mesure: recherche dans toutes les boîtes,", modeleMessages.count, "résultats,",
                    Date.now() - t, "ms")
        Qt.quit()
    }

    /// Scénario « mesure-liste » : temps d'ouverture d'INBOX depuis l'index,
    /// de trois rafraîchissements sans changement, puis d'un rafraîchissement
    /// après l'arrivée d'un message, que l'épreuve insère dans l'index pendant
    /// l'attente annoncée par « pret-insertion ».
    function etapeScenarioMesure() {
        var c = comptesConnus[0]
        mesureSuite = { paquets: 0, plusLong: 0 }
        var t = Date.now()
        ouvrirDossier(c.compte, "INBOX")
        console.log("mesure: ouverture (première page),", modeleMessages.count, "messages,", Date.now() - t, "ms")
        // La suite, par paquets : la fenêtre ne doit jamais rester figée
        // longtemps entre deux.
        var suivi = Qt.createQmlObject('import QtQuick; Timer { interval: 10; repeat: true }', fenetre)
        suivi.triggered.connect(function() {
            if (minuteurSuite.running)
                return
            suivi.stop()
            console.log("mesure: liste complète,", modeleMessages.count, "messages en", Date.now() - t, "ms |",
                        mesureSuite.paquets, "paquets, le plus long", mesureSuite.plusLong, "ms")
            mesureSuite = null
            var index = JSON.parse(boite.messages())
            var conforme = index.length === modeleMessages.count
            for (var i = 0; conforme && i < index.length; ++i)
                conforme = index[i].uid === modeleMessages.get(i).uid
            console.log("mesure: liste conforme à l'index :", conforme)
            // Relecture complète pendant qu'un message lointain est choisi : la
            // suite est lue d'un trait jusqu'à lui, il reste choisi.
            var loin = modeleMessages.get(30000).uid
            uidCourant = loin
            selection = ({ [loin]: true })
            var t3 = Date.now()
            rafraichirListe(true)
            console.log("mesure: relecture complète, message choisi au rang 30 000 :", Date.now() - t3, "ms |",
                        modeleMessages.count, "lignes | toujours choisi", uidCourant === loin && selection[loin] === true,
                        "| à son rang", indexDe(loin) === 30000)
            arreterSuite()
            uidCourant = 0
            selection = ({})
            rafraichirListe(true)
            completerListe()
            etapeScenarioMesureSuite()
        })
        suivi.start()
    }

    function etapeScenarioMesureSuite() {
        var t
        for (var k = 0; k < 3; ++k) {
            t = Date.now()
            rafraichirListe()
            console.log("mesure: rafraîchissement sans changement,", Date.now() - t, "ms")
        }
        // Ce que faisait la 0.4.4 à chaque rafraîchissement : la liste entière
        // relue, puis comparée ligne à ligne.
        for (k = 0; k < 3; ++k) {
            t = Date.now()
            var liste = JSON.parse(boite.messages())
            var memeForme = liste.length === modeleMessages.count
            for (var i = 0; memeForme && i < liste.length; ++i)
                memeForme = liste[i].uid === modeleMessages.get(i).uid
            for (var j = 0; memeForme && j < liste.length; ++j)
                modeleMessages.get(j).lu !== liste[j].lu
            console.log("mesure: ancien rafraîchissement sans changement,", Date.now() - t, "ms")
        }
        console.log("scenario: pret-insertion")
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
        attente.triggered.connect(function() {
            var t2 = Date.now()
            rafraichirListe()
            console.log("mesure: rafraîchissement après une arrivée,", modeleMessages.count, "messages,",
                        Date.now() - t2, "ms | premier :", modeleMessages.get(0).sujet)
            // L'ancienne reconstruction, sur la même liste.
            t2 = Date.now()
            poserListe(JSON.parse(boite.messages()))
            console.log("mesure: ancienne reconstruction,", Date.now() - t2, "ms")
            Qt.quit()
        })
        attente.start()
    }

    /// Scénario d'essai « defilement » : l'arborescence, agrandie pour
    /// déborder et défilée tout en bas, doit remonter d'elle-même quand un
    /// élément glissé se tient contre son bord haut.
    function etapeScenarioDefilement() {
        if (essai.defilement)
            return
        essai.defilement = true
        reglages.zoomArborescence = 3
        // « favoris-fixes » : les comptes défilés tout en bas, sans glisser —
        // la capture montre les Favoris restés en haut.
        if (essai.scenario === "favoris-fixes") {
            var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1000 }', fenetre)
            attente.triggered.connect(function() {
                var v = vueArborescence
                v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
                console.log("scenario: comptes défilés à", v.contentY, "; Favoris visibles :", vueFavoris.visible, vueFavoris.count, "lignes")
            })
            attente.start()
            return
        }
        Qt.callLater(function() {
            var v = vueArborescence
            v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
            var avant = v.contentY
            var sonde = Qt.createQmlObject('import QtQuick; Rectangle { width: 40; height: 16 }', fenetre.contentItem)
            var p = v.mapToItem(fenetre.contentItem, 20, 4)
            sonde.x = p.x
            sonde.y = p.y
            glisse = sonde
            var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            attente.triggered.connect(function() {
                glisse = null
                console.log("scenario: défilement de", avant, "à", v.contentY, "(haut :", v.originY + ")",
                            v.contentY <= v.originY + 1 ? "OK" : "ECHEC")
            })
            attente.start()
        })
    }

    /// Scénario d'essai « signature » : importe la signature Outlook déposée
    /// dans le dossier de sortie (signature.htm et ses images), la pose sur le
    /// compte, rédige un message à `MMAIL_DESTINATAIRE` et l'envoie.
    function etapeScenarioSignature() {
        if (essai.signatureFaite)
            return
        essai.signatureFaite = true
        var a = comptesConnus[0]
        var html = boite.importerSignature(a.adresse, identifiantsEssai.sortie + "/signature.htm")
        console.log("scenario: signature importée :", html.length, "caractères", boite.erreur)
        poserIdentite(a.adresse, { nom: "Essai MMail", signature: "Essai MMail", html: html,
                                   nouveaux: true, reponses: true })
        if (essai.scenario === "signature-vue") {
            dlgIdentite.ouvrir(a.compte)
            return
        }
        var r = ouvrirRedaction(a.compte)
        essai.objetSignature = "Essai signature HTML " + Date.now()
        r.remplir({ a: identifiantsEssai.utilisateur2, objet: essai.objetSignature, mode: "nouveau" })
        var corps = r.htmlActuel()
        console.log("scenario: rédaction :", (corps.match(/<img[^>]*>/g) || []).join(" "))
        r.envoyer(false)
    }

    /// Scénario d'essai « chevron » : un clic sur le chevron du dossier
    /// « Essais », tel que la ligne le reçoit, doit replier ses sous-dossiers ;
    /// un second, les déplier.
    function etapeScenarioChevron() {
        if (essai.etapeChevron === undefined)
            essai.etapeChevron = 0
        // Trois clics, chacun à un tour de boucle d'événements du précédent :
        // replier « INBOX/Fournisseurs », le déplier, replier la boîte de
        // réception.
        var cibles = identifiantsEssai.dossier ? [identifiantsEssai.dossier, identifiantsEssai.dossier]
                                               : ["INBOX/Fournisseurs", "INBOX/Fournisseurs", "INBOX"]
        if (essai.etapeChevron >= cibles.length)
            return
        var present = function(chemin) {
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier" && l.chemin === chemin)
                    return k
            }
            return -1
        }
        var etat = function() {
            var noms = []
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier")
                    noms.push(l.chemin + (l.enfants ? (l.replie ? "[+]" : "[-]") : ""))
            }
            return noms.join(" | ")
        }
        var cible = cibles[essai.etapeChevron]
        var i = present(cible)
        var ligne = i >= 0 ? vueArborescence.itemAtIndex(i) : null
        // Un clic au milieu de la place du chevron, où le doigt se pose :
        // retrait du dossier plus la moitié de la largeur du chevron.
        var x = ligne ? ligne.leftPadding + modeleArborescence.get(i).profondeur * 14 + 7 : -1
        if (ligne && !ligne.surChevron(x))
            x = -1
        console.log("scenario: clic sur le chevron de", cible, "à x =", x)
        if (x < 0)
            return
        boite.replierDossier(modeleArborescence.get(i).compte, cible, !modeleArborescence.get(i).replie)
        console.log("scenario: arbre :", etat())
        essai.etapeChevron += 1
        Qt.callLater(etapeScenarioChevron)
    }

    /// Scénario d'essai « aller-retour » (MMAIL_SCENARIO) : le premier message
    /// de la boîte de réception du premier compte part dans celle du second par
    /// les fonctions mêmes de l'interface, puis revient. Chaque étape est tracée
    /// sur la console ; sert à éprouver le tri sans personne devant l'écran.
    function etapeScenario() {
        if (!essai || essai.scenario !== "aller-retour")
            return
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(0).sujet
            console.log("scenario: aller", essai.sujet, "de", a.adresse, "vers", b.adresse)
            choisir(0, 0)
            deplacerVers(b.id, "INBOX")
        } else if (essai.etape === 1 && boite.compteCourant === a.id && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(b.id, "INBOX")
        } else if (essai.etape === 2 && boite.compteCourant === b.id) {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: trouve dans", b.adresse, "- retour")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC, message absent de", b.adresse)
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 4
            ouvrirDossier(a.id, "INBOX")
        } else if (essai.etape === 4 && boite.compteCourant === a.id) {
            var revenu = false
            for (var j = 0; j < modeleMessages.count; ++j)
                revenu = revenu || modeleMessages.get(j).sujet === essai.sujet
            console.log(revenu ? "scenario: OK, message revenu dans " + a.adresse
                               : "scenario: ECHEC, message non revenu")
            essai.etape = 9
        }
    }

    /// Scénario d'essai « pieces » (MMAIL_SCENARIO) : dans la boîte de
    /// réception du premier compte, le premier message portant une pièce
    /// jointe est affiché, et sa première pièce enregistrée dans le dossier
    /// désigné par MMAIL_SORTIE. Sert à éprouver la chaîne sans écran.
    /// Scénario « ouvrir-avec » : MMAIL_DOSSIER désigne un programme d'essai.
    /// Retenu pour les .pdf, il ouvre le PDF du premier message ; « Ouvrir
    /// avec… » ponctuel sur le tableur ; puis le choix est retiré.
    function etapeScenarioOuvrirAvec() {
        if (essai.etape !== 0 || pieces.length === 0)
            return
        essai.etape = 1
        var programme = identifiantsEssai.dossier
        var de = function(ext) { return pieces.filter(function(p) { return extensionDe(p.nom) === ext })[0] }
        retenirLogiciel("pdf", programme)
        console.log("scenario: retenu pour .pdf :", lanceur.nom(logicielPour(de("pdf").nom)))
        ouvrirPiece(de("pdf"))
        var suite = Qt.createQmlObject('import QtQuick; Timer { interval: 2500 }', fenetre)
        suite.triggered.connect(function() {
            console.log("scenario: après le PDF :", messageEtat.texte)
            ouvrirPiece(de("xlsx"), programme)
            var fin = Qt.createQmlObject('import QtQuick; Timer { interval: 2500 }', fenetre)
            fin.triggered.connect(function() {
                console.log("scenario: après le tableur :", messageEtat.texte)
                retenirLogiciel("pdf", "")
                console.log("scenario: retenus après retrait :", JSON.stringify(logicielsPieces()))
                Qt.quit()
            })
            fin.start()
        })
        suite.start()
    }

    function etapeScenarioPieces() {
        if (essai.etape === 0 && pieces.length > 0) {
            essai.etape = 1
            console.log("scenario: pieces", JSON.stringify(pieces))
            boite.enregistrerPiece(uidCourant, pieces[0].indice,
                                   identifiantsEssai.sortie + "/" + encodeURIComponent(pieces[0].nom))
        } else if (essai.etape === 0 && essai.examines < modeleMessages.count - 1) {
            essai.examines += 1
            choisir(essai.examines, 0)
        }
    }

    // Sonde du scénario « glisser » : un objet de glisser-déposer Qt Quick,
    // déposé par programme sur une ligne de l'arborescence. Les zones de dépôt
    // le reçoivent exactement comme ce qu'on glisse à la souris.
    Item {
        id: sonde
        property int compte: 0
        property string chemin: ""
        width: 4; height: 4
        Drag.hotSpot.x: 2
        Drag.hotSpot.y: 2
    }

    /// Dépose la sonde sur la ligne `index` de l'arborescence.
    function deposerSonde(cible, clef, compte, chemin) {
        if (!cible)
            return false
        cible.vue.positionViewAtIndex(cible.index, ListView.Contain)
        var ligne = cible.vue.itemAtIndex(cible.index)
        if (!ligne)
            return false
        var point = ligne.mapToItem(fenetre.contentItem, 20, ligne.height / 2)
        sonde.parent = fenetre.contentItem
        sonde.compte = compte
        sonde.chemin = chemin
        sonde.Drag.keys = [clef]
        sonde.x = point.x - 2
        sonde.y = point.y - 2
        sonde.Drag.active = true
        var action = sonde.Drag.drop()
        sonde.Drag.active = false
        return action !== Qt.IgnoreAction
    }

    /// Ligne de l'arborescence : sa liste (Favoris ou comptes) et son rang.
    function indexLigne(genre, compte, chemin) {
        var favoris = genre === "rubrique" || genre === "favori" || genre === "favori-vide"
        var modele = favoris ? modeleFavoris : modeleArborescence
        for (var i = 0; i < modele.count; ++i) {
            var l = modele.get(i)
            if (l.genre === genre && (compte === 0 || l.compte === compte) && (chemin === "" || l.chemin === chemin))
                return { vue: favoris ? vueFavoris : vueArborescence, index: i }
        }
        return null
    }

    function favorisActuels() {
        return JSON.parse(boite.arborescence(false))
                .filter(function(l) { return l.genre === "favori" })
                .map(function(l) { return l.adresse + ":" + l.chemin })
    }

    /// Scénario « glisser » (MMAIL_SCENARIO) : deux dossiers déposés sur la
    /// rubrique Favoris puis l'un sur l'autre, et un message déposé sur un
    /// dossier puis ramené. Chaque étape est tracée ; les favoris sont retirés
    /// à la fin.
    function etapeScenarioGlisser() {
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            var ok1 = deposerSonde(indexLigne("rubrique", 0, ""), "mmail/dossier", a.id, "Essais")
            var ok2 = deposerSonde(indexLigne("favori", a.id, "Essais"), "mmail/dossier", b.id, "INBOX")
            var ordre = favorisActuels()
            console.log("scenario: favoris", ok1, ok2, JSON.stringify(ordre))
            console.log(ordre.length === 2 && ordre[0] === b.adresse + ":INBOX" && ordre[1] === a.adresse + ":Essais"
                        ? "scenario: OK favoris ordonnes" : "scenario: ECHEC favoris")
            // Un dossier déposé sur un dossier ordinaire ne fait rien.
            var refus = deposerSonde(indexLigne("dossier", a.id, "Archive"), "mmail/dossier", a.id, "Essais")
            console.log(refus ? "scenario: ECHEC dossier accepte hors Favoris" : "scenario: OK dossier refuse hors Favoris")
            // Un message déposé sur un dossier y part.
            essai.sujet = modeleMessages.get(0).sujet
            choisir(0, 0)
            var ok3 = deposerSonde(indexLigne("dossier", a.id, "Essais/Clients"), "mmail/messages", 0, "")
            console.log("scenario: message depose", ok3, essai.sujet)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(a.id, "Essais/Clients")
        } else if (essai.etape === 2 && boite.dossierCourant === "Essais/Clients") {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message arrive dans Essais/Clients")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de Essais/Clients")
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            boite.epinglerDossier(a.id, "Essais", false)
            boite.epinglerDossier(b.id, "INBOX", false)
            console.log("scenario: message ramene, favoris retires :", JSON.stringify(favorisActuels()))
        }
    }

    /// Contrôle de fabrication : l'index local s'ouvre, s'écrit et se relit,
    /// sans toucher au réseau ; le coffre répond. Appelé par --smoke.
    function controleIndex() {
        if (!boite.ouvrirProfil(cheminProfil))
            return "profil : " + boite.erreur
        var arborescence = JSON.parse(boite.arborescence(true))
        if (!Array.isArray(arborescence))
            return "arborescence illisible"
        rafraichirArborescence()
        if (JSON.parse(boite.messages()).length !== 0)
            return "aucun dossier ouvert, donc aucun message"
        if (!Array.isArray(JSON.parse(boite.dossiersCibles())))
            return "cibles illisibles"
        if (coffre.service !== "MMail")
            return "coffre absent"
        // Un appel sans session ne doit ni bloquer ni réussir.
        if (boite.ouvrirDossier(0, "INBOX"))
            return "une commande a été acceptée sans compte"
        if (boite.deplacer("1", 0, "INBOX"))
            return "un déplacement a été accepté sans dossier ouvert"
        if (boite.ouvrirPiece(1, 0))
            return "une pièce jointe a été demandée sans dossier ouvert"
        // Presse-papier : la copie explicite écrit, et la règle y reconnaît la
        // sienne — pas de protection contre MMail lui-même.
        copierTexte("essai de copie", "message")
        if (reglePresse.libelle !== "message")
            return "règle du presse-papier : origine « " + reglePresse.libelle + " »"
        if (!reglePresse.peutEcraser(maintenant()))
            return "règle du presse-papier : protection contre sa propre copie"
        if (premierePolice(["police-inexistante", "autre-inexistante"]) !== "autre-inexistante")
            return "repli de police"
        // La rubrique Favoris est toujours là, même vide.
        if (arborescence.length === 0 || arborescence[0].genre !== "rubrique")
            return "rubrique Favoris absente"
        // Apparences : la palette suit le choix, et « systeme » la rend.
        var fondSysteme = palette.window.toString()
        var apparenceAvant = apparence
        choisirApparence("classique")
        if (palette.window.toString() !== "#d4d0c8")
            return "apparence classique non appliquée : " + palette.window
        choisirApparence("moderne")
        if (palette.window.toString() !== "#f6f7f8")
            return "apparence moderne non appliquée : " + palette.window
        choisirApparence("mmedia")
        if (palette.window.toString() !== "#f1f2f2" || palette.highlight.toString() !== "#d6eefa")
            return "apparence MMEDIA non appliquée : " + palette.window
        // Les repères de marque sont lus : bandeau à la place du fond du
        // style, texte mis en valeur en cyan foncé…
        if (marque === null || entete.background.visible || texteAccent.toString() !== "#0e6f9e")
            return "repères de l'apparence MMEDIA non appliqués"
        // … et rendus en la quittant : fond du style, couleur de sélection.
        choisirApparence("moderne")
        if (marque !== null || !entete.background.visible
                || accent.toString() !== palette.highlight.toString())
            return "repères de l'apparence MMEDIA non rendus"
        choisirApparence("systeme")
        if (palette.window.toString() !== fondSysteme && apparenceAvant === "systeme")
            return "apparence système non rendue"
        choisirApparence(apparenceAvant)
        // Zoom borné, et propre à chaque colonne.
        var zoomAvant = [reglages.zoomArborescence, reglages.zoomListe, reglages.zoomMessage]
        if (poserZoom("liste", 10) !== zoomMaximum || poserZoom("liste", 0.01) !== zoomMinimum)
            return "zoom non borné"
        poserZoom("message", 2)
        if (reglages.zoomListe !== zoomMinimum || tailleMessage !== tailleBase * 2)
            return "zooms mêlés entre colonnes"
        reglages.zoomArborescence = zoomAvant[0]
        reglages.zoomListe = zoomAvant[1]
        reglages.zoomMessage = zoomAvant[2]
        return "ok"
    }
}
