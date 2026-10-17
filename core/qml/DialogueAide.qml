// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Aide de MMail » (F1).
//
// Sorti de Main.qml tel quel (0.5.16, découpage) : les noms employés sans
// être déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « DialogueAide { id: dlgAide } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

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
                + "Menu « Comptes » : ajouter, modifier ou retirer un compte. À l'ajout, le serveur est "
                + "cherché d'après l'adresse, si son domaine publie sa configuration. Un lien de "
                + "configuration, reçu de votre administrateur, ajoute un ou plusieurs comptes "
                + "sans rien saisir ; il ne sert qu'une fois. Retirer un compte efface de ce "
                + "poste son index et son mot de passe mémorisé ; rien n'est supprimé sur le serveur. "
                + "« Ajouter un compte Microsoft (Office 365, Outlook.com)… » : la connexion se fait chez "
                + "Microsoft, dans le navigateur ; si votre organisation l'exige, MMail donne le lien que "
                + "son administrateur ouvre une fois pour l'autoriser. "
                + "« Ajouter un compte Google (Gmail)… » : de même chez Google ; ce menu n'apparaît "
                + "qu'une fois MMail publié par Google — avant, seuls les comptes déclarés pour les "
                + "essais pourraient s'y connecter. "
                + "« Modifier un compte » (aussi au clic droit sur le compte, « Modifier ce compte… ») : "
                + "serveur de lecture, serveur d'envoi, mot de passe — laissé vide, il ne change pas —, "
                + "et de là, nom et signature. Clic droit sur un compte : aussi se connecter, se "
                + "déconnecter, oublier le mot de passe. "
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
                + "Ctrl+E, puis Entrée : dans le dossier actif ou dans toutes les boîtes. Le texte "
                + "entier des messages est cherché sur le serveur, même dans les dossiers pas encore "
                + "ouverts sur ce poste ; dans toutes les boîtes, les résultats arrivent dossier par "
                + "dossier, la barre d'état suit la progression. Pendant une "
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
                + "Ctrl+clic et Maj+clic sélectionnent plusieurs messages. Au survol d'une ligne, "
                + "la corbeille en bout de ligne y envoie ce message sans l'ouvrir.<br><br>"
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
                + "déposer, protégé une minute. L'objet et l'expéditeur se sélectionnent aussi "
                + "(Ctrl+C) ; clic droit : « Copier l'adresse de l'expéditeur », « Copier l'objet ». "
                + "Sous Android, l'appui long choisit un mot et ouvre "
                + "la barre d'Android (Copier, Tout sélectionner) ; un toucher la referme.<br><br>"
                + "<b>Pièces jointes</b><br>"
                + "Une ligne par pièce sous l'en-tête du message : son type, son nom, sa taille, "
                + "« Ouvrir » et « Enregistrer sous… », qui propose le dernier dossier choisi. "
                + "Clic droit sur une pièce : « Ouvrir avec… » un "
                + "autre logiciel, pour cette fois ou pour tous les fichiers du même type (menu "
                + "Affichage, « Logiciels d'ouverture… », pour les revoir) ; un type que le système ne "
                + "sait pas ouvrir fait choisir le logiciel, retenu d'emblée ; « Joindre au message en "
                + "cours » ou « Joindre à un nouveau message ». Un programme ou un script ne s'ouvre "
                + "pas depuis MMail, il s'enregistre.")
        }
    }
}
