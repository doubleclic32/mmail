// SPDX-License-Identifier: GPL-3.0-or-later
// Réactions de l'interface aux signaux du noyau (Boite) : session ouverte,
// dossier lu, corps reçu, envoi, brouillon, agenda, déplacement, échec…
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, modeleMessages…) sont ceux de
// Main.qml, où « ReactionsNoyau {} » l'instancie. Aucune propriété de la
// fenêtre n'y est écrite sans être nommée (vérifié au découpage).
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Connections {
    target: boite

    function onMicrosoftConnecte(adresse, hote, jeton) {
        // Un compte Microsoft ne vit que par le coffre : sans son jeton,
        // il faudrait se reconnecter à chaque lancement. Écrit une fois la
        // connexion acceptée (cf. onConnecte).
        fenetre.secretsAConserver[adresse] = { cle: fenetre.cle(adresse, hote), motDePasse: jeton }
        fenetre.dernierDemande = null
        messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
    }

    function onMicrosoftRefuse(message, consentement) {
        if (consentement)
            dlgConsentement.ouvrir(message)
        else
            messageEtat.texte = qsTr("Connexion à Microsoft : %1").arg(message)
    }

    function onGoogleConnecte(adresse, hote, jeton) {
        // Comme un compte Microsoft : le jeton va au coffre une fois la
        // connexion acceptée.
        fenetre.secretsAConserver[adresse] = { cle: fenetre.cle(adresse, hote), motDePasse: jeton }
        fenetre.dernierDemande = null
        messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
    }

    function onGoogleRefuse(message) {
        messageEtat.texte = qsTr("Connexion à Google : %1").arg(message)
    }

    function onJetonRenouvele(adresse, hote, jeton) {
        coffre.ecrire(fenetre.cle(adresse, hote), jeton)
    }

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
        if (fenetre.modificationEnCours && fenetre.modificationEnCours.id === compte)
            fenetre.modificationEnCours = null
        if (reglagesSauvegarde.aChercher && !fenetre.rechercheSauvegarde) {
            fenetre.rechercheSauvegarde = true
            boite.chercherSauvegardes(compte)
        }
        if (fenetre.essai && fenetre.essai.scenario === "sauvegarde" && fenetre.essai.etape === 0) {
            fenetre.essai.etape = 1
            var pauseSauvegarde = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
            pauseSauvegarde.triggered.connect(function() { scenarios.etapeScenarioSauvegarde(compte, adresse) })
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
            // Serveur de lecture changé : l'ancienne clé ne sert plus.
            if (secret.ancienne)
                fenetre.effacerDiscretement(secret.ancienne)
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
            scenarios.etapeScenario()
            if (fenetre.essai.scenario === "glisser")
                scenarios.etapeScenarioGlisser()
            if (fenetre.essai.scenario === "corbeille-ligne")
                scenarios.etapeScenarioCorbeille()
            if (fenetre.essai.scenario === "glisser-ligne")
                scenarios.etapeScenarioGlisserLigne()
            if (fenetre.essai.scenario === "filtres")
                scenarios.etapeScenarioFiltres()
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
            colonneListe.champRecherche.text = identifiantsEssai.dossier
            fenetre.lancerRecherche()
            console.log("scenario: l'index trouve", modeleMessages.count, "message(s)")
            return
        }
        // Scénario « double-clic » (0.6.16) : le premier message de INBOX,
        // non lu, ouvert d'un double-clic avec le marquage « Jamais » — sa
        // fenêtre s'ouvre et il passe lu (suite dans etapeScenarioDoubleClic) ;
        // puis un brouillon, qui se reprend.
        if (fenetre.essai && fenetre.essai.scenario === "double-clic" && !fenetre.essai.ouvert && chemin === "INBOX") {
            fenetre.essai.ouvert = true
            Qt.callLater(function() {
                reglages.delaiLu = -1
                fenetre.choisir(0, 0)
                var avant = modeleMessages.get(0).lu
                fenetre.doubleClicMessage(0)
                console.log("scenario: double-clic sur « " + modeleMessages.get(0).sujet + " » | marquage", reglages.delaiLu,
                            "| lu avant", avant, "| après", modeleMessages.get(0).lu,
                            "| fenêtres", fenetre.fenetresMessage.length)
            })
        }
        if (fenetre.essai && fenetre.essai.scenario === "double-clic" && fenetre.essai.etape === "brouillons"
                && chemin === "Drafts" && !fenetre.essai.repris) {
            // La liste du dossier se pose après ce signal ; vide, c'est
            // l'index avant la lecture du serveur : on attend la suivante.
            scenarios.plusTard(1500, function() {
                if (fenetre.essai.repris || modeleMessages.count === 0)
                    return
                fenetre.essai.repris = true
                var avant = Object.keys(fenetre.redactions).length
                fenetre.choisir(0, 0)
                fenetre.doubleClicMessage(0)
                console.log("scenario: brouillon « " + (modeleMessages.count > 0 ? modeleMessages.get(0).sujet : "—")
                            + " » : rédactions", avant, "→", Object.keys(fenetre.redactions).length,
                            "| fenêtres de message", fenetre.fenetresMessage.length)
                scenarios.plusTard(3000, Qt.quit)
            })
        }
        // Scénario « recherche-lente » (0.6.16) : le texte de MMAIL_DOSSIER
        // cherché dans INBOX, où le banc fait durer la recherche (LENTEUR),
        // puis dans toutes les boîtes ; le compte doit rester connecté (suite
        // dans onResultatsServeur, onRechercheInachevee, onResultatsPartout).
        if (fenetre.essai && fenetre.essai.scenario === "recherche-lente" && !fenetre.essai.cherche) {
            fenetre.essai.cherche = true
            fenetre.essai.depart = Date.now()
            colonneListe.champRecherche.text = identifiantsEssai.dossier || "essai"
            fenetre.lancerRecherche()
            console.log("scenario: dossier actif, l'index trouve", modeleMessages.count, "message(s) | compte",
                        fenetre.compteParId(compte).etat, "|", messageEtat.texte)
            return
        }
        // Scénario « recherche-serveur » : INBOX seule passée par l'index,
        // le texte de MMAIL_DOSSIER est cherché dans toutes les boîtes ; le
        // serveur le trouve aussi dans le corps des messages et dans les
        // dossiers jamais ouverts (suite dans onResultatsPartout).
        if (fenetre.essai && fenetre.essai.scenario === "recherche-serveur" && !fenetre.essai.cherche) {
            fenetre.essai.cherche = true
            colonneListe.porteeRecherche.currentIndex = 1
            colonneListe.champRecherche.text = identifiantsEssai.dossier
            fenetre.lancerRecherche()
            console.log("scenario: l'index trouve", modeleMessages.count, "message(s) | dossiers à interroger",
                        fenetre.dossiersPartout)
            return
        }
        // Scénario « sous-dossier » : le sous-dossier MMAIL_DOSSIER de la
        // boîte de réception, créé par le dialogue du clic droit. Un nom
        // déjà pris ou interdit est refusé dans le dialogue même.
        if (fenetre.essai && fenetre.essai.scenario === "sous-dossier" && !fenetre.essai.cree) {
            fenetre.essai.cree = true
            dlgNouveauDossier.ouvrir(compte, "INBOX", qsTr("Boîte de réception"))
            dlgNouveauDossier.champNouveauDossier.text = "a/b"
            dlgNouveauDossier.creer()
            console.log("scenario: refus de « a/b » :", dlgNouveauDossier.motif, "| dialogue ouvert :", dlgNouveauDossier.visible)
            dlgNouveauDossier.champNouveauDossier.text = identifiantsEssai.dossier
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
            colonneListe.champRecherche.forceActiveFocus()
            fenetre.proposerHistorique()
            if (!colonneListe.panneauHistorique.opened) {
                colonneListe.panneauHistorique.entrees = fenetre.historiqueRecherches()
                colonneListe.panneauHistorique.open()
            }
            console.log("scenario: historique proposé", colonneListe.panneauHistorique.entrees.length, "| ouvert", colonneListe.panneauHistorique.opened)
        }
        if (fenetre.essai && fenetre.essai.scenario === "recherche-partout" && fenetre.essai.etape < 3) {
            scenarios.etapeScenarioRecherchePartout(compte, chemin)
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
            scenarios.etapeScenario()
        if (fenetre.essai && fenetre.essai.scenario === "glisser")
            scenarios.etapeScenarioGlisser()
        if (fenetre.essai && fenetre.essai.scenario === "corbeille-ligne")
            scenarios.etapeScenarioCorbeille()
        if (fenetre.essai && fenetre.essai.scenario === "glisser-ligne")
            scenarios.etapeScenarioGlisserLigne()
        if (fenetre.essai && fenetre.essai.scenario === "filtres")
            scenarios.etapeScenarioFiltres()
        if (fenetre.essai && fenetre.essai.scenario === "en-tete")
            scenarios.etapeScenarioEnTete()
        if (fenetre.essai && fenetre.essai.scenario === "dialogues")
            scenarios.etapeScenarioDialogues()
        if (fenetre.essai && fenetre.essai.scenario === "image-filtres")
            scenarios.etapeScenarioImageFiltres()
        if (fenetre.essai && fenetre.essai.scenario === "tactile")
            scenarios.etapeScenarioTactile()
        if (fenetre.essai && fenetre.essai.scenario === "accessibilite")
            scenarios.etapeScenarioAccessibilite()
        if (fenetre.essai && fenetre.essai.scenario === "chevron")
            scenarios.etapeScenarioChevron()
        if (fenetre.essai && (fenetre.essai.scenario === "defilement" || fenetre.essai.scenario === "favoris-fixes"))
            scenarios.etapeScenarioDefilement()
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
            scenarios.etapeScenarioSignature()
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

    function onCorpsRecu(uid, texte, brut, html, bloquees, pieces, confirmation, entete, compte, chemin) {
        // Un UID ne désigne un message que dans son dossier : celui d'une
        // fenêtre de message (0.6.16) peut porter le même numéro.
        if (uid !== fenetre.uidCourant || brut !== fenetre.sourceVisible
                || compte !== boite.compteCourant || chemin !== boite.dossierCourant)
            return
        // Le format d'abord : le texte s'interprète selon lui.
        fenetre.lienSurvole = ""
        fenetre.corpsHtml = html
        fenetre.htmlAffiche = html ? texte : ""
        fenetre.imagesBloquees = bloquees
        colonneLecture.vueCorps.text = html ? fenetre.htmlAuZoom(texte) : texte
        fenetre.bornerImages()
        colonneLecture.vueCorps.cursorPosition = 0
        if (!brut) {
            fenetre.confirmationDemandee = confirmation
            fenetre.pieces = JSON.parse(pieces)
            // Le message entier dit s'il porte des pièces : la ligne suit.
            var i = fenetre.rangDe(uid)
            if (i >= 0 && modeleMessages.get(i).pieces !== (fenetre.pieces.length > 0))
                modeleMessages.setProperty(i, "pieces", fenetre.pieces.length > 0)
        }
        if (fenetre.essai && fenetre.essai.scenario === "pieces")
            scenarios.etapeScenarioPieces()
        if (fenetre.essai && fenetre.essai.scenario === "impression" && !brut)
            Qt.callLater(scenarios.etapeScenarioImpression)
        if (fenetre.essai && fenetre.essai.scenario === "ouvrir-avec")
            scenarios.etapeScenarioOuvrirAvec()
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
            scenarios.etapeScenarioBrouillonAuto(r, uid)
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

    function onMarquageLuDiffere(uid, secondes) {
        minuterieLu.uid = uid
        minuterieLu.interval = secondes * 1000
        minuterieLu.restart()
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
        if (fenetre.essai && fenetre.essai.scenario === "joindre") {
            console.log("scenario: pièces de la rédaction :", JSON.stringify(r.pieces.map(function(p) { return p.nom })))
            // Puis ouverte depuis la rédaction, pour la vérifier (0.6.9).
            r.ouvrirPiece(r.pieces[0])
            console.log("scenario: ouverture depuis la rédaction | risquée", r.pieces[0].risquee, "|", r.note)
        }
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

    // Recherche dans toutes les boîtes : un dossier de plus a répondu. La
    // liste n'est reposée que s'il a trouvé quelque chose, ou à la fin ;
    // elle garde sa position si l'on s'y est déplacé.
    function onResultatsPartout(compte, chemin, texte, uids, restants) {
        if (!fenetre.rechercheGlobale || texte !== fenetre.recherche)
            return
        fenetre.dossiersFaits += 1
        if (uids.length > 0) {
            var trouves = fenetre.trouvesPartout
            trouves[compte + "\n" + chemin] = uids.split(",").map(Number)
            fenetre.trouvesPartout = trouves
        }
        var fini = fenetre.dossiersFaits >= fenetre.dossiersPartout
        if (uids.length > 0 || fini)
            fenetre.poserResultats()
        messageEtat.texte = fenetre.etatRecherchePartout()
        if (fini && fenetre.essai && fenetre.essai.scenario === "recherche-lente") {
            console.log("scenario: toutes les boîtes, fin après", scenarios.secondesDepuis(fenetre.essai.depart),
                        "s | résultats", modeleMessages.count, "| inachevés", fenetre.inachevesPartout.length,
                        "| compte", fenetre.compteParId(compte).etat, "|", messageEtat.texte)
            scenarios.plusTard(8000, Qt.quit)
        }
        if (fenetre.essai && fenetre.essai.scenario === "recherche-serveur") {
            console.log("scenario: serveur,", chemin, ":", uids.length > 0 ? uids.split(",").length : 0,
                        "| restants", restants, "| liste", modeleMessages.count)
            if (fini) {
                var dossiers = {}
                for (var i = 0; i < modeleMessages.count; ++i)
                    dossiers[modeleMessages.get(i).chemin] = true
                console.log("scenario: fin |", messageEtat.texte, "| dossiers", Object.keys(dossiers).join(", "),
                            "| premier :", modeleMessages.count > 0 ? modeleMessages.get(0).sujet : "—")
                Qt.quit()
            }
        }
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
        if (fenetre.essai && fenetre.essai.scenario === "recherche-lente" && !fenetre.essai.partout) {
            // `rechercheInachevee`, s'il vient, suit ce signal : le bilan
            // attend qu'il soit passé.
            fenetre.essai.partout = true
            Qt.callLater(function() {
                console.log("scenario: dossier actif, le serveur rend", uids.length > 0 ? uids.split(",").length : 0,
                            "UID après", scenarios.secondesDepuis(fenetre.essai.depart), "s | compte",
                            fenetre.compteParId(compte).etat, "|", messageEtat.texte)
                // Huit secondes pour une capture de la barre d'état, puis
                // toutes les boîtes.
                scenarios.plusTard(8000, function() {
                    fenetre.essai.depart = Date.now()
                    colonneListe.porteeRecherche.currentIndex = 1
                    fenetre.lancerRecherche()
                    console.log("scenario: toutes les boîtes, dossiers à interroger", fenetre.dossiersPartout,
                                "|", messageEtat.texte)
                })
            })
        }
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

    // Du nouveau courrier (0.6.2) : une notification si la fenêtre n'est pas
    // au premier plan. Pas sous Android, où MMail ne relève pas le courrier
    // en arrière-plan.
    function onNouveauCourrier(compte, nombre, expediteur, sujet) {
        var titre = nombre > 1 ? qsTr("%1 nouveaux messages").arg(nombre)
                               : qsTr("Nouveau message de %1").arg(expediteur)
        var texte = nombre > 1 ? qsTr("Dernier : %1 — %2").arg(expediteur).arg(sujet) : sujet
        var montrer = reglages.notifications && !fenetre.active && Qt.platform.os !== "android"
        var faite = montrer ? notifieur.notifier(titre, texte) : false
        if (fenetre.essai && fenetre.essai.scenario === "notification")
            scenarios.etapeScenarioNotification(nombre, expediteur, sujet, montrer, faite)
    }

    // Des filtres ont agi : leurs déplacements, lancés par le noyau, rendront
    // chacun « deplacementTermine », comme ceux de l'interface.
    function onFiltresAppliques(compte, touches, deplacements) {
        fenetre.deplacementsEnVol += deplacements
        if (touches > 0)
            messageEtat.texte = qsTr("Filtres : %1.").arg(fenetre.accord(touches, qsTr("message trié"), qsTr("messages triés")))
        if (fenetre.essai && fenetre.essai.scenario === "filtres")
            console.log("scenario: filtres appliqués", compte, touches, deplacements)
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
            scenarios.etapeScenario()
            if (fenetre.essai.scenario === "glisser")
                scenarios.etapeScenarioGlisser()
            if (fenetre.essai.scenario === "corbeille-ligne")
                scenarios.etapeScenarioCorbeille()
            if (fenetre.essai.scenario === "glisser-ligne")
                scenarios.etapeScenarioGlisserLigne()
            if (fenetre.essai.scenario === "filtres")
                scenarios.etapeScenarioFiltres()
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
                dlgNouveauDossier.champNouveauDossier.text = nom
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
            scenarios.bilanScenarioVidage("vidées, " + nombre + " message(s) | " + messageEtat.texte)
    }

    // Le serveur n'a pas fini de chercher dans un dossier (0.6.16) : ses
    // résultats sont ceux de l'index, et la barre d'état le dit. Le compte
    // reste connecté.
    function onRechercheInachevee(compte, chemin, dossier, texte, motif) {
        if (texte !== fenetre.recherche)
            return
        var c = fenetre.compteParId(compte)
        var nom = (c ? c.adresse + " › " : "") + dossier
        if (fenetre.rechercheGlobale) {
            // Le bilan vient avec `resultatsPartout`, qui suit.
            fenetre.inachevesPartout = fenetre.inachevesPartout.concat([{ nom: nom, motif: motif }])
            return
        }
        if (compte !== boite.compteCourant || chemin !== boite.dossierCourant)
            return
        messageEtat.texte = qsTr("Recherche « %1 » : %2 — le serveur n'a pas fini de chercher dans « %3 » (%4) : résultats des messages déjà sur le poste.")
                .arg(texte).arg(fenetre.accord(modeleMessages.count, qsTr("résultat"), qsTr("résultats")))
                .arg(nom).arg(motif)
    }

    function onEchec(compte, etape, message) {
        if (fenetre.essai && fenetre.essai.scenario === "vidage")
            scenarios.bilanScenarioVidage("échec du compte " + compte + " | étape " + etape + " | " + message)
        if (etape === "tri") {
            // Le compte des déplacements en cours n'est pas touché ici : un
            // déplacement rend toujours « deplacementTermine », réussi ou
            // non ; un « tri » échoué peut aussi être un marquage.
            fenetre.rafraichirListe()
        } else if (etape === "identifiants") {
            // Mot de passe refusé : on redemande, sans effacer d'office
            // celui du coffre — il a pu changer côté serveur.
            if (fenetre.modificationEnCours && fenetre.modificationEnCours.id === compte)
                fenetre.modificationEnCours = null
            var c = fenetre.compteParId(compte)
            fenetre.demanderCompte(c ? c : fenetre.dernierDemande, message)
        } else if (etape === "connexion" && fenetre.modificationEnCours
                   && fenetre.modificationEnCours.id === compte) {
            // Nouveau serveur de lecture injoignable : la fenêtre de
            // modification revient, avec le motif.
            var m = fenetre.modificationEnCours
            fenetre.modificationEnCours = null
            fenetre.demanderCompte({ id: m.id, adresse: m.adresse, hote: m.hote, hoteEnvoi: m.hoteEnvoi },
                                   message, true)
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
