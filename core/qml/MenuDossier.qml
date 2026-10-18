// SPDX-License-Identifier: GPL-3.0-or-later
// Menu contextuel d’un dossier de l’arborescence.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuDossier { id: menuDossier } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

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
    function ouvrir(c, ch, f, m, n, sel, item, x, y) {
        compte = c; chemin = ch; favori = f; masque = m; nom = n; selectionnable = sel
        if (item)
            popup(item, x, y)
        else
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
