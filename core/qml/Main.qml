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
        // Dernier dossier où une pièce jointe a été enregistrée (adresse
        // file:///), proposé à l'enregistrement suivant.
        property string dernierDossier: ""
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

    // Android : Qt montre la barre d'Android (Copier, Tout sélectionner) tant
    // qu'un texte en lecture seule garde le focus et une sélection, et ne la
    // retire sur aucune autre demande (qandroidinputcontext.cpp, Qt 6.8). Un
    // toucher n'importe où dans la fenêtre — le corps du message comme la
    // liste, dont les lignes ne prennent pas le focus — retire donc la
    // sélection, et la barre avec. Gestionnaire passif : il voit le toucher
    // sans le prendre à l'élément touché. L'appui long n'est pas un toucher.
    TapHandler {
        id: androidSansSelection
        parent: fenetre.contentItem
        enabled: Qt.platform.os === "android" && colonneLecture.vueCorps.selectedText.length > 0
        acceptedDevices: PointerDevice.TouchScreen
        gesturePolicy: TapHandler.DragThreshold
        onTapped: colonneLecture.vueCorps.deselect()
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
    // Vue tactile (décision 32) : celle d'Android, qui suit les usages du
    // système — menus au point d'appui, ni raccourcis clavier ni agenda. À
    // distinguer de la vue compacte, qui ne dit que la largeur : une fenêtre
    // étroite sur le bureau reste à la souris et au clavier.
    readonly property bool tactile: Qt.platform.os === "android" || identifiantsEssai.tactile === true
    property int vue: 0
    // « courrier » ou « agenda » : ce qu'occupe la fenêtre sous la barre
    // d'outils.
    property string module: "courrier"

    // Sélection de la liste : uid → vrai. Réaffectée à chaque changement pour
    // que les liaisons qui la lisent soient réévaluées.
    property var selection: ({})
    property int ancre: -1
    property int uidCourant: 0
    /// Adresse de l'expéditeur du message affiché (« Copier l'adresse » de l'en-tête).
    property string adresseAffichee: ""
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
    // … et ce que le serveur y trouve, dossier par dossier, dans le texte des
    // messages : « compte\nchemin » → UID, rangés dans l'index par le noyau.
    // `dossiersPartout` : dossiers à interroger, `dossiersFaits` : ceux qui
    // ont répondu. La liste s'arrête à `resultatsMax` lignes, les plus
    // récentes : au-delà, l'affichage ralentirait sans rien apprendre.
    property var trouvesPartout: ({})
    property int dossiersPartout: 0
    property int dossiersFaits: 0
    readonly property int resultatsMax: 1000
    property bool resultatsTronques: false
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
        // Deux barres, une seule visible : la hauteur se donne ici — avec deux
        // enfants, la barre ne la tire plus d'elle-même et s'écrasait
        // (constaté par Manu sur le téléphone le 18/10, et vrai sur le bureau).
        // Au doigt, 48 au moins : la hauteur d'une barre d'application d'Android.
        contentHeight: fenetre.tactile ? Math.max(48, barreTactile.implicitHeight)
                                       : barreBureau.implicitHeight
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

        // Vue tactile (décision 32) : la barre d'application d'Android —
        // ☰ ouvre l'arborescence en tiroir (‹ revient du message à la liste),
        // le nom du dossier, la loupe, ⋮ pour le reste.
        RowLayout {
            id: barreTactile
            visible: fenetre.tactile
            anchors.fill: parent
            anchors.leftMargin: 2
            anchors.rightMargin: 2
            spacing: 2
            BoutonEntete {
                text: fenetre.vue === 2 ? "‹" : ""
                font.pixelSize: 22
                Layout.preferredWidth: 48
                Layout.fillHeight: true
                Accessible.name: fenetre.vue === 2 ? qsTr("Retour à la liste") : qsTr("Dossiers")
                onClicked: {
                    if (fenetre.vue === 2)
                        fenetre.vue = 1
                    else
                        tiroir.open()
                }
                Canvas {
                    visible: fenetre.vue !== 2
                    anchors.centerIn: parent
                    width: 22
                    height: 18
                    property color encre: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                    onEncreChanged: requestPaint()
                    onPaint: {
                        var c = getContext("2d")
                        c.reset()
                        c.fillStyle = encre
                        for (var i = 0; i < 3; ++i)
                            c.fillRect(0, i * 7.5, width, 3)
                    }
                }
            }
            Label {
                text: fenetre.vueUnifiee ? qsTr("Toutes les BàL")
                      : boite.dossierCourant.length > 0 ? fenetre.libelleCourant() : "MMail"
                color: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                elide: Text.ElideRight
                font.bold: true
                font.pointSize: fenetre.tailleBase * 1.15
                Layout.fillWidth: true
                Layout.leftMargin: 4
            }
            // « Toutes les BàL » reste à portée de pouce (retour de Manu du
            // 18/10), avec ses non-lus.
            BoutonEntete {
                cerne: true
                compteur: fenetre.nonLusReceptions
                text: qsTr("BàL") + (fenetre.marque === null && fenetre.nonLusReceptions > 0
                                     ? " (" + fenetre.nonLusReceptions + ")" : "")
                font.bold: fenetre.nonLusReceptions > 0
                checkable: true
                checked: fenetre.vueUnifiee
                Layout.fillHeight: true
                Accessible.name: qsTr("Toutes les boîtes de réception")
                onClicked: {
                    checked = Qt.binding(function() { return fenetre.vueUnifiee })
                    fenetre.ouvrirBoitesUnifiees()
                }
            }
            BusyIndicator {
                running: boite.occupe
                visible: running
                Layout.preferredHeight: 26
                Layout.preferredWidth: 26
            }
            BoutonEntete {
                Layout.preferredWidth: 48
                Layout.fillHeight: true
                Accessible.name: qsTr("Rechercher")
                onClicked: {
                    fenetre.vue = 1
                    colonneListe.rechercheOuverte = true
                    colonneListe.champRecherche.forceActiveFocus()
                }
                Canvas {
                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    property color encre: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                    onEncreChanged: requestPaint()
                    onPaint: fenetre.dessinerLoupe(getContext("2d"), width, height, encre)
                }
            }
            BoutonEntete {
                id: boutonPlus
                Canvas {
                    anchors.centerIn: parent
                    width: 6
                    height: 22
                    property color encre: fenetre.marque ? fenetre.marque.texteBandeau : fenetre.palette.windowText
                    onEncreChanged: requestPaint()
                    onPaint: {
                        var c = getContext("2d")
                        c.reset()
                        c.fillStyle = encre
                        for (var i = 0; i < 3; ++i) {
                            c.beginPath()
                            c.arc(3, 3 + i * 8, 2.6, 0, 2 * Math.PI)
                            c.fill()
                        }
                    }
                }
                Layout.preferredWidth: 48
                Layout.fillHeight: true
                Accessible.name: qsTr("Plus d'options")
                onClicked: menuPlus.popup(boutonPlus, boutonPlus.width - menuPlus.width, boutonPlus.height)
            }
        }

        RowLayout {
            id: barreBureau
            visible: !fenetre.tactile
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
                visible: fenetre.compact && !fenetre.tactile
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

    BandeauVersion { id: bandeauVersion }

    // Vue tactile : l'arborescence dans un tiroir, tiré du bord gauche ou
    // ouvert par ☰ ; « Toutes les BàL » en tête, comme les applications de
    // messagerie d'Android. Le volet de l'arborescence des volets n'y sert pas.
    Drawer {
        id: tiroir
        edge: Qt.LeftEdge
        interactive: fenetre.tactile
        width: Math.min(fenetre.width * 0.85, 360)
        height: fenetre.height
        ColumnLayout {
            anchors.fill: parent
            spacing: 0
            ItemDelegate {
                Layout.fillWidth: true
                text: qsTr("Toutes les BàL")
                      + (fenetre.nonLusReceptions > 0 ? "  (" + fenetre.nonLusReceptions + ")" : "")
                font.bold: fenetre.nonLusReceptions > 0
                highlighted: fenetre.vueUnifiee
                onClicked: fenetre.ouvrirBoitesUnifiees()
            }
            VoletArborescence {
                id: arborescenceTiroir
                visible: true
                Layout.fillWidth: true
                Layout.fillHeight: true
            }
        }
    }

    // Bandeau du glisser d'une ligne : ce qui vient d'être fait, et « Annuler ».
    Popup {
        id: bandeauGlissement
        property string texte: ""
        x: Math.round((parent.width - width) / 2)
        y: parent.height - height - 90
        width: Math.min(parent.width - 24, 460)
        closePolicy: Popup.NoAutoClose
        padding: 8
        // Sombre sur tout fond, comme le bandeau d'Android : il se voit.
        background: Rectangle {
            color: "#2b2b2b"
            radius: 12
        }
        RowLayout {
            anchors.fill: parent
            spacing: 10
            Label {
                text: bandeauGlissement.texte
                color: "#ffffff"
                wrapMode: Text.Wrap
                font.pointSize: fenetre.tailleBase * 1.15
                Layout.fillWidth: true
                Layout.leftMargin: 8
            }
            // Un gros rond, une flèche de retour : la cible du pouce.
            RoundButton {
                id: boutonAnnuler
                Layout.preferredWidth: 56
                Layout.preferredHeight: 56
                highlighted: true
                Accessible.name: qsTr("Annuler")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Annuler")
                onClicked: fenetre.annulerGlissement()
                contentItem: Canvas {
                    property color encre: "#ffffff"
                    onPaint: {
                        var c = getContext("2d")
                        c.reset()
                        var w = width, h = height
                        var cx = w * 0.53, cy = h * 0.56, r = w * 0.22
                        c.strokeStyle = encre
                        c.fillStyle = encre
                        c.lineWidth = Math.max(2, w * 0.08)
                        c.lineCap = "round"
                        c.beginPath()
                        c.arc(cx, cy, r, Math.PI, Math.PI * 2.3, false)
                        c.stroke()
                        var ax = cx - r, ay = cy
                        c.beginPath()
                        c.moveTo(ax - w * 0.12, ay - h * 0.03)
                        c.lineTo(ax + w * 0.12, ay - h * 0.03)
                        c.lineTo(ax, ay + h * 0.15)
                        c.closePath()
                        c.fill()
                    }
                }
            }
        }
    }

    // Vue tactile : « Nouveau message » en bouton flottant, en bas à droite de
    // la liste — l'usage d'Android.
    RoundButton {
        visible: fenetre.tactile && fenetre.vue === 1 && fenetre.module === "courrier"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 16
        z: 10
        width: 58
        height: 58
        text: "+"
        font.pixelSize: 30
        highlighted: true
        Accessible.name: qsTr("Nouveau message")
        onClicked: fenetre.rediger("nouveau")
    }

    // Vue tactile : le menu ⋮ — « Comptes… » et « Affichage… » ouvrent leurs
    // menus à part (des sous-menus se choisissaient mal au doigt), puis ce que
    // la barre de bureau montre en boutons.
    Menu {
        id: menuPlus
        onAboutToShow: width = fenetre.largeurMenu(menuPlus)
        MenuItem {
            text: qsTr("Nouveau message")
            onTriggered: fenetre.rediger("nouveau")
        }
        MenuItem {
            text: qsTr("Toutes les BàL")
                  + (fenetre.nonLusReceptions > 0 ? "  (" + fenetre.nonLusReceptions + ")" : "")
            onTriggered: fenetre.ouvrirBoitesUnifiees()
        }
        MenuItem {
            text: qsTr("Carnet d'adresses…")
            onTriggered: dlgCarnet.open()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Comptes…")
            onTriggered: Qt.callLater(function() {
                menuComptes.popup(boutonPlus, boutonPlus.width - Math.max(menuComptes.width, 200), boutonPlus.height)
            })
        }
        MenuItem {
            text: qsTr("Affichage…")
            onTriggered: Qt.callLater(function() {
                menuAffichage.popup(boutonPlus, boutonPlus.width - Math.max(menuAffichage.width, 200), boutonPlus.height)
            })
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Actualiser")
            onTriggered: boite.actualiser()
        }
        MenuItem {
            text: qsTr("Dossiers masqués")
            checkable: true
            checked: reglages.afficherMasques
            onTriggered: {
                reglages.afficherMasques = checked
                fenetre.rafraichirArborescence()
            }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Aide")
            onTriggered: dlgAide.open()
        }
        MenuItem {
            text: qsTr("À propos de MMail…")
            onTriggered: dlgAPropos.open()
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

    PopupRappels { id: popupRappels }

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

    // Modèles des volets, à la racine depuis le découpage de la 0.5.16 : la
    // fenêtre, les scénarios et les réactions au noyau s'en servent.
    ListModel { id: modeleFavoris }
    ListModel { id: modeleArborescence }
    ListModel {
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

    SplitView {
        id: volets
        anchors.fill: parent
        anchors.topMargin: bandeauVersion.height
        orientation: Qt.Horizontal
        visible: fenetre.module === "courrier"

        VoletArborescence { id: colonneArborescence }

        VoletListe { id: colonneListe }

        VoletLecture { id: colonneLecture }
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
            scenarios.etapeScenarioMiseAJour("proposee")
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
            scenarios.etapeScenarioMiseAJour("prete")
    }

    function miseAJourEchouee(version, message) {
        erreurMiseAJour = message
        etatMiseAJour = "echec"
        if (essai && essai.scenario.indexOf("mise-a-jour") === 0)
            scenarios.etapeScenarioMiseAJour("echec")
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

    /// Pas de copie automatique sous Android : la barre d'Android a « Copier »,
    /// et chaque dépôt au presse-papier y fait paraître un aperçu (Android 13).
    function selectionChangee() {
        if (reglagesEdition.copieAuto && Qt.platform.os !== "android")
            minuteurSelection.restart()
    }

    /// Copie automatique : la sélection du message part au presse-papier une
    /// fois stabilisée, sauf si un autre logiciel vient d'y déposer quelque
    /// chose — ce contenu-là est protégé une minute.
    function copierSelectionAuto() {
        var texte = colonneLecture.vueCorps.selectedText
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
        Redaction {}
    }

    // ------------------------------------------------ ligne d'arborescence
    Component {
        id: ligneArborescence
        LigneArborescence {}
    }

    // ---------------------------------------------------- ligne de message
    Component {
        id: ligneMessage
        LigneMessage {}
    }

    MenuComptes { id: menuComptes }

    Menu {
        id: menuAide
        onAboutToShow: width = fenetre.largeurMenu(menuAide)
        MenuItem {
            text: fenetre.tactile ? qsTr("Aide") : qsTr("Aide (F1)")
            onTriggered: dlgAide.open()
        }
        MenuItem {
            text: qsTr("À propos de MMail…")
            onTriggered: dlgAPropos.open()
        }
    }

    MenuAffichage { id: menuAffichage }

    MenuPiece { id: menuPiece }

    // « Ouvrir avec… » : un logiciel du poste, pour cette fois ou pour toutes
    // les pièces de la même extension (décision 9). `sansLogiciel` : le
    // système n'en connaît aucun pour ce type, le choix est alors retenu
    // d'avance (cf. ouvrirPiece).
    FileDialog {
        id: dlgChoisirLogiciel
        property var piece: null
        property bool sansLogiciel: false
        function choisir(p, sans) {
            piece = p
            sansLogiciel = sans
            open()
        }
        title: qsTr("Ouvrir « %1 » avec…").arg(piece ? piece.nom : "")
        fileMode: FileDialog.OpenFile
        nameFilters: Qt.platform.os === "windows" ? [qsTr("Programmes (*.exe)"), qsTr("Tous les fichiers (*)")] : []
        onAccepted: dlgOuvrirAvec.ouvrir(piece, selectedFile.toString(), sansLogiciel)
    }

    DialogueOuvrirAvec { id: dlgOuvrirAvec }

    DialogueLogiciels { id: dlgLogiciels }

    FileDialog {
        id: dlgEnregistrerPiece
        property var piece: null
        property int uid: 0
        title: qsTr("Enregistrer la pièce jointe")
        fileMode: FileDialog.SaveFile
        onAccepted: {
            var fichier = selectedFile.toString()
            reglagesPieces.dernierDossier = fichier.substring(0, fichier.lastIndexOf("/"))
            boite.enregistrerPiece(uid, piece.indice, fichier)
        }
    }

    // Clic droit sur l'objet ou l'expéditeur du message affiché.
    Menu {
        id: menuEnTete
        property string texteChoisi: ""
        font.pointSize: fenetre.tailleColonne("message")
        onAboutToShow: width = fenetre.largeurMenu(menuEnTete)
        MenuItem {
            text: qsTr("Copier")
            enabled: menuEnTete.texteChoisi.length > 0
            onTriggered: fenetre.copierTexte(menuEnTete.texteChoisi, "message")
        }
        MenuItem {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: fenetre.adresseAffichee.length > 0
            onTriggered: fenetre.copierTexte(fenetre.adresseAffichee, "message")
        }
        MenuItem {
            text: qsTr("Copier l'objet")
            enabled: colonneLecture.sujetAffiche.text.length > 0
            onTriggered: fenetre.copierTexte(colonneLecture.sujetAffiche.text, "message")
        }
    }

    Menu {
        id: menuCorps
        font.pointSize: fenetre.tailleColonne("message")
        onAboutToShow: width = fenetre.largeurMenu(menuCorps)
        MenuItem {
            text: qsTr("Copier")
            enabled: colonneLecture.vueCorps.selectedText.length > 0
            onTriggered: fenetre.copierTexte(colonneLecture.vueCorps.selectedText, fenetre.sourceVisible ? "source" : "message")
        }
        MenuItem {
            text: qsTr("Tout sélectionner")
            onTriggered: colonneLecture.vueCorps.selectAll()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Copier la sélection automatiquement")
            checkable: true
            checked: reglagesEdition.copieAuto
            onTriggered: reglagesEdition.copieAuto = checked
        }
    }

    MenuMessage { id: menuMessage }

    MenuDossier { id: menuDossier }

    MenuCompte { id: menuCompte }

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
            colonneListe.champRecherche.forceActiveFocus()
            colonneListe.champRecherche.selectAll()
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
        enabled: !fenetre.dialogueOuvert() && !colonneLecture.vueCorps.activeFocus
        onActivated: fenetre.toutChoisir()
    }

    DialogueCompte { id: dlgCompte }

    DialogueConsentement { id: dlgConsentement }

    DialogueIdentite { id: dlgIdentite }

    DialogueReprise { id: dlgReprise }

    DialogueRestauration { id: dlgRestauration }

    DialogueLien { id: dlgLien }

    DialogueDeplacer { id: dlgDeplacer }

    DialogueAide { id: dlgAide }

    DialogueAPropos { id: dlgAPropos }

    DialogueFiltres { id: dlgFiltres }

    // Sous Android : la page de connexion Microsoft intégrée (cf.
    // ConnexionIntegree.qml, absente du module ailleurs).
    Loader {
        id: chargeurConnexion
        active: Qt.platform.os === "android"
        source: active ? "ConnexionIntegree.qml" : ""
    }
    Connections {
        target: chargeurConnexion.item
        ignoreUnknownSignals: true
        function onRetour(url) {
            if (!boite.terminerMicrosoft(url))
                messageEtat.texte = qsTr("Retour de connexion inattendu : relancez « Ajouter un compte Microsoft… ».")
        }
        function onAbandon() {
            messageEtat.texte = qsTr("Connexion à Microsoft abandonnée.")
        }
    }

    DialogueCarnet { id: dlgCarnet }

    // « Créer un filtre… » sur un message : un filtre sur son expéditeur, pour
    // son compte, à compléter dans le dialogue des filtres.
    function creerFiltre() {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        var r = null
        for (var i = 0; i < modeleMessages.count; ++i) {
            var m = modeleMessages.get(i)
            if (m.uid === uids[0] && (!listeMulti || cleResultat(m.compte, m.chemin, m.uid) === cleChoisie)) {
                r = m
                break
            }
        }
        if (!r)
            return
        var compte = listeMulti ? r.compte : boite.compteCourant
        var adresse = ""
        for (var c = 0; c < comptesConnus.length; ++c) {
            if (comptesConnus[c].compte === compte)
                adresse = comptesConnus[c].adresse
        }
        dlgFiltres.ouvrir({ nom: r.expediteur || r.adresse, actif: true, compte: adresse, tout: true,
                            conditions: [{ champ: "expediteur", operateur: "est", valeur: r.adresse }],
                            lu: false, suivi: false, destination: "dossier", dossier: "", arreter: false })
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

    DialogueViderDossier { id: dlgViderDossier }

    DialogueNouveauDossier { id: dlgNouveauDossier }

    // ------------------------------------------------------ coffre et session
    Coffre {
        id: coffre

        onLu: function(cle, secret, trouve, erreur) {
            var compte = fenetre.compteParCle(cle)
            if (!compte)
                return
            // « Modifier le compte… » : nouveau serveur de lecture, le mot de
            // passe vient de l'ancienne clé.
            var nouveauHote = fenetre.changementsServeur[compte.adresse]
            if (nouveauHote !== undefined) {
                delete fenetre.changementsServeur[compte.adresse]
                if (trouve) {
                    // Le secret suit le serveur du compte dès maintenant :
                    // l'ancien l'avait accepté, et une faute dans le nouveau
                    // se corrige alors sans le ressaisir.
                    coffre.ecrire(fenetre.cle(compte.adresse, nouveauHote), secret)
                    fenetre.effacerDiscretement(cle)
                    boite.connecter(nouveauHote, compte.adresse, secret)
                } else {
                    fenetre.demanderCompte({ id: compte.id, adresse: compte.adresse, hote: nouveauHote,
                                             hoteEnvoi: compte.hoteEnvoi },
                                           qsTr("Mot de passe absent du coffre : saisissez-le."), true)
                }
                return
            }
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
            if (fenetre.effacementsDiscrets[cle]) {
                delete fenetre.effacementsDiscrets[cle]
                return
            }
            messageEtat.texte = ok
                    ? qsTr("Mot de passe retiré du coffre du système.")
                    : qsTr("Retrait du coffre impossible : %1").arg(erreur)
        }
    }

    // Secrets à confier au coffre une fois la connexion acceptée par le
    // serveur : on n'y range jamais un mot de passe que le serveur a refusé.
    property var secretsAConserver: ({})
    // « Modifier le compte… » : nouveau serveur de lecture par adresse, en
    // attente du mot de passe du coffre.
    property var changementsServeur: ({})
    // Clés du coffre dont le retrait ne s'annonce pas.
    property var effacementsDiscrets: ({})
    // Dernier changement de serveur de lecture, jusqu'à la connexion.
    property var modificationEnCours: null
    // Comptes dont le mot de passe est à demander, l'un après l'autre.
    property var aDemander: []
    property var essai: null

    ReactionsNoyau {}

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

    function dessinerCorbeille(ctx, w, h, encre) {
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.lineWidth = Math.max(1.2, w * 0.11)
        ctx.lineJoin = "round"
        ctx.lineCap = "round"
        // Couvercle et poignée.
        ctx.beginPath()
        ctx.moveTo(w * 0.08, h * 0.2)
        ctx.lineTo(w * 0.92, h * 0.2)
        ctx.moveTo(w * 0.36, h * 0.2)
        ctx.lineTo(w * 0.4, h * 0.07)
        ctx.lineTo(w * 0.6, h * 0.07)
        ctx.lineTo(w * 0.64, h * 0.2)
        ctx.stroke()
        // Cuve, et ses deux rainures.
        ctx.beginPath()
        ctx.moveTo(w * 0.18, h * 0.3)
        ctx.lineTo(w * 0.26, h * 0.94)
        ctx.lineTo(w * 0.74, h * 0.94)
        ctx.lineTo(w * 0.82, h * 0.3)
        ctx.moveTo(w * 0.42, h * 0.42)
        ctx.lineTo(w * 0.43, h * 0.8)
        ctx.moveTo(w * 0.58, h * 0.42)
        ctx.lineTo(w * 0.57, h * 0.8)
        ctx.stroke()
    }

    /// Bouton corbeille d'une ligne : ce message seul part à la corbeille,
    /// sans être ouvert ; le message affiché et la sélection ne bougent pas.
    /// Si c'est le message affiché, seul choisi, c'est « Supprimer » : le
    /// suivant prend sa place.
    function supprimerLigne(index) {
        if (index < 0 || index >= modeleMessages.count)
            return
        var r = modeleMessages.get(index)
        var compte = listeMulti ? r.compte : boite.compteCourant
        var chemin = listeMulti ? r.chemin : boite.dossierCourant
        var cle = compte + "|" + chemin + "|" + r.uid
        var seulChoisi = listeMulti ? cleChoisie === cle
                                    : selection[r.uid] === true && uidsChoisis().length === 1
        if (seulChoisi && compte === boite.compteCourant && chemin === boite.dossierCourant) {
            supprimerSelection()
            return
        }
        if (!boite.supprimerDans(compte, chemin, String(r.uid))) {
            messageEtat.texte = boite.erreur
            return
        }
        var retires = {}
        for (var u in enDeplacement)
            retires[u] = true
        retires[cle] = true
        enDeplacement = retires
        deplacementsEnVol += 1
        modeleMessages.remove(index)
        if (!listeMulti && selection[r.uid] === true) {
            var reste = {}
            for (var k in selection) {
                if (parseInt(k) !== r.uid)
                    reste[k] = true
            }
            selection = reste
        }
        // Le message affiché, choisi parmi d'autres : il ne reste pas à l'écran.
        if (!listeMulti && uidCourant === r.uid) {
            uidCourant = 0
            colonneLecture.sujetAffiche.text = ""
            colonneLecture.auteurAffiche.text = ""
            adresseAffichee = ""
            afficherTexte(qsTr("Aucun message sélectionné."))
        }
        messageEtat.texte = qsTr("Message mis à la corbeille.")
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
        return dlgCompte.visible || dlgConsentement.visible || dlgDeplacer.visible || dlgRetrait.visible
            || dlgAide.visible || dlgAPropos.visible || dlgLien.visible || dlgIdentite.visible
            || dlgNouveauDossier.visible
            // Sur téléphone, la rédaction couvre la fenêtre principale.
            || (compact && Object.keys(redactions).length > 0)
    }

    /// Ouvre le dialogue de compte : `compte` nul pour un nouveau compte ;
    /// `modification` : un compte présent, ses serveurs modifiables.
    function demanderCompte(compte, note, modification) {
        if (dlgCompte.visible) {
            if (compte && !modification)
                aDemander.push({ compte: compte, note: note })
            return
        }
        // Un compte Microsoft ou Google n'a pas de mot de passe à redemander :
        // la fenêtre propose de se reconnecter au service.
        if (compte && compte.id > 0 && compte.auth === undefined) {
            var connu = compteParId(compte.id)
            compte.auth = connu ? connu.auth : ""
        }
        if (compte && compteOAuth(compte.auth))
            modification = true
        dernierDemande = compte
        dlgCompte.compteId = compte && compte.id ? compte.id : 0
        dlgCompte.modification = modification === true && dlgCompte.compteId > 0
        dlgCompte.service = dlgCompte.modification && compteOAuth(compte.auth) ? compte.auth : ""
        dlgCompte.ancienHote = compte ? compte.hote : ""
        dlgCompte.ancienHoteEnvoi = compte && compte.hoteEnvoi ? compte.hoteEnvoi : ""
        dlgCompte.champAdresse.text = compte ? compte.adresse : ""
        dlgCompte.champHote.text = compte ? compte.hote : ""
        dlgCompte.champHoteEnvoi.text = compte && compte.hoteEnvoi ? compte.hoteEnvoi : ""
        dlgCompte.champMotDePasse.text = ""
        dlgCompte.caseMemoriser.checked = reglages.memoriser
        dlgCompte.noteCompte.text = note
        dlgCompte.hoteSaisi = dlgCompte.champHote.text.length > 0
        dlgCompte.infoServeur = ""
        dlgCompte.open()
    }

    /// « Modifier le compte… » : le compte tel que l'index le connaît.
    function modifierCompte(id) {
        var present = listeComptes().filter(function(c) { return c.id === id })[0]
        if (present)
            demanderCompte(present, "", true)
    }

    /// Configuration automatique : le serveur se déduit de l'adresse, tant que
    /// la personne ne l'a pas saisi elle-même.
    function chercherServeur() {
        var adresse = dlgCompte.champAdresse.text.trim()
        if (dlgCompte.compteId > 0 || dlgCompte.hoteSaisi || adresse.indexOf("@") < 1)
            return
        dlgCompte.infoServeur = qsTr("Recherche du serveur…")
        configuration.decouvrir(adresse)
    }

    function serveurDecouvert(adresse, hote) {
        // Réponse tardive : l'adresse a changé, ou le serveur a été saisi.
        if (!dlgCompte.visible || dlgCompte.hoteSaisi || dlgCompte.champAdresse.text.trim() !== adresse)
            return
        if (hote.length > 0) {
            dlgCompte.champHote.text = hote
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
    /// `{"mmail": 1, "comptes": [{"adresse", "hote", "motDePasse", "hoteEnvoi"?}]}`.
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
                    && (c.hoteEnvoi === undefined || typeof c.hoteEnvoi === "string")
            if (!valide) {
                ecartes.push(c && typeof c.adresse === "string" ? c.adresse : "?")
                continue
            }
            retenus.push({ adresse: c.adresse.trim(), hote: c.hote.trim().toLowerCase(),
                           hoteEnvoi: c.hoteEnvoi === undefined ? null : c.hoteEnvoi.trim().toLowerCase(),
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
            // Absent du document : le serveur d'envoi d'un compte déjà connu
            // reste tel qu'il est.
            if (boite.connecter(r.hote, r.adresse, r.motDePasse) && r.hoteEnvoi !== null)
                boite.poserServeurEnvoi(r.adresse, r.hoteEnvoi)
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
        var adresse = dlgCompte.champAdresse.text.trim()
        var hote = dlgCompte.champHote.text.trim()
        var hoteEnvoi = dlgCompte.champHoteEnvoi.text.trim()
        var nouveau = dlgCompte.compteId === 0
        var motDePasse = dlgCompte.champMotDePasse.text
        dlgCompte.champMotDePasse.text = ""
        if (dlgCompte.oauth) {
            // Rien à appliquer ici : la reconnexion passe par son bouton.
            demandeSuivante()
            return
        }
        reglages.memoriser = dlgCompte.caseMemoriser.checked
        if (dlgCompte.modification) {
            appliquerModification(dlgCompte.compteId, adresse, dlgCompte.ancienHote, hote.toLowerCase(),
                                  dlgCompte.ancienHoteEnvoi, hoteEnvoi.toLowerCase(), motDePasse)
            demandeSuivante()
            return
        }
        dernierDemande = { id: dlgCompte.compteId, adresse: adresse, hote: hote }
        if (dlgCompte.caseMemoriser.checked)
            secretsAConserver[adresse] = { cle: cle(adresse, hote), motDePasse: motDePasse }
        else
            coffre.effacer(cle(adresse, hote))
        messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
        // Le compte est enregistré dès l'appel : son serveur d'envoi suit.
        if (boite.connecter(hote, adresse, motDePasse) && nouveau)
            boite.poserServeurEnvoi(adresse, hoteEnvoi)
        demandeSuivante()
    }

    /// « Modifier le compte… ». Le compte se reconnaît à son adresse : le
    /// connecter à un autre serveur de lecture met l'index à jour. Le mot de
    /// passe du coffre est rangé sous l'adresse et le serveur — il change donc
    /// de clé avec le serveur, une fois la connexion acceptée.
    function appliquerModification(id, adresse, ancienHote, hote, ancienEnvoi, hoteEnvoi, motDePasse) {
        if (hote.length === 0) {
            demanderCompte({ id: id, adresse: adresse, hote: ancienHote, hoteEnvoi: ancienEnvoi },
                           qsTr("Le serveur de lecture est obligatoire."), true)
            return
        }
        if (hoteEnvoi !== ancienEnvoi) {
            if (!boite.poserServeurEnvoi(adresse, hoteEnvoi)) {
                messageEtat.texte = qsTr("Serveur d'envoi refusé : %1").arg(boite.erreur)
                return
            }
            messageEtat.texte = hoteEnvoi.length > 0
                    ? qsTr("Envoi de %1 par %2.").arg(adresse).arg(hoteEnvoi)
                    : qsTr("Envoi de %1 par son serveur de lecture.").arg(adresse)
        }
        var ancienne = cle(adresse, ancienHote)
        var nouvelle = cle(adresse, hote)
        // Un serveur injoignable ou mal écrit rouvre cette fenêtre (cf. onEchec).
        modificationEnCours = hote !== ancienHote
                ? { id: id, adresse: adresse, hote: hote, hoteEnvoi: hoteEnvoi } : null
        if (motDePasse.length > 0) {
            if (reglages.memoriser) {
                secretsAConserver[adresse] = { cle: nouvelle, motDePasse: motDePasse,
                                               ancienne: ancienne !== nouvelle ? ancienne : "" }
            } else {
                effacerDiscretement(ancienne)
                if (nouvelle !== ancienne)
                    effacerDiscretement(nouvelle)
            }
            messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
            boite.connecter(hote, adresse, motDePasse)
            return
        }
        if (hote === ancienHote) {
            // « Mémoriser » décoché : le mot de passe quitte le coffre.
            if (!reglages.memoriser)
                coffre.effacer(ancienne)
            return
        }
        // Nouveau serveur de lecture sans mot de passe saisi : celui du coffre
        // (cf. Coffre.onLu), à défaut il est demandé.
        if (reglages.memoriser) {
            changementsServeur[adresse] = hote
            messageEtat.texte = qsTr("Connexion de %1 à %2…").arg(adresse).arg(hote)
            coffre.lire(ancienne)
        } else {
            demanderCompte({ id: id, adresse: adresse, hote: hote, hoteEnvoi: hoteEnvoi },
                           qsTr("Nouveau serveur de lecture : le mot de passe est nécessaire."), true)
        }
    }

    /// Retrait du coffre sans message : celui d'un changement de serveur.
    function effacerDiscretement(c) {
        effacementsDiscrets[c] = true
        coffre.effacer(c)
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
            if (c.auth === "microsoft") {
                demanderCompte({ id: p ? p.id : 0, adresse: c.adresse, hote: hote, auth: "microsoft" },
                               qsTr("Compte Microsoft : reconnectez-le à Microsoft."))
            } else if (c.auth === "google") {
                demanderCompte({ id: p ? p.id : 0, adresse: c.adresse, hote: hote, auth: "google" },
                               qsTr("Compte Google : reconnectez-le à Google."))
            } else if (typeof c.motDePasse === "string" && c.motDePasse.length > 0) {
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
            bilan.triggered.connect(scenarios.bilanScenarioRestauration)
            bilan.start()
        }
    }

    function connecterCompte(compte) {
        var connu = compte.id > 0 ? compteParId(compte.id) : null
        // Un compte Microsoft ou Google ne vit que par son jeton au coffre.
        if (reglages.memoriser || (connu && compteOAuth(connu.auth)))
            coffre.lire(cle(compte.adresse, compte.hote))
        else
            demanderCompte(compte, "")
    }

    /// Compte à jetons OAuth2 : Microsoft ou Google, pas de mot de passe.
    function compteOAuth(auth) {
        return auth === "microsoft" || auth === "google"
    }

    /// Connexion d'un compte Google (Gmail) : comme `connecterMicrosoft` ;
    /// la suite revient par `googleConnecte` ou `googleRefuse`.
    function connecterGoogle(indice) {
        if (!boite.googleDisponible()) {
            messageEtat.texte = qsTr("Comptes Google indisponibles dans cette version de MMail.")
            return
        }
        var url = boite.connecterGoogle(indice)
        if (url.length === 0) {
            messageEtat.texte = qsTr("Connexion à Google impossible : %1").arg(boite.erreur)
            return
        }
        Qt.openUrlExternally(url)
        messageEtat.texte = qsTr("Connexion à Google dans votre navigateur : MMail attend son retour…")
    }

    /// Ajout d'un compte Microsoft : l'adresse d'abord — elle mène à la
    /// connexion de son organisation, ou à celle des comptes personnels pour
    /// Outlook.com, sans passer par un compte déjà connecté dans le navigateur
    /// (retour de Manu du 18/10).
    function ajouterMicrosoft(adresse) {
        if (adresse.indexOf("@") > 0) {
            connecterMicrosoft(adresse)
            return
        }
        dlgAdresseMicrosoft.open()
    }
    Dialog {
        id: dlgAdresseMicrosoft
        title: qsTr("Compte Microsoft")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: {
            champAdresseMicrosoft.text = ""
            champAdresseMicrosoft.forceActiveFocus()
        }
        onAccepted: {
            if (champAdresseMicrosoft.text.indexOf("@") > 0)
                fenetre.connecterMicrosoft(champAdresseMicrosoft.text.trim())
        }
        ColumnLayout {
            width: parent.width
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Adresse du compte (Office 365 de l'entreprise, ou Outlook.com) :")
            }
            TextField {
                id: champAdresseMicrosoft
                Layout.fillWidth: true
                placeholderText: qsTr("prenom.nom@entreprise.fr")
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                onAccepted: dlgAdresseMicrosoft.accept()
            }
        }
    }

    /// Connexion d'un compte Microsoft : la page de Microsoft s'ouvre dans le
    /// navigateur ; la suite revient par `microsoftConnecte` ou
    /// `microsoftRefuse`. `indice` : l'adresse d'un compte à reconnecter.
    function connecterMicrosoft(indice) {
        if (!boite.microsoftDisponible()) {
            messageEtat.texte = qsTr("Comptes Microsoft indisponibles dans cette version de MMail.")
            return
        }
        var url = boite.connecterMicrosoft(indice)
        if (url.length === 0) {
            messageEtat.texte = qsTr("Connexion à Microsoft impossible : %1").arg(boite.erreur)
            return
        }
        // Sous Android, dans la page intégrée : le navigateur du système
        // laisserait MMail gelé en arrière-plan (retour de Manu du 18/10).
        if (chargeurConnexion.item) {
            chargeurConnexion.item.ouvrir(url)
            messageEtat.texte = qsTr("Connexion à Microsoft…")
            return
        }
        Qt.openUrlExternally(url)
        messageEtat.texte = qsTr("Connexion à Microsoft dans votre navigateur : MMail attend son retour…")
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
        tiroir.close()
        // Un dossier choisi dans l'arborescence met fin à une recherche dans
        // toutes les boîtes, même s'il est celui du dernier résultat lu.
        var sortieGlobale = rechercheGlobale || vueUnifiee
        if (sortieGlobale) {
            sortirRechercheGlobale()
            vueUnifiee = false
            unifieeAvantRecherche = false
            colonneListe.champRecherche.text = ""
        }
        if (sortieGlobale || compte !== boite.compteCourant || chemin !== boite.dossierCourant) {
            recherche = ""
            uidsServeur = []
            if (!colonneListe.porteeRecherche.partout)
                colonneListe.champRecherche.text = ""
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
        colonneLecture.sujetAffiche.text = ""
        colonneLecture.auteurAffiche.text = ""
        adresseAffichee = ""
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
        var enHaut = listeEnHaut()
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
            if (fenetre.masquee(cleDeplacement(n.uid)) || rangExact(n.h, n.uid) >= 0)
                continue
            modeleMessages.insert(rangInsertion(n.h, n.uid), n)
        }
        if (enHaut)
            colonneListe.vueMessages.positionViewAtBeginning()
        oublierAbsents(absents)
        majInfoDossier()
    }

    /// Vrai si la liste est tout en haut. Des messages qui arrivent au-dessus
    /// doivent alors se voir : la vue garde sinon à l'écran les lignes déjà
    /// affichées, et les nouvelles se glissent au-dessus (constaté le 15/10, à
    /// la première synchronisation d'une boîte Gmail de 1 300 messages).
    function listeEnHaut() {
        return modeleMessages.count === 0 || colonneListe.vueMessages.contentY <= colonneListe.vueMessages.originY + 1
    }

    /// Pose une liste entière, en gardant la position et ce qui est choisi.
    /// `suite` : ce n'est que la première page d'un gros dossier, le reste
    /// arrive par paquets (cf. `chargerSuite`).
    function poserListe(liste, suite) {
        arreterSuite()
        var messages = liste.filter(function(m) {
            return !fenetre.masquee(cleDeplacement(m.uid))
        })
        var enHaut = listeEnHaut()
        var position = colonneListe.vueMessages.contentY
        var rangHaut = Math.max(0, colonneListe.vueMessages.indexAt(1, position + 1))
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
        if (enHaut)
            colonneListe.vueMessages.positionViewAtBeginning()
        else
            colonneListe.vueMessages.contentY = position
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
            return !fenetre.masquee(cleDeplacement(m.uid))
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
            colonneLecture.sujetAffiche.text = ""
            colonneLecture.auteurAffiche.text = ""
            adresseAffichee = ""
            afficherTexte(qsTr("Aucun message sélectionné."))
        }
    }

    // ---- recherche

    /// Lance la recherche saisie, ou `texteImpose` (une recherche passée, choisie
    /// dans la liste). Dans le dossier ouvert comme dans toutes les boîtes, la
    /// liste ne contient plus que les résultats.
    function lancerRecherche(texteImpose) {
        var texte = (texteImpose !== undefined ? texteImpose : colonneListe.champRecherche.text).trim()
        colonneListe.panneauHistorique.close()
        if (texte.length === 0) {
            quitterRecherche()
            return
        }
        colonneListe.champRecherche.text = texte
        noterRecherche(texte)
        if (vueUnifiee) {
            vueUnifiee = false
            unifieeAvantRecherche = true
            cleChoisie = ""
            viderListe()
        }
        if (colonneListe.porteeRecherche.partout) {
            if (!rechercheGlobale)
                dossierAvantRecherche = boite.dossierCourant.length > 0
                        ? { compte: boite.compteCourant, chemin: boite.dossierCourant } : null
            rechercheGlobale = true
            recherche = texte
            uidsServeur = []
            cleChoisie = ""
            arreterRecherchePartout()
            viderListe()
            poserResultats()
            // L'index répond tout de suite ; le serveur suit, dossier par
            // dossier, dans le texte des messages et jusque dans les dossiers
            // jamais ouverts sur ce poste.
            dossiersPartout = boite.chercherPartoutSurLeServeur(texte)
            messageEtat.texte = etatRecherchePartout()
            return
        }
        // De toutes les boîtes au dossier actif : celui d'où l'on était parti.
        if (rechercheGlobale) {
            var avant = dossierAvantRecherche
            sortirRechercheGlobale()
            if (avant)
                boite.ouvrirDossier(avant.compte, avant.chemin)
            colonneListe.champRecherche.text = texte
        }
        if (boite.dossierCourant.length === 0)
            return
        recherche = texte
        uidsServeur = []
        poserResultats()
        if (!boite.chercherSurLeServeur(texte))
            messageEtat.texte = qsTr("Recherche dans l'index seulement (compte hors ligne).")
    }

    /// Résultats de l'index et ceux que le serveur a trouvés dans le texte des
    /// messages — du dossier ouvert, ou de chaque dossier de chaque boîte —,
    /// du plus récent au plus ancien.
    function poserResultats() {
        if (rechercheGlobale) {
            var trouves = JSON.parse(boite.chercher(recherche, true))
            var connus = {}
            for (var j = 0; j < trouves.length; ++j)
                connus[trouves[j].compte + "\n" + trouves[j].chemin + "\n" + trouves[j].uid] = true
            for (var cle in trouvesPartout) {
                var absents = trouvesPartout[cle].filter(function(u) { return !connus[cle + "\n" + u] })
                if (absents.length === 0)
                    continue
                var coupure = cle.indexOf("\n")
                trouves = trouves.concat(JSON.parse(boite.trouvesParUid(Number(cle.slice(0, coupure)),
                                                                         cle.slice(coupure + 1),
                                                                         absents.join(","))))
            }
            // Les plus récents d'abord pour la coupure, puis le tri choisi.
            trouves.sort(function(a, b) { return b.h - a.h || b.uid - a.uid })
            resultatsTronques = trouves.length > resultatsMax
            poserListe(trierResultats(trouves.slice(0, resultatsMax).map(function(r) {
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
        colonneListe.vueMessages.positionViewAtBeginning()
    }

    function sortirRechercheGlobale() {
        rechercheGlobale = false
        cleChoisie = ""
        dossierAvantRecherche = null
        recherche = ""
        uidsServeur = []
        arreterRecherchePartout()
        viderListe()
    }

    /// Arrête la recherche sur le serveur de toutes les boîtes, si elle court
    /// encore : les dossiers qui restaient ne sont pas interrogés.
    function arreterRecherchePartout() {
        if (dossiersFaits < dossiersPartout)
            boite.chercherPartoutSurLeServeur("")
        trouvesPartout = ({})
        dossiersPartout = 0
        dossiersFaits = 0
        resultatsTronques = false
    }

    /// État d'une recherche dans toutes les boîtes, pour la barre d'état.
    function etatRecherchePartout() {
        var nombre = accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats"))
        if (resultatsTronques)
            nombre = qsTr("les %1 plus récents").arg(modeleMessages.count)
        var debut = qsTr("Recherche « %1 » dans toutes les boîtes : %2").arg(recherche).arg(nombre)
        if (dossiersPartout === 0)
            return debut + qsTr(" (messages déjà sur le poste seulement : aucun compte connecté).")
        if (dossiersFaits < dossiersPartout)
            return debut + qsTr(" — recherche sur le serveur, dossier %1 sur %2…").arg(dossiersFaits + 1).arg(dossiersPartout)
        return debut + "."
    }

    /// Sort de la recherche : la liste du dossier revient entière — celui d'où
    /// l'on était parti, après une recherche dans toutes les boîtes.
    function quitterRecherche() {
        colonneListe.champRecherche.text = ""
        colonneListe.panneauHistorique.close()
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
        tiroir.close()
        if (rechercheGlobale)
            sortirRechercheGlobale()
        recherche = ""
        uidsServeur = []
        colonneListe.champRecherche.text = ""
        unifieeAvantRecherche = false
        cleChoisie = ""
        vueUnifiee = true
        viderListe()
        poserBoitesUnifiees()
        if (compact)
            vue = 1
    }

    /// Pose ou met à jour la boîte unifiée, ligne à ligne : elle se relit à
    /// chaque changement d'un compte — un message marqué lu au clic suffit —,
    /// et la vider pour la remplir ramenait la liste en haut quand on était
    /// descendu (constaté le 14/10). Les lignes restées en place gardent la
    /// leur, et la vue sa position.
    function poserBoitesUnifiees() {
        arreterSuite()
        var lignes = JSON.parse(boite.boitesDeReception()).map(function(r) {
            r.cheminAffiche = cheminResultat(r)
            delete r.segments
            return r
        }).filter(function(r) {
            return !fenetre.masquee(r.compte + "|" + r.chemin + "|" + r.uid)
        })
        var cle = function(m) { return fenetre.cleResultat(m.compte, m.chemin, m.uid) }
        var voulues = {}
        for (var i = 0; i < lignes.length; ++i)
            voulues[cle(lignes[i])] = true
        var enHaut = listeEnHaut()
        if (modeleMessages.count === 0) {
            modeleMessages.append(lignes)
        } else {
            var dansLaListe = {}
            for (var k = modeleMessages.count - 1; k >= 0; --k) {
                var c = cle(modeleMessages.get(k))
                if (voulues[c])
                    dansLaListe[c] = true
                else
                    modeleMessages.remove(k)
            }
            // Le noyau rend les lignes dans l'ordre de la liste : chacune est
            // à son rang, à mettre à jour, à insérer, ou plus bas — deux
            // messages de même date peuvent changer d'ordre d'une lecture à
            // l'autre.
            for (var j = 0; j < lignes.length; ++j) {
                var l = lignes[j]
                var cl = cle(l)
                if (j < modeleMessages.count && cle(modeleMessages.get(j)) === cl) {
                    modeleMessages.set(j, l)
                } else if (!dansLaListe[cl]) {
                    modeleMessages.insert(j, l)
                } else {
                    for (var a = j + 1; a < modeleMessages.count; ++a) {
                        if (cle(modeleMessages.get(a)) === cl) {
                            modeleMessages.move(a, j, 1)
                            break
                        }
                    }
                    modeleMessages.set(j, l)
                }
            }
        }
        if (enHaut)
            colonneListe.vueMessages.positionViewAtBeginning()
        // Même règle que poserListe : la sélection ne garde que les messages
        // encore là.
        var presents = {}
        for (var p = 0; p < lignes.length; ++p)
            presents[lignes[p].uid] = true
        var absents = {}
        for (var uid in selection)
            if (!presents[uid])
                absents[uid] = true
        if (uidCourant > 0 && !presents[uidCourant])
            absents[uidCourant] = true
        oublierAbsents(absents)
        majInfoDossier()
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
        var filtre = colonneListe.champRecherche.text.trim().toLowerCase()
        var h = historiqueRecherches().filter(function(t) {
            return filtre.length === 0 || t.toLowerCase().indexOf(filtre) >= 0
        })
        colonneListe.panneauHistorique.entrees = h
        if (h.length > 0 && colonneListe.champRecherche.activeFocus)
            colonneListe.panneauHistorique.open()
        else
            colonneListe.panneauHistorique.close()
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
        colonneListe.vueMessages.currentIndex = index
        colonneListe.vueMessages.forceActiveFocus()
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
        colonneListe.vueMessages.currentIndex = index
        colonneListe.vueMessages.forceActiveFocus()
        if (r.compte !== boite.compteCourant || r.chemin !== boite.dossierCourant)
            boite.ouvrirDossier(r.compte, r.chemin)
        afficherMessage(r.uid, index)
    }

    function deplacerCurseur(pas, etendre) {
        var index = Math.max(0, Math.min(modeleMessages.count - 1, colonneListe.vueMessages.currentIndex + pas))
        choisir(index, etendre ? Qt.ShiftModifier : 0)
        colonneListe.vueMessages.positionViewAtIndex(index, ListView.Contain)
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
            colonneLecture.sujetAffiche.text = ligne.sujet
            importanceCourante = ligne.importance
            colonneLecture.auteurAffiche.text = ligne.expediteur
                    + (ligne.adresse.length > 0 && ligne.adresse !== ligne.expediteur
                       ? " <" + ligne.adresse + ">" : "")
                    + "  ·  " + dateLongue(ligne.date)
            adresseAffichee = ligne.adresse
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
        return colonneLecture.sujetAffiche.text
    }

    /// Texte simple dans le volet du message : attente, absence, erreur.
    function afficherTexte(texte) {
        corpsHtml = false
        htmlAffiche = ""
        imagesBloquees = 0
        lienSurvole = ""
        colonneLecture.vueCorps.text = texte
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
        var position = colonneLecture.cadreCorps.ScrollBar.vertical.position
        colonneLecture.vueCorps.text = htmlAuZoom(htmlAffiche)
        bornerImages()
        colonneLecture.cadreCorps.ScrollBar.vertical.position = position
    }

    /// Les images du message HTML affiché, bornées à la largeur de la colonne.
    function bornerImages() {
        if (corpsHtml && !sourceVisible)
            colonneLecture.mefLecture.bornerImages(Math.floor(colonneLecture.vueCorps.width - colonneLecture.vueCorps.leftPadding - colonneLecture.vueCorps.rightPadding) - 2)
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

    // Ligne retirée de la liste : en cours de déplacement, ou glissée et en
    // attente derrière le bandeau « Annuler ».
    function masquee(cle) {
        return enDeplacement[cle] === true || enAttente[cle] === true
    }

    // Glisser d'une ligne (vue tactile) : la ligne quitte la liste d'emblée,
    // l'action attend 5 s derrière le bandeau « Annuler », puis part comme
    // celle de la corbeille de la ligne. Une seule action attend : la suivante
    // envoie la précédente.
    property var glissement: null
    property var enAttente: ({})
    function glisserLigne(index, genre) {
        if (index < 0 || index >= modeleMessages.count)
            return
        validerGlissement()
        var r = modeleMessages.get(index)
        var compte = listeMulti ? r.compte : boite.compteCourant
        var chemin = listeMulti ? r.chemin : boite.dossierCourant
        var role = listeMulti ? (r.role || "") : roleCourant
        var cle = compte + "|" + chemin + "|" + r.uid
        glissement = { cle: cle, compte: compte, chemin: chemin, uid: r.uid, genre: genre }
        var attente = {}
        for (var k in enAttente)
            attente[k] = true
        attente[cle] = true
        enAttente = attente
        modeleMessages.remove(index)
        if (!listeMulti && uidCourant === r.uid) {
            uidCourant = 0
            colonneLecture.sujetAffiche.text = ""
            colonneLecture.auteurAffiche.text = ""
            adresseAffichee = ""
            afficherTexte(qsTr("Aucun message sélectionné."))
        }
        bandeauGlissement.texte = genre === "corbeille" ? qsTr("Message mis à la corbeille.")
                                 : role === "Junk" ? qsTr("Message remis en boîte de réception.")
                                 : qsTr("Message classé indésirable.")
        bandeauGlissement.open()
        minuterieGlissement.restart()
    }
    function retirerDeLAttente(cle) {
        var attente = {}
        for (var k in enAttente) {
            if (k !== cle)
                attente[k] = true
        }
        enAttente = attente
    }
    function validerGlissement() {
        if (!glissement)
            return
        minuterieGlissement.stop()
        bandeauGlissement.close()
        var g = glissement
        glissement = null
        var ok = g.genre === "corbeille" ? boite.supprimerDans(g.compte, g.chemin, String(g.uid))
                                         : boite.signalerIndesirableDans(g.compte, g.chemin, String(g.uid))
        retirerDeLAttente(g.cle)
        if (ok) {
            var retires = {}
            for (var u in enDeplacement)
                retires[u] = true
            retires[g.cle] = true
            enDeplacement = retires
            deplacementsEnVol += 1
        } else {
            messageEtat.texte = boite.erreur
            rafraichirListe(true)
        }
    }
    function annulerGlissement() {
        if (!glissement)
            return
        minuterieGlissement.stop()
        bandeauGlissement.close()
        var g = glissement
        glissement = null
        retirerDeLAttente(g.cle)
        rafraichirListe(true)
    }
    Timer {
        id: minuterieGlissement
        interval: 5000
        onTriggered: fenetre.validerGlissement()
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
        colonneLecture.sujetAffiche.text = ""
        colonneLecture.auteurAffiche.text = ""
        adresseAffichee = ""
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
        // Un type que Windows ne sait pas ouvrir : il demanderait lui-même
        // quel logiciel prendre, et ce choix échappait à MMail (constaté le
        // 14/10). MMail le fait donc choisir, « Toujours… » coché d'avance.
        if (!programme && !essai && Qt.platform.os === "windows"
                && logicielPour(piece.nom).length === 0 && !lanceur.associe(extensionDe(piece.nom))) {
            messageEtat.texte = qsTr("Aucun logiciel n'ouvre les fichiers « %1 » sur ce poste : choisissez-en un.")
                                .arg(piece.nom)
            dlgChoisirLogiciel.choisir(piece, true)
            return
        }
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

    /// Dossier proposé à l'enregistrement d'une pièce jointe.
    function dossierEnregistrement() {
        var dernier = reglagesPieces.dernierDossier
        return dernier.length > 0 && boite.dossierExiste(dernier) ? dernier : dossierTelechargements
    }

    function enregistrerPiece(piece) {
        if (!piece)
            return
        dlgEnregistrerPiece.piece = piece
        dlgEnregistrerPiece.uid = uidCourant
        // Le dernier dossier choisi, s'il existe encore ; sinon les
        // téléchargements de l'utilisateur.
        var dossier = dossierEnregistrement()
        dlgEnregistrerPiece.currentFolder = dossier
        dlgEnregistrerPiece.selectedFile = dossier + "/" + encodeURIComponent(piece.nom)
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
        // Vue tactile : le retour arrière d'Android ferme le tiroir, ramène
        // du message à la liste, puis quitte — la liste est l'écran d'accueil.
        if (tactile) {
            validerGlissement()
            if (tiroir.opened) {
                fermeture.accepted = false
                tiroir.close()
            } else if (vue === 2) {
                fermeture.accepted = false
                vue = 1
            }
            return
        }
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
        if (identifiantsEssai.taille) {
            var taille = identifiantsEssai.taille.split("x")
            width = parseInt(taille[0])
            height = parseInt(taille[1])
        }
        // L'agenda relève, sous Android, de l'application du téléphone ; la
        // liste est l'écran d'accueil, le tiroir s'ouvre tant qu'aucun dossier
        // n'est choisi.
        if (tactile) {
            module = "courrier"
            vue = 1
            Qt.callLater(function() {
                if (boite.dossierCourant.length === 0 && !vueUnifiee)
                    tiroir.open()
            })
        }
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
                pause.triggered.connect(essai.scenario === "garde" ? scenarios.etapeScenarioGarde : scenarios.etapeScenarioReprise)
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
                delai.triggered.connect(scenarios.etapeScenarioGardeServeur)
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
            // Scénario « compte-microsoft » : sans serveur. Un compte Microsoft
            // de l'index s'ouvre en modification sans serveur ni mot de passe,
            // avec « Se reconnecter à Microsoft… » — aussi quand son mot de
            // passe serait redemandé ; la connexion prépare l'adresse de
            // Microsoft (MMAIL_MICROSOFT_CLIENT_ID) ; le refus de consentement
            // montre le lien de l'administrateur.
            if (essai.scenario === "compte-microsoft") {
                var attenteMs = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
                attenteMs.triggered.connect(function() {
                    var ms = listeComptes().filter(function(c) { return c.auth === "microsoft" })[0]
                    console.log("scenario: compte", ms ? ms.adresse : "aucun", "| auth", ms ? ms.auth : "")
                    modifierCompte(ms.id)
                    console.log("scenario: modification", dlgCompte.visible, dlgCompte.title, "| microsoft", dlgCompte.microsoft,
                                "| serveur", dlgCompte.champHote.visible, "| mot de passe", dlgCompte.champMotDePasse.visible,
                                "| reconnexion", dlgCompte.boutonReconnexion.visible)
                    dlgCompte.close()
                    demanderCompte(compteParId(ms.id), "jeton refusé")
                    console.log("scenario: redemande", dlgCompte.modification, dlgCompte.microsoft, "| note", dlgCompte.noteCompte.text)
                    dlgCompte.close()
                    console.log("scenario: disponible", boite.microsoftDisponible())
                    var url = boite.connecterMicrosoft(ms.adresse)
                    console.log("scenario: adresse", url.indexOf("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?") === 0,
                                "| indice", url.indexOf("login_hint=" + encodeURIComponent(ms.adresse)) > 0,
                                "| redirection", /redirect_uri=http%3A%2F%2Flocalhost%3A[0-9]+&/.test(url))
                    dlgConsentement.ouvrir("AADSTS65001: consentement requis")
                    console.log("scenario: consentement", dlgConsentement.visible, "|", dlgConsentement.champConsentement.text)
                    Qt.quit()
                })
                attenteMs.start()
            }
            // Scénario « compte-google » : sans serveur, comme
            // « compte-microsoft ». Un compte Google de l'index s'ouvre en
            // modification sans serveur ni mot de passe, avec « Se reconnecter
            // à Google… » — aussi quand son jeton serait redemandé ; la
            // connexion prépare l'adresse de Google (MMAIL_GOOGLE_CLIENT_ID et
            // MMAIL_GOOGLE_CLIENT_SECRET), sinon dit pourquoi elle ne peut pas.
            if (essai.scenario === "compte-google") {
                var attenteG = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
                attenteG.triggered.connect(function() {
                    var g = listeComptes().filter(function(c) { return c.auth === "google" })[0]
                    console.log("scenario: compte", g ? g.adresse : "aucun", "| auth", g ? g.auth : "")
                    modifierCompte(g.id)
                    console.log("scenario: modification", dlgCompte.visible, dlgCompte.title, "| service", dlgCompte.service,
                                "| serveur", dlgCompte.champHote.visible, "| mot de passe", dlgCompte.champMotDePasse.visible,
                                "| reconnexion", dlgCompte.boutonReconnexion.visible, "«" + dlgCompte.boutonReconnexion.text + "»")
                    dlgCompte.close()
                    demanderCompte(compteParId(g.id), "jeton refusé")
                    console.log("scenario: redemande", dlgCompte.modification, dlgCompte.service, "| note", dlgCompte.noteCompte.text)
                    dlgCompte.close()
                    console.log("scenario: disponible", boite.googleDisponible(), "| ajout ouvert", boite.googleAjoutOuvert())
                    var url = boite.connecterGoogle(g.adresse)
                    if (url.length === 0) {
                        console.log("scenario: sans client :", boite.erreur)
                    } else {
                        console.log("scenario: adresse", url.indexOf("https://accounts.google.com/o/oauth2/v2/auth?") === 0,
                                    "| indice", url.indexOf("login_hint=" + encodeURIComponent(g.adresse)) > 0,
                                    "| redirection", /redirect_uri=http%3A%2F%2F127\.0\.0\.1%3A[0-9]+&/.test(url),
                                    "| hors ligne", url.indexOf("access_type=offline") > 0)
                    }
                    Qt.quit()
                })
                attenteG.start()
            }
            // Scénario « modifier-compte » : « Modifier le compte… » pose puis
            // retire un serveur d'envoi, mène le compte à un serveur de lecture
            // injoignable — la fenêtre revient, avec le motif —, puis le ramène.
            if (essai.scenario === "modifier-compte") {
                var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
                var suivi = { n: 0, id: 0, hote: "" }
                var present = function() {
                    return listeComptes().filter(function(c) { return c.id === suivi.id })[0] || {}
                }
                var valider = function(hote, envoi, mdp) {
                    dlgCompte.champHote.text = hote
                    dlgCompte.champHoteEnvoi.text = envoi
                    dlgCompte.champMotDePasse.text = mdp
                    dlgCompte.accept()
                }
                tic.triggered.connect(function() {
                    suivi.n += 1
                    if (suivi.n === 12) {
                        suivi.id = comptesConnus[0].compte
                        suivi.hote = comptesConnus[0].hote
                        modifierCompte(suivi.id)
                        console.log("scenario: fenêtre", dlgCompte.visible, dlgCompte.title, "| serveur", dlgCompte.champHote.text,
                                    "| adresse modifiable", dlgCompte.champAdresse.enabled, "| serveur modifiable", dlgCompte.champHote.enabled)
                        valider(suivi.hote, "smtp.essai.invalid", "")
                        console.log("scenario: envoi posé", present().hoteEnvoi, "| état", messageEtat.texte)
                        modifierCompte(suivi.id)
                        valider(suivi.hote, "", "")
                        console.log("scenario: envoi retiré", JSON.stringify(present().hoteEnvoi), "| état", messageEtat.texte)
                        modifierCompte(suivi.id)
                        valider("hors-ligne.invalid", "", identifiantsEssai.motDePasse)
                        console.log("scenario: lecture vers hors-ligne.invalid | index", present().hote)
                    } else if (suivi.n === 20) {
                        console.log("scenario: fenêtre revenue", dlgCompte.visible, "| modification", dlgCompte.modification,
                                    "| serveur", dlgCompte.champHote.text, "| motif", dlgCompte.noteCompte.text)
                        valider(suivi.hote, "", identifiantsEssai.motDePasse)
                    } else if (suivi.n === 30) {
                        tic.stop()
                        console.log("scenario: retour | index", present().hote, "| état", present().etat,
                                    "| fenêtre", dlgCompte.visible, "|", messageEtat.texte)
                        Qt.quit()
                    }
                })
                tic.start()
            }
            // Scénario « unifiee-position » : la boîte unifiée descendue en bas,
            // le dernier message choisi — au téléphone, la liste cède la place
            // au message —, marqué lu par le serveur, puis non lu et lu : autant
            // de relectures de la liste. Revenue à l'écran, elle doit être où
            // elle était.
            if (essai.scenario === "unifiee-position") {
                var pas = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
                var etat = { n: 0, avant: 0, bas: 0, uid: "" }
                pas.triggered.connect(function() {
                    etat.n += 1
                    if (etat.n === 12) {
                        ouvrirBoitesUnifiees()
                    } else if (etat.n === 13) {
                        colonneListe.vueMessages.positionViewAtEnd()
                    } else if (etat.n === 14) {
                        etat.avant = colonneListe.vueMessages.contentY
                        etat.bas = modeleMessages.count - 1
                        etat.uid = String(modeleMessages.get(etat.bas).uid)
                        choisir(etat.bas, 0)
                    } else if (etat.n === 19) {
                        boite.marquerLu(etat.uid, false)
                    } else if (etat.n === 20) {
                        boite.marquerLu(etat.uid, true)
                    } else if (etat.n === 22) {
                        if (compact)
                            vue = 1
                    } else if (etat.n === 23) {
                        pas.stop()
                        console.log("scenario: unifiée", modeleMessages.count, "messages | compacte", compact,
                                    "| position avant", Math.round(etat.avant), "après", Math.round(colonneListe.vueMessages.contentY),
                                    "| ligne choisie", colonneListe.vueMessages.currentIndex, "sur", etat.bas)
                        Qt.quit()
                    }
                })
                pas.start()
            }
            // Scénario « arrivee-en-haut » : boîte de réception ouverte, tout en
            // haut, le premier message choisi ; un message déposé de l'extérieur
            // doit arriver en tête ET se voir (essai du 15/10 sous Gmail : il se
            // glissait au-dessus de la partie visible).
            if (essai.scenario === "arrivee-en-haut") {
                var veille = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
                // MMAIL_DOSSIER : nombre de messages attendus (une rafale).
                var suiviA = { n: 0, avant: -1, attendus: Math.max(1, parseInt(identifiantsEssai.dossier) || 1) }
                veille.triggered.connect(function() {
                    suiviA.n += 1
                    if (suiviA.n === 10 && boite.dossierCourant === "INBOX") {
                        choisir(0, 0)
                        suiviA.avant = modeleMessages.count
                        console.log("scenario: pret", modeleMessages.count, "messages, haut", colonneListe.vueMessages.contentY, colonneListe.vueMessages.originY)
                    } else if (suiviA.avant >= 0 && modeleMessages.count >= suiviA.avant + suiviA.attendus) {
                        veille.stop()
                        var haut = colonneListe.vueMessages.indexAt(1, colonneListe.vueMessages.contentY + 1)
                        console.log("scenario: arrive", modeleMessages.get(0).sujet, "| premiere ligne visible", haut,
                                    "| contentY", colonneListe.vueMessages.contentY, "originY", colonneListe.vueMessages.originY)
                        console.log(haut === 0 ? "scenario: OK nouveau message visible en tete"
                                               : "scenario: ECHEC nouveau message hors de la vue")
                        Qt.quit()
                    } else if (suiviA.n > 120) {
                        veille.stop()
                        console.log("scenario: ECHEC rien n'est arrive")
                        Qt.quit()
                    }
                })
                veille.start()
            }
            // Scénario « corbeille-unifiee » : la corbeille d'une ligne dans
            // Toutes les BàL — une autre ligne que celle choisie. Le message
            // part, ne revient pas à la relecture de la liste, la ligne choisie
            // reste ; il est ensuite ramené de la corbeille.
            if (essai.scenario === "corbeille-unifiee") {
                var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
                var suiviU = { n: 0, cle: "", sujet: "" }
                tic.triggered.connect(function() {
                    suiviU.n += 1
                    var a = listeComptes()[0]
                    if (suiviU.n === 12) {
                        ouvrirBoitesUnifiees()
                    } else if (suiviU.n === 14) {
                        choisir(0, 0)
                        suiviU.cle = cleChoisie
                        var r = modeleMessages.get(1)
                        suiviU.sujet = r.sujet
                        var avant = modeleMessages.count
                        supprimerLigne(1)
                        console.log("scenario: unifiée", avant, "->", modeleMessages.count, "| choisie gardée",
                                    cleChoisie === suiviU.cle, "|", suiviU.sujet)
                    } else if (suiviU.n === 20) {
                        var revenu = false
                        for (var i = 0; i < modeleMessages.count; ++i)
                            revenu = revenu || modeleMessages.get(i).sujet === suiviU.sujet
                        console.log(!revenu && deplacementsEnVol === 0 && cleChoisie === suiviU.cle
                                    ? "scenario: OK corbeille dans Toutes les BàL"
                                    : "scenario: ECHEC revenu " + revenu + " en vol " + deplacementsEnVol)
                        ouvrirDossier(a.id, "Trash")
                    } else if (suiviU.n === 24) {
                        for (var j = 0; j < modeleMessages.count; ++j) {
                            if (modeleMessages.get(j).sujet === suiviU.sujet) {
                                choisir(j, 0)
                                deplacerVers(a.id, "INBOX")
                                console.log("scenario: message ramene de la corbeille")
                                break
                            }
                        }
                    } else if (suiviU.n === 28) {
                        tic.stop()
                        Qt.quit()
                    }
                })
                tic.start()
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
                    // MMAIL_HOTE_ENVOI : l'envoi passe par ce serveur-là.
                    if (identifiantsEssai.hoteEnvoi && identifiantsEssai.hoteEnvoi.length > 0)
                        console.log("scenario: serveur d'envoi", identifiantsEssai.hoteEnvoi,
                                    boite.poserServeurEnvoi(c.adresse, identifiantsEssai.hoteEnvoi),
                                    "|", JSON.stringify(JSON.parse(boite.comptes()).map(function(x) { return x.hoteEnvoi })))
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
                attenteImage.triggered.connect(scenarios.etapeScenarioCollerImage)
                attenteImage.start()
            }
            if (essai.scenario === "brouillon-auto") {
                var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 6000 }', fenetre)
                attente.triggered.connect(function() { scenarios.etapeScenarioBrouillonAuto(null, 0) })
                attente.start()
            }
            // Scénario « horsligne » : aucune connexion ; le profil d'une
            // session précédente doit suffire à lire la boîte de réception.
            // Scénario « mesure-liste » : aucune connexion ; chronomètre
            // l'ouverture d'INBOX depuis l'index et ses rafraîchissements.
            if (essai.scenario === "mesure-recherche") {
                var mesureRecherche = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                mesureRecherche.triggered.connect(scenarios.etapeScenarioMesureRecherche)
                mesureRecherche.start()
                return
            }
            if (essai.scenario === "mesure-liste") {
                var mesure = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                mesure.triggered.connect(scenarios.etapeScenarioMesure)
                mesure.start()
                return
            }
            if (essai.scenario === "horsligne") {
                var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
                attente.triggered.connect(scenarios.etapeScenarioHorsLigne)
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

    Scenarios { id: scenarios }

    function capturerBandeauVersion(nom) {
        if (!identifiantsEssai.sortie)
            return
        var chemin = lanceur.chemin(identifiantsEssai.sortie + "/bandeau-" + nom + ".png")
        bandeauVersion.grabToImage(function(r) { r.saveToFile(chemin) })
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
                return { vue: favoris ? colonneArborescence.vueFavoris : colonneArborescence.vueArborescence, index: i }
        }
        return null
    }

    function favorisActuels() {
        return JSON.parse(boite.arborescence(false))
                .filter(function(l) { return l.genre === "favori" })
                .map(function(l) { return l.adresse + ":" + l.chemin })
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
        // Associations des types de fichiers : un texte s'ouvre, une
        // extension inventée non (MMail fait alors choisir le logiciel).
        if (!lanceur.associe("txt"))
            return "aucun logiciel associé aux fichiers .txt"
        if (Qt.platform.os === "windows" && lanceur.associe("mmailessai"))
            return "un logiciel associé à une extension inexistante"
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
