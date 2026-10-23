// SPDX-License-Identifier: GPL-3.0-or-later
// Scénarios d'essai (MMAIL_SCENARIO) : l'interface pilotée sans personne
// devant l'écran, contre les boîtes d'essai, chaque étape tracée sur la
// console (« scenario: … »). Jamais en usage courant : rien ne les appelle
// sans les variables d'environnement d'essai (cf. cpp/main.cpp).
//
// Sortis de Main.qml en 0.5.16 (découpage) : les noms qu'ils emploient sans
// les déclarer (fenetre, boite, essai, choisir…) sont ceux de Main.qml, où
// « Scenarios { id: scenarios } » les instancie ; une propriété de la
// fenêtre ne s'y écrit que qualifiée (fenetre.selection = …).
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

QtObject {
    id: scenarios

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
            colonneListe.porteeRecherche.currentIndex = 1
            colonneListe.champRecherche.text = identifiantsEssai.dossier
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
                            "| affiché", colonneLecture.sujetAffiche.text, "| liste", modeleMessages.count, "| globale", rechercheGlobale)
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
            console.log("scenario: affiché :", colonneLecture.sujetAffiche.text, "| html :", corpsHtml, "|",
                        colonneLecture.vueCorps.text.length, "caractères",
                        colonneLecture.vueCorps.text.indexOf("Chargement") === 0 || colonneLecture.vueCorps.text.length < 20 ? "ECHEC" : "OK")
        })
        attente.start()
    }

    /// Scénario d'essai « envoyes » : sans connexion, le dossier des envoyés
    /// du premier compte s'ouvre sur l'index ; sa première ligne doit nommer
    /// les destinataires (« À : … »), et non l'expéditeur.
    function etapeScenarioEnvoyes() {
        var c = comptesConnus[0]
        var envoyes = JSON.parse(boite.arborescence(false)).filter(function(l) {
            return l.genre === "dossier" && l.compte === c.compte && l.role === "Sent"
        })
        if (envoyes.length === 0) {
            console.log("scenario: envoyés : aucun dossier d'envoyés ECHEC")
            return
        }
        ouvrirDossier(c.compte, envoyes[0].chemin)
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
        attente.triggered.connect(function() {
            var ligne = colonneListe.vueMessages.itemAtIndex(0)
            var texte = ligne ? ligne.auteurLigne : ""
            console.log("scenario: envoyés,", fenetre.roleCourant, "|", modeleMessages.count, "messages | 1re ligne :",
                        texte, texte.indexOf(qsTr("À : ")) === 0 ? "OK" : "ECHEC")
        })
        attente.start()
    }

    /// Scénario d'essai « suggestion » : sans connexion, l'entrée du menu « ? »
    /// ouvre une rédaction adressée à `adresseSuggestions`, version et système
    /// dans l'objet.
    function etapeScenarioSuggestion() {
        var r = envoyerSuggestion()
        var a = r ? r.contenu().a : ""
        var objet = r ? r.contenu().objet : ""
        console.log("scenario: suggestion,", a, "|", objet,
                    a === adresseSuggestions && objet.indexOf(Qt.application.version) > 0 ? "OK" : "ECHEC")
    }

    /// Scénario « mesure-recherche » : temps d'une recherche dans le dossier
    /// ouvert (index seul, hors connexion), du retour à la liste entière, et
    /// d'une recherche dans toutes les boîtes.
    function etapeScenarioMesureRecherche() {
        var c = comptesConnus[0]
        ouvrirDossier(c.compte, "INBOX")
        colonneListe.champRecherche.text = "Correspondant 12"
        var t = Date.now()
        lancerRecherche()
        console.log("mesure: recherche dans le dossier,", modeleMessages.count, "résultats,", Date.now() - t, "ms")
        t = Date.now()
        quitterRecherche()
        console.log("mesure: sortie de recherche,", modeleMessages.count, "messages,", Date.now() - t, "ms")
        colonneListe.porteeRecherche.currentIndex = 1
        colonneListe.champRecherche.text = "dossier 39"
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
        fenetre.mesureSuite = { paquets: 0, plusLong: 0 }
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
            fenetre.mesureSuite = null
            var index = JSON.parse(boite.messages())
            var conforme = index.length === modeleMessages.count
            for (var i = 0; conforme && i < index.length; ++i)
                conforme = index[i].uid === modeleMessages.get(i).uid
            console.log("mesure: liste conforme à l'index :", conforme)
            // Relecture complète pendant qu'un message lointain est choisi : la
            // suite est lue d'un trait jusqu'à lui, il reste choisi.
            var loin = modeleMessages.get(30000).uid
            fenetre.uidCourant = loin
            fenetre.selection = ({ [loin]: true })
            var t3 = Date.now()
            rafraichirListe(true)
            console.log("mesure: relecture complète, message choisi au rang 30 000 :", Date.now() - t3, "ms |",
                        modeleMessages.count, "lignes | toujours choisi", uidCourant === loin && selection[loin] === true,
                        "| à son rang", indexDe(loin) === 30000)
            arreterSuite()
            fenetre.uidCourant = 0
            fenetre.selection = ({})
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
                var v = colonneArborescence.vueArborescence
                v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
                console.log("scenario: comptes défilés à", v.contentY, "; Favoris visibles :", colonneArborescence.vueFavoris.visible, colonneArborescence.vueFavoris.count, "lignes")
            })
            attente.start()
            return
        }
        Qt.callLater(function() {
            var v = colonneArborescence.vueArborescence
            v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
            var avant = v.contentY
            var sonde = Qt.createQmlObject('import QtQuick; Rectangle { width: 40; height: 16 }', fenetre.contentItem)
            var p = v.mapToItem(fenetre.contentItem, 20, 4)
            sonde.x = p.x
            sonde.y = p.y
            fenetre.glisse = sonde
            var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            attente.triggered.connect(function() {
                fenetre.glisse = null
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
        var ligne = i >= 0 ? colonneArborescence.vueArborescence.itemAtIndex(i) : null
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

    /// Scénario « corbeille-ligne » : dans la boîte de réception, le premier
    /// message affiché, la corbeille de la deuxième ligne l'envoie à la
    /// corbeille sans toucher au message affiché ni à la sélection ; il y est
    /// retrouvé, puis ramené.
    // Scénario « en-tete » : l'objet et l'expéditeur du message affiché se
    // sélectionnent et se copient ; « Enregistrer sous… » propose le dernier
    // dossier choisi s'il existe, sinon les téléchargements.
    function etapeScenarioEnTete() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        var ligne = modeleMessages.get(0)
        colonneLecture.sujetAffiche.selectAll()
        console.log("scenario: objet sélectionnable", colonneLecture.sujetAffiche.selectByMouse && colonneLecture.sujetAffiche.readOnly,
                    "| sélection = objet", colonneLecture.sujetAffiche.selectedText === ligne.sujet)
        menuEnTete.texteChoisi = colonneLecture.sujetAffiche.selectedText
        copierTexte(menuEnTete.texteChoisi, "message")
        console.log("scenario: copie de l'objet", pressePapier.texte === ligne.sujet)
        copierTexte(adresseAffichee, "message")
        console.log("scenario: copie de l'adresse", adresseAffichee.length > 0 && pressePapier.texte === ligne.adresse,
                    "|", adresseAffichee)
        colonneLecture.auteurAffiche.selectAll()
        console.log("scenario: expéditeur sélectionnable", colonneLecture.auteurAffiche.selectedText.indexOf(ligne.adresse) >= 0)
        reglagesPieces.dernierDossier = ""
        var d1 = dossierEnregistrement()
        reglagesPieces.dernierDossier = "file:///tmp"
        var d2 = dossierEnregistrement()
        reglagesPieces.dernierDossier = "file:///tmp/mmail-dossier-absent"
        var d3 = dossierEnregistrement()
        reglagesPieces.dernierDossier = ""
        console.log("scenario: dossier d'enregistrement", d1 === dossierTelechargements, d2 === "file:///tmp",
                    d3 === dossierTelechargements)
        Qt.quit()
    }

    // Scénario « accessibilite » : ce que les lecteurs d'écran recevront des
    // lignes de la liste et de l'arborescence (rôle, nom, sélection), lu sur
    // les objets eux-mêmes — la plateforme sans écran n'a pas de pont
    // d'accessibilité à interroger.
    function etapeScenarioAccessibilite() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        for (var i = 0; i < Math.min(3, modeleMessages.count); ++i) {
            var l = colonneListe.vueMessages.itemAtIndex(i)
            if (l)
                console.log("scenario: message", i, "| rôle", l.Accessible.role === Accessible.ListItem,
                            "| choisi", l.Accessible.selected, "|", l.Accessible.name)
        }
        for (var k = 0; k < Math.min(8, colonneArborescence.vueArborescence.count); ++k) {
            var d = colonneArborescence.vueArborescence.itemAtIndex(k)
            if (d)
                console.log("scenario: arborescence", k, "| rôle", d.Accessible.role === Accessible.TreeItem,
                            "| courant", d.Accessible.selected, "|", d.Accessible.name)
        }
        Qt.quit()
    }

    // Scénario « dialogues » : chaque dialogue et menu s'ouvre puis se referme,
    // l'un après l'autre, sans rien valider — leurs liaisons sont évaluées, une
    // référence cassée paraît sur la console. Sert à comparer deux versions.
    function etapeScenarioDialogues() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        var cibles = [["dlgIdentite", dlgIdentite], ["dlgAide", dlgAide], ["dlgDeplacer", dlgDeplacer],
                      ["dlgRestauration", dlgRestauration], ["dlgAPropos", dlgAPropos], ["dlgCompte", dlgCompte],
                      ["dlgNouveauDossier", dlgNouveauDossier], ["dlgConsentement", dlgConsentement],
                      ["dlgOuvrirAvec", dlgOuvrirAvec], ["dlgLogiciels", dlgLogiciels], ["dlgReprise", dlgReprise],
                      ["dlgLien", dlgLien], ["dlgViderDossier", dlgViderDossier], ["popupRappels", popupRappels],
                      ["menuComptes", menuComptes], ["menuAffichage", menuAffichage], ["menuPiece", menuPiece],
                      ["menuMessage", menuMessage], ["menuDossier", menuDossier], ["menuCompte", menuCompte]]
        var i = 0
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 400; repeat: true }", fenetre)
        t.triggered.connect(function() {
            if (i > 0)
                cibles[i - 1][1].close()
            if (i >= cibles.length) {
                t.stop()
                console.log("scenario: dialogues et menus parcourus :", cibles.length)
                Qt.quit()
                return
            }
            var c = cibles[i]
            if (typeof c[1].popup === "function")
                c[1].popup()
            else
                c[1].open()
            console.log("scenario: ouvert", c[0], c[1].opened === true)
            i += 1
        })
        t.start()
    }

    // Scénario « tactile » : la vue du téléphone en images, sans téléphone
    // (MMAIL_TACTILE=1, MMAIL_TAILLE=412x915) — liste, tiroir, menu ⋮, message.
    function etapeScenarioTactile() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        var racine = fenetre.contentItem
        while (racine.parent)
            racine = racine.parent
        function saisir(nom) {
            var chemin = lanceur.chemin(identifiantsEssai.sortie + "/tactile-" + nom + ".png")
            racine.grabToImage(function(r) { r.saveToFile(chemin) })
            console.log("scenario: tactile", nom, "| vue", vue, "| tiroir", tiroir.opened,
                        "| menu", menuPlus.opened || menuComptes.opened, "| titre", boite.dossierCourant,
                        "| barre", entete.height)
        }
        var etapes = [
            function() { saisir("liste") },
            function() { tiroir.open() },
            function() { saisir("tiroir") },
            function() { tiroir.close(); menuPlus.popup(fenetre.width - 10, 0) },
            function() { saisir("menu") },
            function() { menuPlus.close(); menuComptes.popup(boutonPlus, boutonPlus.width - 260, boutonPlus.height) },
            function() { saisir("comptes") },
            function() { menuComptes.close(); bandeauGlissement.texte = qsTr("Message mis à la corbeille."); bandeauGlissement.open() },
            function() { saisir("annuler") },
            function() { bandeauGlissement.close(); choisir(0, 0) },
            function() { saisir("message") },
            function() { vue = 1; rediger("nouveau") },
            function() { saisir("redaction") },
            // Retour à la liste d’abord : depuis le message, la fermeture (et Qt.quit)
            // est le retour arrière d’Android, qui ramène à la liste sans quitter.
            function() { console.log("scenario: tactile fin | tactile", tactile, "| compact", compact); vue = 1; Qt.quit() }
        ]
        var i = 0
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 700; repeat: true }", fenetre)
        t.triggered.connect(function() {
            if (i < etapes.length)
                etapes[i++]()
        })
        t.start()
    }

    // Scénario « glisser-ligne » : une ligne glissée vers la corbeille, puis
    // annulée — elle revient ; glissée de nouveau et laissée partir — elle est
    // dans la corbeille ; ramenée enfin en boîte de réception.
    function etapeScenarioGlisserLigne() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(1).sujet
            var avant = modeleMessages.count
            // Comme au doigt : la ligne est d'abord glissée (ouverte).
            var glissee = colonneListe.vueMessages.itemAtIndex(1)
            if (glissee)
                glissee.swipe.open(SwipeDelegate.Right)
            glisserLigne(1, "corbeille")
            console.log("scenario: glissée", avant, "->", modeleMessages.count, "| bandeau", bandeauGlissement.opened,
                        "|", bandeauGlissement.texte, "|", essai.sujet)
            annulerGlissement()
            var revenue = -1
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet)
                    revenue = i
            }
            console.log(revenue >= 0 && modeleMessages.count === avant && !bandeauGlissement.opened
                        ? "scenario: OK annulée, ligne revenue au rang " + revenue
                        : "scenario: ECHEC annulation " + modeleMessages.count + " / " + avant)
            // Retour de Manu du 18/10 : après un glisser, d'autres lignes
            // restaient décalées. Une fois la liste redessinée, aucune ligne
            // affichée ne doit l'être.
            essai.etape = 5  // en attente du contrôle : les autres étapes se taisent
            // La liste recycle ses lignes : faire défiler jusqu'au bout et
            // revenir reprend les lignes rangées, dont la ligne glissée.
            var vue = colonneListe.vueMessages
            var etapesControle = [
                function() { vue.positionViewAtEnd() },
                function() { vue.positionViewAtBeginning() },
                function() {
                    var decalees = []
                    for (var d = 0; d < modeleMessages.count; ++d) {
                        var l = vue.itemAtIndex(d)
                        if (l && l.visible && l.swipe.position !== 0)
                            decalees.push(d + ":" + l.swipe.position)
                    }
                    console.log(decalees.length === 0 ? "scenario: OK aucune ligne décalée après l'annulation"
                                                       : "scenario: ECHEC lignes décalées " + decalees.join(" "))
                    var rang = -1
                    for (var i = 0; i < modeleMessages.count; ++i) {
                        if (modeleMessages.get(i).sujet === essai.sujet)
                            rang = i
                    }
                    glisserLigne(rang, "corbeille")
                    essai.etape = 1
                    console.log("scenario: glissée de nouveau, en attente", Object.keys(enAttente).length)
                }
            ]
            var controle = Qt.createQmlObject("import QtQuick; Timer { interval: 600; repeat: true }", fenetre)
            controle.triggered.connect(function() {
                etapesControle.shift()()
                if (etapesControle.length === 0)
                    controle.stop()
            })
            controle.start()
        } else if (essai.etape === 1 && glissement === null && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille) {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message dans la corbeille")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de la corbeille")
            essai.etape = 9
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            console.log("scenario: message ramene en boite de reception")
            Qt.quit()
        }
    }

    // Scénario « filtres » (0.6.0) : un filtre incomplet refusé ; un filtre
    // sur l'objet d'un message de la boîte de réception, qui l'envoie dans
    // « Essais » quand on l'applique ; le message y est, on le ramène — et les
    // filtres ne le renvoient pas ; la liste est vidée à la fin.
    function etapeScenarioFiltres() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(1).sujet
            var refus = boite.poserFiltres(JSON.stringify([{ nom: "Incomplet", conditions: [], lu: true }]))
            console.log("scenario: filtre incomplet", refus ? "ECHEC accepté" : "OK refusé : " + boite.erreur)
            var filtres = [{ nom: "Essai des filtres", actif: true, compte: a.adresse, tout: true,
                             conditions: [{ champ: "objet", operateur: "est", valeur: essai.sujet }],
                             lu: false, suivi: false, destination: "dossier", dossier: "Essais", arreter: false }]
            console.log("scenario: filtre posé", boite.poserFiltres(JSON.stringify(filtres)),
                        "| relu", JSON.parse(boite.filtres()).length)
            var touches = boite.appliquerFiltres(a.id)
            console.log(touches === 1 ? "scenario: OK un message touché par le filtre"
                                      : "scenario: ECHEC messages touchés " + touches)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(a.id, "Essais")
        } else if (essai.etape === 2 && boite.dossierCourant === "Essais") {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message rangé dans Essais par le filtre")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent d'Essais")
            essai.etape = 9
            boite.poserFiltres("[]")
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 4
            ouvrirDossier(a.id, "INBOX")
        } else if (essai.etape === 4 && boite.dossierCourant === "INBOX") {
            essai.etape = 9
            var encore = boite.appliquerFiltres(a.id)
            console.log(encore === 0 ? "scenario: OK message ramené laissé en boîte de réception"
                                     : "scenario: ECHEC filtre réappliqué au message ramené : " + encore)
            boite.poserFiltres("[]")
            console.log("scenario: filtres vidés", JSON.parse(boite.filtres() || "[]").length)
            Qt.quit()
        }
    }

    // Scénario « image-filtres » : le dialogue des filtres en image, avec un
    // filtre à l'essai (rien n'est enregistré).
    function etapeScenarioImageFiltres() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        dlgFiltres.ouvrir({ nom: "Factures des fournisseurs", actif: true, compte: a.adresse, tout: false,
                            conditions: [{ champ: "objet", operateur: "contient", valeur: "facture" },
                                         { champ: "expediteur", operateur: "finit", valeur: "@fournisseur.fr" }],
                            lu: true, suivi: false, destination: "dossier", dossier: "Essais", arreter: true })
        var racine = fenetre.contentItem
        while (racine.parent)
            racine = racine.parent
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 900 }", fenetre)
        t.triggered.connect(function() {
            var chemin = lanceur.chemin(identifiantsEssai.sortie + "/filtres.png")
            racine.grabToImage(function(r) {
                r.saveToFile(chemin)
                console.log("scenario: dialogue des filtres en image", dlgFiltres.opened, dlgFiltres.liste.length)
                dlgFiltres.close()
                Qt.quit()
            })
        })
        t.start()
    }

    // Scénario « notification » (0.6.2) : un message à soi-même, d'objet
    // unique ; à son arrivée dans la boîte de réception, le noyau le signale
    // (nouveauCourrier) ; il est ensuite mis à la corbeille.
    function lancerScenarioNotification() {
        var c = comptesConnus[0]
        var n = ouvrirRedaction(c.compte)
        n.pret()
        essai.sujet = "Essai notification " + Date.now()
        n.remplir({ a: identifiantsEssai.utilisateur, objet: essai.sujet, mode: "nouveau", texte: "Arrivée signalée." })
        n.envoyer(false)
        console.log("scenario: message envoyé à soi-même")
    }
    function etapeScenarioNotification(nombre, expediteur, sujet, montrer, faite) {
        console.log("scenario: nouveau courrier", nombre, "|", expediteur, "|", sujet,
                    "| notification", montrer ? (faite ? "montrée" : "demandée, sans service de notification") : "non demandée (fenêtre au premier plan)")
        if (sujet !== essai.sujet || essai.etape)
            return
        essai.etape = 1
        console.log("scenario: OK arrivée signalée")
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 3000 }", fenetre)
        t.triggered.connect(function() {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    supprimerLigne(i)
                    console.log("scenario: message d'essai mis à la corbeille")
                    break
                }
            }
            var fin = Qt.createQmlObject("import QtQuick; Timer { interval: 4000 }", fenetre)
            fin.triggered.connect(function() { Qt.quit() })
            fin.start()
        })
        t.start()
    }

    // Scénario « impression » (0.6.3) : le premier message de la boîte de
    // réception, affiché, est enregistré en PDF dans le dossier de l'essai.
    function etapeScenarioImpression() {
        if (essai.etape || uidCourant <= 0 || colonneLecture.vueCorps.text === qsTr("Chargement…"))
            return
        essai.etape = 1
        var chemin = lanceur.chemin(identifiantsEssai.sortie + "/message.pdf")
        var ok = impression.enregistrerPdf(colonneLecture.vueCorps.textDocument, enteteImpression(), sujetCourant(), chemin)
        console.log("scenario: impression disponible", impression.disponible, "| PDF", ok, "|", sujetCourant())
        Qt.quit()
    }

    function etapeScenarioCorbeille() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            choisir(0, 0)
            var affiche = modeleMessages.get(0).uid
            var avant = modeleMessages.count
            essai.sujet = modeleMessages.get(1).sujet
            supprimerLigne(1)
            var garde = uidCourant === affiche && selection[affiche] === true && uidsChoisis().length === 1
            console.log("scenario: ligne retiree", avant, "->", modeleMessages.count, "|", essai.sujet)
            console.log(modeleMessages.count === avant - 1 && garde
                        ? "scenario: OK message affiche et selection inchanges"
                        : "scenario: ECHEC affiche " + uidCourant + " / " + affiche)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            console.log("scenario: corbeille", corbeille)
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille) {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message dans la corbeille")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de la corbeille")
            essai.etape = 9
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            console.log("scenario: message ramene en boite de reception")
            Qt.quit()
        }
    }


    /// Scénario « non-lu » (0.6.7, demande de Gwenaëlle du 19/10) : un
    /// « Marquer comme non lu » doit tenir, sur la ligne, dans l'index et sur le
    /// serveur. A : message affiché puis marqué lu à l'affichage, puis non lu
    /// avant toute veille — la 0.6.6 laissait la ligne en « lu ». B : non lu
    /// demandé pendant le chargement du message — la 0.6.6 le remarquait lu
    /// sur le serveur. La boîte de réception doit compter deux messages.
    function lancerScenarioNonLu() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, a: 0, b: 0, echecs: 0 }
        var luDansLaListe = function(uid) {
            var i = indexDe(uid)
            return i >= 0 ? modeleMessages.get(i).lu : null
        }
        var luDansLIndex = function(uid) {
            var l = JSON.parse(boite.messagesParUid(String(uid)))
            return l.length > 0 ? l[0].lu : null
        }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    bilan("non-lu : " + modeleMessages.count + " message(s) en boîte de réception, il en faut deux", false)
                    suivi.n = 39
                    return
                }
                suivi.a = modeleMessages.get(0).uid
                suivi.b = modeleMessages.get(1).uid
                // Les deux non lus au départ.
                boite.marquerLu(suivi.a + "," + suivi.b, false)
                rafraichirListe()
            } else if (suivi.n === 10) {
                // A : affiché, donc marqué lu à l'affichage.
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 13) {
                bilan("A : marqué lu à l'affichage | liste " + luDansLaListe(suivi.a), luDansLaListe(suivi.a) === true)
                marquerSelection(false)
                bilan("A : « Marquer comme non lu » aussitôt | liste " + luDansLaListe(suivi.a)
                      + " | index " + luDansLIndex(suivi.a),
                      luDansLaListe(suivi.a) === false && luDansLIndex(suivi.a) === false)
            } else if (suivi.n === 15) {
                // B : choisi, le chargement demandé, et « non lu » dans le même
                // tour de la boucle d'événements — avant que le corps n'arrive.
                choisir(indexDe(suivi.b), 0)
                minuterieCorps.stop()
                boite.demanderCorps(suivi.b)
                marquerSelection(false)
            } else if (suivi.n === 19) {
                bilan("B : non lu pendant le chargement | liste " + luDansLaListe(suivi.b)
                      + " | index " + luDansLIndex(suivi.b),
                      luDansLaListe(suivi.b) === false && luDansLIndex(suivi.b) === false)
                // Le serveur : un autre dossier, puis la boîte de réception
                // relue — ses drapeaux reviennent du serveur.
                fenetre.selection = ({})
                uidCourant = 0
                ouvrirDossier(c.compte, "Essais")
            } else if (suivi.n === 22) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 24) {
                boite.actualiser()
            } else if (suivi.n === 30) {
                bilan("serveur : A " + luDansLIndex(suivi.a) + " | B " + luDansLIndex(suivi.b),
                      luDansLIndex(suivi.a) === false && luDansLIndex(suivi.b) === false)
                console.log("scenario: non-lu, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
                suivi.n = 39
            } else if (suivi.n >= 40) {
                tic.stop()
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « defilement-lecture » (0.6.10) : retirer la sélection du message ne
    /// ramène plus la lecture en haut. Sous Android, chaque début de
    /// glissement la retire ; le TextArea, dans son ScrollView, revenait alors
    /// à son curseur. Le témoin rejoue l'ancien appel, deselect() seul.
    function lancerScenarioDefilementLecture() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var vue = colonneLecture.vueCorps
        var flick = colonneLecture.cadreCorps.contentItem
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | contentY " + Math.round(flick.contentY)
                        + " | sélection « " + vue.selectedText + " »")
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 1) {
                    console.log("scenario: ECHEC defilement-lecture : boîte de réception vide")
                    suivi.echecs += 1
                    suivi.n = 9
                    return
                }
                choisir(0, 0)
            } else if (suivi.n === 9) {
                // Un message long, en texte simple, à la place du corps reçu.
                var lignes = []
                for (var i = 1; i <= 300; ++i)
                    lignes.push("Ligne " + i + " du message d'essai, assez longue pour se replier dans la colonne de lecture.")
                vue.text = lignes.join("\n")
                vue.cursorPosition = 0
            } else if (suivi.n === 10) {
                console.log("scenario: hauteur du contenu " + Math.round(flick.contentHeight) + " | vue " + Math.round(flick.height))
                flick.contentY = 800
                vue.deselect()
                bilan("témoin : deselect() seul, sans sélection, ramène la vue au curseur (attendu : contentY < 800)",
                      flick.contentY < 800)
                flick.contentY = 800
                colonneLecture.deselectionner()
                bilan("sans sélection : la lecture reste à 800", Math.round(flick.contentY) === 800)
            } else if (suivi.n === 11) {
                vue.select(0, 5)
                flick.contentY = 800
                colonneLecture.deselectionner()
                bilan("sélection en haut, lecture plus bas : sélection retirée, la lecture reste à 800",
                      Math.round(flick.contentY) === 800 && vue.selectedText === "")
            } else if (suivi.n === 12) {
                tic.stop()
                console.log("scenario: defilement-lecture " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « recherche-effacee » (0.6.12, demande de Fabienne) : le mot
    /// effacé au clavier (champ vidé, textEdited) met fin à la recherche et
    /// rend la liste de départ — dans le dossier, puis dans toutes les boîtes.
    function lancerScenarioRechercheEffacee() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, total: 0, mot: "" }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | liste " + modeleMessages.count
                        + " | recherche " + JSON.stringify(recherche) + " | globale " + rechercheGlobale
                        + " | noyau sur " + boite.dossierCourant)
        }
        var effacer = function() {
            colonneListe.champRecherche.text = ""
            colonneListe.champRecherche.textEdited()
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 6) {
                suivi.total = modeleMessages.count
                if (suivi.total < 1) {
                    bilan("boîte de réception vide", false)
                    suivi.n = 12
                    return
                }
                var mots = modeleMessages.get(0).sujet.split(/\s+/).filter(function(m) { return m.length > 3 })
                suivi.mot = mots.length > 0 ? mots[0] : modeleMessages.get(0).sujet
                colonneListe.porteeRecherche.currentIndex = 0
                colonneListe.champRecherche.text = suivi.mot
                lancerRecherche()
                bilan("recherche « " + suivi.mot + " » dans le dossier (" + suivi.total + " messages avant)",
                      recherche === suivi.mot)
            } else if (suivi.n === 7) {
                effacer()
            } else if (suivi.n === 8) {
                bilan("mot effacé : recherche finie, liste de départ",
                      recherche === "" && modeleMessages.count === suivi.total)
                colonneListe.porteeRecherche.currentIndex = 1
                colonneListe.champRecherche.text = suivi.mot
                lancerRecherche()
                bilan("recherche « " + suivi.mot + " » dans toutes les boîtes", rechercheGlobale)
            } else if (suivi.n === 10) {
                effacer()
            } else if (suivi.n === 12) {
                bilan("mot effacé : retour au dossier de départ, liste entière",
                      recherche === "" && !rechercheGlobale && boite.dossierCourant === "INBOX"
                      && modeleMessages.count === suivi.total)
            } else if (suivi.n === 13) {
                tic.stop()
                console.log("scenario: recherche-effacee " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « majuscule » (0.6.12, demande de Fabienne) : frappe simulée
    /// dans le corps d'une rédaction, par Redaction.majuscule comme le fait
    /// Keys.onPressed — capitale en début de paragraphe et après « . », « ! »,
    /// « ? » ; rien au fil de la phrase, après « etc. », une initiale, des
    /// points de suspension ou un numéro en tête de ligne ; Ctrl+Z aussitôt
    /// après rend la minuscule ; réglage coupé, rien.
    function lancerScenarioMajuscule() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
        var n = null
        t.triggered.connect(function() {
            if (!n) {
                n = ouvrirRedaction(comptesConnus[0].compte)
                n.pret()
                t.start()
                return
            }
            var corps = n.corps
            corps.text = ""
            corps.cursorPosition = 0
            var touche = function(texte, annuler) {
                var ev = { text: texte, modifiers: 0,
                           matches: function(k) { return annuler === true && k === StandardKey.Undo } }
                if (n.majuscule(ev) || annuler)
                    return
                var p = corps.cursorPosition
                n.mefCorps.inserer(p, texte)
                corps.cursorPosition = p + texte.length
            }
            var taper = function(texte) {
                for (var i = 0; i < texte.length; ++i)
                    touche(texte[i])
            }
            taper("bonjour, tout va bien. merci pour le devis, etc. et le reste. à demain! oui? non... peut-être. "
                  + "J. dupont viendra en 2026. puis (fin.) suite. ç")
            touche("", true)
            taper("a\nnouvelle ligne\n1. premier point")
            reglages.majusculeAuto = false
            taper(". non")
            reglages.majusculeAuto = true
            var attendu = "Bonjour, tout va bien. Merci pour le devis, etc. et le reste. À demain! Oui? Non... peut-être. "
                    + "J. dupont viendra en 2026. Puis (fin.) Suite. ça\nNouvelle ligne\n1. premier point. non"
            // getText rend les fins de paragraphe en U+2029.
            var obtenu = corps.getText(0, corps.length).replace(/[\u2028\u2029]/g, "\n")
            console.log("scenario: texte obtenu " + JSON.stringify(obtenu))
            console.log("scenario: majuscule " + (obtenu === attendu ? "OK" : "ECHEC, attendu " + JSON.stringify(attendu)))
            Qt.quit()
        })
        t.start()
    }

    /// Scénario « disposition » (0.6.14) : « une barre » puis « deux étages »
    /// et retour — la bande des outils ne se montre qu'en deux étages, les
    /// boutons sont sur la barre ou sur la bande, jamais les deux ; la barre
    /// garde sa hauteur ; le choix est retenu dans les réglages.
    function lancerScenarioDisposition() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, hauteur: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | barre " + entete.height
                        + " | bande " + bandeOutils.height + " | réglage " + reglagesApparence.disposition)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n === 3) {
                suivi.hauteur = entete.height
                bilan("une barre par défaut", disposition === "barre" && !bandeOutils.visible && !deuxEtages)
                choisirDisposition("etages")
            } else if (suivi.n === 4) {
                bilan("deux étages : bande montrée, barre de même hauteur",
                      bandeOutils.visible && bandeOutils.height > 0 && entete.height === suivi.hauteur
                      && reglagesApparence.disposition === "etages")
                choisirDisposition("barre")
            } else if (suivi.n === 5) {
                bilan("retour à une barre", !bandeOutils.visible && entete.height === suivi.hauteur
                      && reglagesApparence.disposition === "barre")
                tic.stop()
                console.log("scenario: disposition " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « inverser-selection » (0.6.14, demande de Manu du 22/10) :
    /// dans la boîte de réception (trois messages au moins), un message
    /// choisi, la sélection inversée, puis rétablie ; tout, puis rien ; le
    /// menu d'un message en dix lignes, « Marquer comme lu » s'il y a un non
    /// lu parmi les choisis.
    function lancerScenarioInverserSelection() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, total: 0, premier: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | choisis " + uidsChoisis().length
                        + " sur " + modeleMessages.count)
        }
        var lignesDuMenu = function() {
            var n = 0
            for (var i = 0; i < menuMessage.count; ++i) {
                var e = menuMessage.itemAt(i)
                if (e && e.visible && e.text !== undefined && e.text.length > 0)
                    n += 1
            }
            return n
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                suivi.total = modeleMessages.count
                if (suivi.total < 3) {
                    bilan("il faut trois messages en boîte de réception", false)
                    suivi.n = 19
                    return
                }
                suivi.premier = modeleMessages.get(0).uid
                choisir(0, 0)
                inverserSelection()
                bilan("un choisi, inversé : les autres", uidsChoisis().length === suivi.total - 1
                      && fenetre.selection[suivi.premier] !== true)
                inverserSelection()
                bilan("inversé de nouveau : le premier seul", uidsChoisis().length === 1
                      && fenetre.selection[suivi.premier] === true)
                toutChoisir()
                inverserSelection()
                bilan("tout, inversé : aucun", uidsChoisis().length === 0)
                toutChoisir()
                boite.marquerLu(uidsChoisis().join(","), true)
                rafraichirListe()
            } else if (suivi.n === 9) {
                choisir(0, 0)
                choisir(1, Qt.ControlModifier)
                boite.marquerLu(String(modeleMessages.get(1).uid), false)
                rafraichirListe()
            } else if (suivi.n === 11) {
                menuMessage.popup()
                // Dix lignes en 0.6.14, onze avec « Catégoriser » (0.6.15).
                bilan("menu du message : " + lignesDuMenu() + " lignes, « " + menuMessage.itemAt(5).text
                      + " », « " + menuMessage.itemAt(6).text + " »",
                      lignesDuMenu() === 11 && menuMessage.unNonLu && menuMessage.itemAt(5).text === qsTr("Marquer comme lu"))
                menuMessage.close()
                choisir(0, 0)
            } else if (suivi.n === 13) {
                menuMessage.popup()
                bilan("un message lu : « " + menuMessage.itemAt(5).text + " »",
                      !menuMessage.unNonLu && menuMessage.itemAt(5).text === qsTr("Marquer comme non lu"))
                menuMessage.close()
                ouvrirBoitesUnifiees()
            } else if (suivi.n === 15) {
                var avant = uidsChoisis().length
                inverserSelection()
                bilan("Toutes les BàL : pas d'inversion (un seul message à la fois)",
                      vueUnifiee && uidsChoisis().length === avant)
                quitterBoitesUnifiees()
            } else if (suivi.n === 17) {
                bilan("interrupteur éteint : retour au dossier", !vueUnifiee && boite.dossierCourant === "INBOX")
            } else if (suivi.n === 18) {
                tic.stop()
                console.log("scenario: inverser-selection " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Menu du clic droit d'un champ de saisie (cf. MenuSaisie) : le
    /// gestionnaire posé dans le champ.
    function menuDuChamp(champ) {
        for (var i = 0; i < champ.data.length; ++i) {
            var o = champ.data[i]
            if (o && o.menu !== undefined && o.agir !== undefined)
                return o
        }
        return null
    }

    /// Entrée d'un menu par son libellé.
    function entreeDuMenu(menu, libelle) {
        for (var i = 0; i < menu.count; ++i) {
            var e = menu.itemAt(i)
            if (e && e.text === libelle)
                return e
        }
        return null
    }

    /// Scénario « menu-champ » (0.6.14, demande de Manu du 22/10) : le menu
    /// du clic droit du champ de recherche — Coller ce qu'un autre champ a
    /// copié, puis Tout sélectionner, Couper, Annuler ; Couper et Copier
    /// grisés sans sélection.
    function lancerScenarioMenuChamp() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var mot = "IIC OCEAN - ILOT RF1.1"
        var champ = colonneListe.champRecherche
        var h = menuDuChamp(champ)
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | champ " + JSON.stringify(champ.text)
                        + " | focus " + champ.activeFocus)
        }
        // Le clic droit, comme onTapped.
        var clicDroit = function() {
            h.debut = champ.selectionStart
            h.fin = champ.selectionEnd
            h.agi = false
            h.menu.popup()
        }
        var choisirEntree = function(libelle) {
            entreeDuMenu(h.menu, libelle).triggered()
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n === 3) {
                if (!h) {
                    bilan("pas de menu sur le champ de recherche", false)
                    suivi.n = 9
                    return
                }
                // Le presse-papier rempli par un autre champ.
                var source = Qt.createQmlObject('import QtQuick; TextInput {}', fenetre.contentItem)
                source.text = mot
                source.selectAll()
                source.copy()
                source.destroy()
                champ.text = ""
                clicDroit()
                bilan("menu ouvert, champ vide : Coller seul permis (Couper " + entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      + ", Copier " + entreeDuMenu(h.menu, qsTr("Copier")).enabled + ", Coller "
                      + entreeDuMenu(h.menu, qsTr("Coller")).enabled + ")",
                      h.menu.opened && !entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      && !entreeDuMenu(h.menu, qsTr("Copier")).enabled && entreeDuMenu(h.menu, qsTr("Coller")).enabled)
                choisirEntree(qsTr("Coller"))
            } else if (suivi.n === 4) {
                bilan("Coller : le texte copié dans la recherche, menu fermé, focus au champ",
                      champ.text === mot && !h.menu.visible && champ.activeFocus)
                clicDroit()
                choisirEntree(qsTr("Tout sélectionner"))
            } else if (suivi.n === 5) {
                bilan("Tout sélectionner", champ.selectedText === mot)
                clicDroit()
                bilan("sélection : Couper et Copier permis", entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      && entreeDuMenu(h.menu, qsTr("Copier")).enabled)
                choisirEntree(qsTr("Couper"))
            } else if (suivi.n === 6) {
                bilan("Couper : champ vide", champ.text === "")
                clicDroit()
                choisirEntree(qsTr("Annuler"))
            } else if (suivi.n === 7) {
                bilan("Annuler : le texte revient", champ.text === mot)
                champ.text = ""
                champ.textEdited()
            } else if (suivi.n === 8) {
                tic.stop()
                console.log("scenario: menu-champ " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « categories » (0.6.15, demande de Manu du 22/10) : dans la
    /// boîte de réception (deux messages au moins), une catégorie d'origine
    /// et une catégorie créée posées ; l'état du sous-menu « Catégoriser » ;
    /// la recherche « catégorie: » dans l'index et sur le serveur (KEYWORD :
    /// la preuve que le serveur les garde) ; tout retiré, et toujours retiré
    /// après relecture du serveur ; la catégorie créée retirée du catalogue,
    /// une catégorie d'origine renommée puis rétablie. Avec PARTAGE=1, une
    /// boîte partagée a son propre catalogue.
    function lancerScenarioCategories() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, motCle: "", premier: 0, second: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var mots = function(uid) {
            var i = indexDe(uid)
            return i >= 0 ? (modeleMessages.get(i).motsCles || "") : "?"
        }
        var catalogue = function(compte, chemin) {
            return JSON.parse(boite.categories(compte, chemin))
        }
        var noms = function(liste) {
            return liste.map(function(x) { return x.nom }).join(", ")
        }
        var recherche = qsTr("catégorie") + ":\"Devis à faire\""
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur })[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    bilan("il faut deux messages en boîte de réception", false)
                    suivi.n = 29
                    return
                }
                suivi.premier = modeleMessages.get(0).uid
                suivi.second = modeleMessages.get(1).uid
                var depart = catalogue(0, "")
                bilan("catalogue de départ : " + noms(depart.liste) + " | boîte " + depart.boite
                      + " | serveur " + depart.serveur,
                      depart.liste.length === 5 && depart.liste[0].nom === "Important"
                      && depart.liste[4].nom === "Peut attendre" && depart.boite === "INBOX" && !depart.refusees)
                choisir(0, 0)
                basculerCategorie("$label1", true)
                bilan("Important posée : « " + mots(suivi.premier) + " » | en-tête " + noms(categoriesAffichees),
                      mots(suivi.premier) === "$label1" && noms(categoriesAffichees) === "Important")
                suivi.motCle = boite.poserCategorie(0, "", "", "Devis à faire", "#0B8043")
                bilan("créée : " + suivi.motCle + " | " + noms(catalogue(0, "").liste),
                      suivi.motCle === "devis_a_faire" && catalogue(0, "").liste.length === 6)
                choisir(1, Qt.ControlModifier)
                basculerCategorie(suivi.motCle, true)
                bilan("posée sur les deux : « " + mots(suivi.premier) + " » / « " + mots(suivi.second) + " »",
                      mots(suivi.premier) === "$label1 devis_a_faire" && mots(suivi.second) === "devis_a_faire")
                var etat = etatCategories()
                bilan("sous-menu : la créée cochée, Important en partie (" + JSON.stringify(etat.porte) + ")",
                      etat.porte["devis_a_faire"] === "tous" && etat.porte["$label1"] === "certains"
                      && etat.liste.length === 6 && !etat.refusees)
                menuMessage.popup()
                bilan("menu du message : « Catégoriser » présent", entreeDuMenu(menuMessage, qsTr("Catégoriser")) !== null)
                menuMessage.close()
            } else if (suivi.n === 10) {
                lancerRecherche(recherche)
                bilan("recherche dans l'index : " + modeleMessages.count + " résultat(s)", modeleMessages.count === 2)
            } else if (suivi.n === 14) {
                var trouves = uidsServeur.slice().sort().join(",")
                bilan("recherche sur le serveur (KEYWORD) : " + JSON.stringify(uidsServeur),
                      trouves === [suivi.premier, suivi.second].sort().join(","))
                quitterRecherche()
            } else if (suivi.n === 16) {
                choisir(Math.max(0, indexDe(suivi.premier)), 0)
                effacerCategories()
                bilan("effacées du premier : « " + mots(suivi.premier) + " »", mots(suivi.premier) === "")
                choisir(Math.max(0, indexDe(suivi.second)), 0)
                basculerCategorie(suivi.motCle, false)
                bilan("retirée du second : « " + mots(suivi.second) + " »", mots(suivi.second) === "")
                bilan("retirée du catalogue", boite.retirerCategorie(0, "", suivi.motCle)
                      && catalogue(0, "").liste.length === 5)
                bilan("Important renommée « Urgent »", boite.poserCategorie(0, "", "$label1", "Urgent", "#D50000") === "$label1"
                      && catalogue(0, "").liste[0].nom === "Urgent")
                bilan("puis rétablie", boite.retirerCategorie(0, "", "$label1") && catalogue(0, "").liste[0].nom === "Important")
                bilan("refus : nom vide", boite.poserCategorie(0, "", "", "  ", "#0B8043") === "")
                if (c.partagees && c.partagees.length > 0) {
                    var chemin = "Shared/" + c.partagees[0]
                    var avant = catalogue(c.compte, chemin)
                    var cree = boite.poserCategorie(c.compte, chemin, "", "Boîte partagée", "#C239B3")
                    var apres = catalogue(c.compte, chemin)
                    bilan("boîte partagée : catalogue de " + avant.boite + " (" + noms(apres.liste) + "), le sien intact ("
                          + catalogue(0, "").liste.length + ")",
                          avant.boite.indexOf("Shared/") === 0 && cree === "boite_partagee" && apres.liste.length === 6
                          && catalogue(0, "").liste.length === 5)
                    boite.retirerCategorie(c.compte, chemin, cree)
                }
            } else if (suivi.n === 18) {
                // Les drapeaux relus du serveur : rien n'y est resté.
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 21) {
                bilan("après relecture du serveur : « " + mots(suivi.premier) + " » / « " + mots(suivi.second) + " »",
                      mots(suivi.premier) === "" && mots(suivi.second) === "")
                var fin = catalogue(0, "")
                console.log("scenario: catalogue final " + noms(fin.liste) + " | serveur " + fin.serveur)
            } else if (suivi.n >= 22) {
                tic.stop()
                console.log("scenario: categories " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénarios « liste-categories », « menu-categories » et
    /// « gerer-categories » (0.6.15) : des catégories posées sur les
    /// messages de la boîte de réception, puis la liste, le sous-menu
    /// « Catégoriser » ouvert ou le dialogue « Gérer les catégories », pour
    /// une capture d'écran (--capture), sans quitter.
    function lancerScenarioCapturesCategories() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
        t.triggered.connect(function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            var t2 = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
            t2.triggered.connect(function() {
                var devis = boite.poserCategorie(0, "", "", "Devis à faire", "#0B8043")
                var compta = boite.poserCategorie(0, "", "", "Comptabilité", "#0078D4")
                var poser = function(rang, motCle) {
                    if (rang >= modeleMessages.count)
                        return
                    choisir(rang, 0)
                    basculerCategorie(motCle, true)
                }
                poser(0, "$label1")
                poser(0, devis)
                poser(1, devis)
                poser(2, "$label2")
                poser(3, compta)
                choisir(0, 0)
                if (essai.scenario === "menu-categories") {
                    menuMessage.popup(colonneListe.x + 120, entete.height + bandeOutils.height + 90)
                    for (var i = 0; i < menuMessage.count; ++i) {
                        var e = menuMessage.itemAt(i)
                        if (e && e.text === qsTr("Catégoriser"))
                            e.triggered()
                    }
                } else if (essai.scenario === "gerer-categories") {
                    gererCategories()
                }
            })
            t2.start()
        })
        t.start()
    }

    /// Scénarios « menu-affichage », « menu-message » et « menu-saisie »
    /// (0.6.14) : le menu ouvert pour une capture d'écran (--capture), sans
    /// quitter.
    function lancerScenarioMenuOuvert() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
        t.triggered.connect(function() {
            if (essai.scenario === "menu-affichage") {
                menuAffichage.popup(Math.max(0, fenetre.width - 330), entete.height)
                menuAffichage.itemAt(1).triggered()
                return
            }
            if (essai.scenario === "menu-saisie") {
                var champ = colonneListe.champRecherche
                var h = menuDuChamp(champ)
                champ.text = "IIC OCEAN"
                champ.forceActiveFocus()
                champ.select(0, 3)
                h.debut = 0
                h.fin = 3
                h.menu.popup(champ, 40, champ.height - 4)
                return
            }
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            var t2 = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
            t2.triggered.connect(function() {
                choisir(0, 0)
                menuMessage.popup(colonneListe.x + 120, entete.height + bandeOutils.height + 90)
            })
            t2.start()
        })
        t.start()
    }

    /// Scénario « lu-differe » (0.6.9, demande de Gwenaëlle du 20/10) : le
    /// marquage « lu » attend le délai réglé, ne vise pas un message quitté
    /// avant, et n'a jamais lieu sur « Jamais ». La boîte de réception doit
    /// compter deux messages.
    function lancerScenarioLuDiffere() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, a: 0, b: 0, echecs: 0 }
        var lu = function(uid) {
            var i = indexDe(uid)
            var l = JSON.parse(boite.messagesParUid(String(uid)))
            return "liste " + (i >= 0 ? modeleMessages.get(i).lu : null) + " | index " + (l.length > 0 ? l[0].lu : null)
        }
        var bilan = function(texte, uid, attendu) {
            var etat = lu(uid)
            var bon = etat === "liste " + attendu + " | index " + attendu
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | " + etat)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    console.log("scenario: ECHEC lu-differe : " + modeleMessages.count + " message(s) en boîte de réception, il en faut deux")
                    suivi.echecs += 1
                    suivi.n = 39
                    return
                }
                suivi.a = modeleMessages.get(0).uid
                suivi.b = modeleMessages.get(1).uid
                boite.marquerLu(suivi.a + "," + suivi.b, false)
                rafraichirListe()
                reglages.delaiLu = 3
            } else if (suivi.n === 9) {
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 11) {
                bilan("A : 3 s, encore non lu après 2 s", suivi.a, false)
            } else if (suivi.n === 14) {
                bilan("A : 3 s, lu après 5 s", suivi.a, true)
            } else if (suivi.n === 15) {
                choisir(indexDe(suivi.b), 0)
            } else if (suivi.n === 16) {
                // B quitté avant la fin du délai.
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 21) {
                bilan("B : quitté au bout d'une seconde, toujours non lu", suivi.b, false)
                reglages.delaiLu = -1
            } else if (suivi.n === 22) {
                choisir(indexDe(suivi.b), 0)
            } else if (suivi.n === 28) {
                bilan("B : « Jamais », non lu après 6 s", suivi.b, false)
                reglages.delaiLu = 0
                boite.marquerLu(String(suivi.a), false)
                rafraichirListe()
            } else if (suivi.n === 29) {
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 31) {
                bilan("A : « À l'affichage », lu aussitôt", suivi.a, true)
                // Le serveur : un autre dossier, puis la boîte de réception
                // relue — ses drapeaux reviennent du serveur.
                fenetre.selection = ({})
                uidCourant = 0
                ouvrirDossier(c.compte, "Essais")
            } else if (suivi.n === 33) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 35) {
                boite.actualiser()
            } else if (suivi.n === 40) {
                bilan("serveur : A lu", suivi.a, true)
                bilan("serveur : B non lu", suivi.b, false)
                console.log("scenario: lu-differe, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
                suivi.n = 44
            } else if (suivi.n >= 45) {
                tic.stop()
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « adresses-envoi » (0.6.7) : les autres adresses d'envoi d'un
    /// compte dans le champ « De », celle d'une réponse ou d'un brouillon
    /// reprise, l'identité du compte reprise faute de la sienne. Sans réseau
    /// après la connexion.
    function etapeScenarioAdressesEnvoi() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var c = comptesConnus[0]
        var lues = lireAdressesEnvoi("Partagee@Exemple.fr ; alias@exemple.fr, pas-une-adresse ; partagee@exemple.fr "
                                     + c.adresse, c.adresse)
        bilan("saisie lue : " + JSON.stringify(lues),
              JSON.stringify(lues.adresses) === JSON.stringify(["Partagee@Exemple.fr", "alias@exemple.fr"])
              && JSON.stringify(lues.invalides) === JSON.stringify(["pas-une-adresse"]))
        poserAdressesEnvoi(c.adresse, lues.adresses)
        poserIdentite(c.adresse, { nom: "Personne Essai", signature: "Signature du compte", html: "",
                                   nouveaux: true, reponses: true })
        var de = expediteursConnus.filter(function(e) { return e.compte === c.compte })
                                  .map(function(e) { return e.adresse })
        bilan("champ « De » : " + de.join(" | "),
              JSON.stringify(de) === JSON.stringify([c.adresse, "Partagee@Exemple.fr", "alias@exemple.fr"]))
        var r = ouvrirRedaction(c.compte)
        r.pret()
        var v = r.contenu()
        bilan("nouveau message : " + v.de + " | " + v.nom, v.de === c.adresse && v.nom === "Personne Essai")
        // Réponse à un message adressé à la boîte partagée (casse différente).
        r.remplir({ mode: "repondre", de: "partagee@exemple.fr", a: "client@exemple.org", objet: "RE : essai" })
        v = r.contenu()
        bilan("réponse depuis la partagée : " + v.de + " | " + v.nom + " | signature du compte "
              + (r.htmlActuel().indexOf("Signature du compte") >= 0 || v.texte.indexOf("Signature du compte") >= 0),
              v.de === "Partagee@Exemple.fr" && v.nom === "Personne Essai")
        // Une adresse qui n'est pas du compte : le champ ne bouge pas.
        r.remplir({ mode: "brouillon", de: "inconnue@exemple.fr", a: "client@exemple.org" })
        bilan("adresse étrangère ignorée : " + r.contenu().de, r.contenu().de === "Partagee@Exemple.fr")
        r.conteneur.fermerRedaction()
        // Son identité propre l'emporte sur celle du compte.
        poserIdentite("alias@exemple.fr", { nom: "Accueil", signature: "", html: "", nouveaux: true, reponses: true })
        var r2 = ouvrirRedaction(c.compte)
        r2.remplir({ mode: "brouillon", de: "alias@exemple.fr", a: "client@exemple.org" })
        bilan("identité propre : " + r2.contenu().nom, r2.contenu().nom === "Accueil")
        r2.conteneur.fermerRedaction()
        // « Nom et signature » sur une adresse sans identité : proposée celle du
        // compte, rien d'écrit si l'on n'y touche pas.
        dlgIdentite.ouvrir(c.compte)
        dlgIdentite.choisirAdresse("Partagee@Exemple.fr")
        var nom = dlgIdentite.valeurs().nom
        dlgIdentite.accept()
        bilan("nom et signature repris du compte : " + nom + " | rien d'écrit " + !aUneIdentite("Partagee@Exemple.fr"),
              nom === "Personne Essai" && !aUneIdentite("Partagee@Exemple.fr"))
        // « Modifier le compte… » : le champ montre la liste, une saisie vide la retire.
        modifierCompte(c.compte)
        var affiche = dlgCompte.champAdressesEnvoi.text
        dlgCompte.champAdressesEnvoi.text = ""
        dlgCompte.accept()
        bilan("modifier le compte : « " + affiche + " » puis retirées -> " + JSON.stringify(adressesEnvoi(c.adresse)),
              affiche === "Partagee@Exemple.fr ; alias@exemple.fr" && adressesEnvoi(c.adresse).length === 0)
        console.log("scenario: adresses-envoi, " + (echecs === 0 ? "tout OK" : echecs + " échec(s)"))
        Qt.quit()
    }

    /// Scénario « partagees » (0.6.8) : la boîte de réception de deux@ est
    /// partagée avec un@ par le Dovecot d'essai. Seul compte du profil, un@ la
    /// voit proposée dans « De » ; deux@ aussi dans le profil, elle n'y est
    /// pas reprise sous un@ — elle a déjà sa ligne.
    function lancerScenarioPartagees() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var partagee = "deux@essai.test"
        var deuxComptes = identifiantsEssai.utilisateur2.length > 0
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur })[0]
            var pret = c !== undefined && c.partagees.length > 0 && (!deuxComptes || comptesConnus.length === 2)
            if (!pret && suivi.n < 30)
                return
            tic.stop()
            var de = function() {
                return expediteursConnus.filter(function(e) { return e.compte === c.compte })
                                        .map(function(e) { return e.adresse })
            }
            if (!pret) {
                bilan("rien de reconnu en 30 s : " + JSON.stringify(comptesConnus), false)
            } else if (deuxComptes) {
                bilan("reconnue : " + JSON.stringify(c.partagees), JSON.stringify(c.partagees) === JSON.stringify([partagee]))
                var lignes = expediteursConnus.filter(function(e) { return e.adresse === partagee })
                bilan("deux comptes, « De » de " + c.adresse + " : " + de().join(" | ") + " ; lignes de "
                      + partagee + " : " + lignes.length,
                      JSON.stringify(de()) === JSON.stringify([c.adresse]) && lignes.length === 1)
            } else {
                bilan("reconnue : " + JSON.stringify(c.partagees), JSON.stringify(c.partagees) === JSON.stringify([partagee]))
                bilan("champ « De » : " + de().join(" | "), JSON.stringify(de()) === JSON.stringify([c.adresse, partagee]))
                // Les saisies s'ajoutent ; la partagée saisie aussi n'est pas répétée.
                poserAdressesEnvoi(c.adresse, ["alias@exemple.fr", "Deux@Essai.test"])
                bilan("avec les saisies : " + de().join(" | "),
                      JSON.stringify(de()) === JSON.stringify([c.adresse, partagee, "alias@exemple.fr"]))
                var propres = autresAdresses(c.compte)
                bilan("adresses données à la préparation d'une réponse : " + JSON.stringify(propres),
                      JSON.stringify(propres) === JSON.stringify([partagee, "alias@exemple.fr"]))
                var r = ouvrirRedaction(c.compte)
                r.pret()
                r.remplir({ mode: "repondre", de: partagee, a: "client@exemple.org", objet: "RE : essai" })
                bilan("réponse depuis la partagée : " + r.contenu().de, r.contenu().de === partagee)
                r.conteneur.fermerRedaction()
                modifierCompte(c.compte)
                bilan("modifier le compte : " + JSON.stringify(dlgCompte.partagees) + " | « "
                      + dlgCompte.champAdressesEnvoi.text + " »",
                      JSON.stringify(dlgCompte.partagees) === JSON.stringify([partagee])
                      && dlgCompte.champAdressesEnvoi.text === "alias@exemple.fr ; Deux@Essai.test")
                dlgCompte.reject()
            }
            console.log("scenario: partagees, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
            Qt.quit()
        })
        tic.start()
    }
}
