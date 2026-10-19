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
}
